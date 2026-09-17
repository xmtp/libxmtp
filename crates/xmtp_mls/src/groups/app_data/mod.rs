//! Group state stored in the OpenMLS AppData dictionary.
//!
//! This module is the bridge between the per-field intent handlers in
//! `mls_sync` and the OpenMLS app data dictionary. It is intentionally
//! `pub(crate)` — there is no public API for reading or writing arbitrary
//! components. The existing per-field helpers (`update_group_name`,
//! `update_admin_list_action`, …) keep their signatures and route through
//! the appropriate sub-module here.

// `pub` (rather than `pub(crate)`) so the public `GroupError::ComponentSource`
// variant in `crate::groups::error` doesn't trip the `private_interfaces`
// lint. The functions inside the module remain `pub(crate)`, so the wider
// crate ecosystem still can't read or write arbitrary components — only
// `GroupError` consumers see the error type.
#[allow(
    dead_code,
    reason = "Retained for removal with migration code in Task 5"
)]
pub(crate) mod bootstrap_validator;
pub mod component_source;
pub mod migration;
pub(crate) mod sender_intents;
pub(crate) mod typed_facade;

use std::collections::BTreeMap;

use openmls::{
    component::ComponentData,
    framing::{MlsMessageOut, ProcessedMessage, ProtocolMessage},
    group::{
        AppDataUpdates, MlsGroup as OpenMlsGroup, ProcessMessageError, ProposalError,
        ResolveAppDataCommitError,
    },
    messages::proposals::{AppDataUpdateOperation, Proposal},
    // `CommitMessageBundle` lives in `prelude` because the natural path
    // (`openmls::group::commit_builder`) is private to the openmls crate.
    // Re-importing through prelude is the only public path.
    prelude::{CommitMessageBundle, ProcessedMessageContent},
    storage::OpenMlsProvider,
};
use xmtp_mls_common::app_data::{component_id::ComponentId, component_registry::ComponentRegistry};

use self::component_source::{
    ComponentSourceError, apply_app_data_update_payload, read_from_app_data_dict,
};
use crate::groups::validated_commit::LibXMTPVersion;

#[cfg(any(test, feature = "test-utils"))]
tokio::task_local! {
    /// Test-only override returned by [`load_component_registry`].
    /// Stored as a tokio task-local (rather than a thread-local) so the
    /// scope survives task migration across worker threads under
    /// `multi_thread` runtimes.
    pub static TEST_REGISTRY_OVERRIDE: ComponentRegistry;
}

/// Error returned by [`process_message_with_app_data`].
///
/// Wraps both an OpenMLS [`ProcessMessageError`] (for the underlying
/// `process_message` failure modes) and a [`ComponentSourceError`] (for
/// failures that happen while we decode an incoming `AppDataUpdate`
/// payload). Splitting them keeps "the message was bad in OpenMLS terms"
/// distinct from "we couldn't decode an AppData payload" so callers can
/// log / retry / surface them differently.
#[derive(Debug, thiserror::Error)]
pub enum ProcessMessageWithAppDataError<StorageError: std::error::Error> {
    /// Standard OpenMLS processing failure (decryption, validation, …).
    #[error(transparent)]
    OpenMls(#[from] ProcessMessageError<StorageError>),
    /// Failed to decode an incoming `AppDataUpdate` payload via
    /// [`apply_app_data_update_payload`]. Almost always indicates a
    /// malformed proposal from a peer (or a wire-format mismatch with a
    /// future version we don't understand yet).
    ///
    /// **Not retryable.** Decode failures are deterministic over the
    /// exact bytes on the wire, so retrying the same message will fail
    /// the same way. `GroupMessageProcessingError::is_retryable` and
    /// `commit_result` treat this as a terminal wire-format violation
    /// (mapped to `CommitResult::Invalid`).
    #[error("failed to decode incoming AppDataUpdate payload: {0}")]
    AppDataDecode(#[from] ComponentSourceError),
    /// The group's committed `MIN_SUPPORTED_PROTOCOL_VERSION` floor
    /// exceeds this client's version. Surfaced *before* any
    /// `AppDataUpdate` payload is dispatched, so a client below the
    /// floor (most commonly after an app downgrade — pausing normally
    /// happens at the floor-bump commit itself, but a downgraded
    /// client never processed one) pauses the group instead of
    /// rejecting a commit it cannot interpret. Rejecting here is what
    /// forks a group: peers above the floor accept the commit and
    /// advance without us.
    ///
    /// Converted to `CommitValidationError::ProtocolVersionTooLow` at
    /// the `mls_sync` boundary so the existing pause machinery
    /// (`set_group_paused`, held cursor, reprocess-on-upgrade) applies
    /// unchanged.
    #[error(
        "group's minimum supported protocol version {min_version} exceeds this client's version {own_version}"
    )]
    ProtocolVersionTooLow {
        min_version: String,
        own_version: String,
    },
    /// Staging an app-data commit failed after we interpreted its
    /// proposals (`OpenMlsGroup::resolve_app_data_commit`). Carries the
    /// same staging failure modes a commit without AppDataUpdate
    /// proposals would surface from `process_message` directly.
    #[error("failed to stage app-data commit: {0}")]
    ResolveAppDataCommit(#[from] ResolveAppDataCommitError),
}

/// Walk a stream of `(ComponentId, &AppDataUpdateOperation)` tuples and
/// produce the resulting [`AppDataUpdates`] the commit builder / message
/// processor wants.
///
/// Accumulates per-component state in a local [`BTreeMap`]
/// (`Some(bytes)` for an Update, `None` for a Remove) so that two proposals
/// targeting the same component inside one batch chain correctly — the
/// second one's `apply_app_data_update_payload` call sees the first
/// proposal's effect as its `old_value`. The migration PR's bootstrap
/// commit emits multiple `AppDataUpdate(COMPONENT_REGISTRY, ...)` proposals
/// back-to-back and would otherwise lose all but the last one.
///
/// Returns `Ok(None)` when the iterator yields no proposals (an empty
/// `BTreeMap::new()` is heap-free, so the common zero-proposal case costs
/// essentially nothing).
pub(crate) fn accumulate_app_data_updates<'a, I>(
    mls_group: &OpenMlsGroup,
    proposals: I,
) -> Result<Option<AppDataUpdates>, ComponentSourceError>
where
    I: IntoIterator<Item = (openmls::component::ComponentId, &'a AppDataUpdateOperation)>,
{
    let mut in_batch: BTreeMap<openmls::component::ComponentId, Option<Vec<u8>>> = BTreeMap::new();

    // Load the pre-commit registry once. It supplies the
    // `ComponentType` tag the type-aware dispatcher in
    // `apply_app_data_update_payload` uses when an unknown component id
    // arrives. Registry updates that land in the same commit don't
    // retroactively change this snapshot — the typed path would need
    // an in-batch registry overlay to handle the corner case where the
    // very same commit both registers a new component and writes to
    // it.
    let registry = load_component_registry(mls_group)?;

    for (openmls_id, operation) in proposals {
        let xmtp_id = ComponentId::from(openmls_id);
        match operation {
            AppDataUpdateOperation::Update(payload) => {
                // Resolve `old_value` from in-batch state first; fall back
                // to the pre-commit dict only if no earlier proposal in
                // this batch touched the same component. The match borrows
                // from `in_batch` only for the duration of the arm body —
                // `apply_app_data_update_payload` returns an owned `Vec<u8>`
                // that outlives the borrow, so the follow-up `insert` is
                // legal without cloning the prior bytes.
                let new_value = match in_batch.get(&openmls_id) {
                    Some(Some(bytes)) => apply_app_data_update_payload(
                        xmtp_id,
                        payload.as_slice(),
                        Some(bytes.as_slice()),
                        &registry,
                    ),
                    Some(None) => {
                        apply_app_data_update_payload(xmtp_id, payload.as_slice(), None, &registry)
                    }
                    None => {
                        let from_dict = read_from_app_data_dict(xmtp_id, mls_group);
                        apply_app_data_update_payload(
                            xmtp_id,
                            payload.as_slice(),
                            from_dict.as_deref(),
                            &registry,
                        )
                    }
                }
                .inspect_err(|e| {
                    tracing::warn!(
                        component_id = %xmtp_id,
                        error = %e,
                        "Failed to apply AppDataUpdate payload"
                    );
                })?;
                in_batch.insert(openmls_id, Some(new_value));
            }
            AppDataUpdateOperation::Remove => {
                // Maps straight to `updater.remove(&id)` below — the
                // component impl's `apply_update_payload` is never
                // consulted for `Remove`, so component-level Remove
                // rejections (e.g. the whole-registry Remove ban in
                // `ComponentRegistryComponent::expand_to_changes`) are
                // enforced during commit validation
                // (`ValidatedCommit::from_staged_commit`), not
                // re-checked here. That's sound because both current
                // commit-processing paths validate before applying.
                in_batch.insert(openmls_id, None);
            }
        }
    }

    if in_batch.is_empty() {
        return Ok(None);
    }

    let mut updater = mls_group.app_data_dictionary_updater();
    for (id, value) in in_batch {
        match value {
            Some(bytes) => updater.set(ComponentData::from_parts(id, bytes.into())),
            None => updater.remove(&id),
        }
    }
    Ok(updater.changes())
}

/// AppDataUpdate-aware wrapper around [`OpenMlsGroup::process_message`].
///
/// `OpenMlsGroup::process_message` returns a commit covering
/// `AppDataUpdate` proposals as
/// [`ProcessedMessageContent::UnresolvedAppDataCommit`] — the application
/// is required to interpret the proposals, compute the resulting
/// [`AppDataUpdates`], and resume staging. This wrapper does that dance:
///
/// 1. `process_message` as usual.
/// 2. On an unresolved app-data commit, hand its (already
///    reference-resolved) `AppDataUpdate` proposals to
///    [`accumulate_app_data_updates`] to compute the resulting
///    [`AppDataUpdates`].
/// 3. Call `resolve_app_data_commit` with those updates, staging the
///    commit and yielding a regular `StagedCommitMessage`.
///
/// Callers replace `mls_group.process_message(provider, message)` with
/// `process_message_with_app_data(mls_group, provider, message)` and get
/// back the same `ProcessedMessage` they used to; the
/// `UnresolvedAppDataCommit` variant never escapes this function.
/// `own` is the client's parsed pkg_version (threaded from the caller's
/// context rather than read from a constant so cross-version tests can
/// override it).
pub(crate) fn process_message_with_app_data<Provider: OpenMlsProvider>(
    mls_group: &mut OpenMlsGroup,
    provider: &Provider,
    message: impl Into<ProtocolMessage>,
    own: &LibXMTPVersion,
) -> Result<ProcessedMessage, ProcessMessageWithAppDataError<Provider::StorageError>> {
    let processed = mls_group.process_message(provider, message)?;

    // PAUSE BEFORE PARSE: every commit on a below-floor group must pause
    // (held cursor, `set_group_paused`), never process — above-floor
    // peers accept it and advance, so rejecting instead of pausing forks
    // the group. Checked here, *after* `process_message` authenticated
    // the message, so the pause decision is never driven by
    // unauthenticated framing bits a sender could spoof to freeze
    // application-message processing on a below-floor group. For an
    // `UnresolvedAppDataCommit` this runs before the proposals are
    // interpreted below — the commit's app-data payloads may use wire
    // formats introduced after this version. It reads ONLY the
    // pre-commit dict — committed, already-validated state — and must
    // never consider the commit's own proposals: a same-commit floor
    // bump has not passed the super-admin policy check yet, and pausing
    // on unvalidated input would let any member freeze the group for
    // everyone. Application messages are unaffected; standalone
    // proposals get the same floor-first hold in `mls_sync`'s
    // `ProposalMessage` arm, which also keeps a below-floor client from
    // ever advancing past a stored-by-peers proposal that a later commit
    // references. (Staging a plain commit inside `process_message`
    // interprets no app-data payloads; nothing is merged until
    // `merge_staged_commit`.)
    let is_commit = matches!(
        processed.content(),
        ProcessedMessageContent::StagedCommitMessage(_)
            | ProcessedMessageContent::UnresolvedAppDataCommit(_)
    );
    if is_commit && let Some(min_version) = committed_floor_exceeding(mls_group, own) {
        return Err(ProcessMessageWithAppDataError::ProtocolVersionTooLow {
            min_version,
            own_version: own.to_string(),
        });
    }

    let unresolved = match processed.content() {
        ProcessedMessageContent::UnresolvedAppDataCommit(unresolved) => unresolved,
        _ => return Ok(processed),
    };

    // Collect owned (id, operation) tuples so the iterator doesn't keep
    // `processed` borrowed — `resolve_app_data_commit` consumes it below.
    // Proposals committed by reference are already resolved from the
    // proposal store by `process_message`.
    let collected: Vec<(openmls::component::ComponentId, AppDataUpdateOperation)> = unresolved
        .app_data_update_proposals()
        .map(|p| (p.component_id(), p.operation().clone()))
        .collect();
    let iter = collected.iter().map(|(id, op)| (*id, op));
    let app_data_updates = accumulate_app_data_updates(mls_group, iter)?;

    Ok(mls_group.resolve_app_data_commit(provider, processed, app_data_updates)?)
}

/// Stage a standalone `AppDataUpdate(Update)` proposal AND a follow-up
/// commit that references it from the OpenMLS proposal store.
///
/// This is the shape XIP §1.5.2 / §3.4 prescribes for post-migration
/// metadata updates: separate proposal and commit MLS messages, so the
/// commit message carries only a `ProposalRef` (hash) rather than the
/// AppDataUpdate payload bytes. Smaller commits, smaller proposal-
/// processing hot paths, identical end state.
///
/// Returns `(proposal_msg, commit_bundle)`. The caller MUST publish
/// `proposal_msg` and `commit_bundle.commit()` together in one
/// `payloads_to_publish` batch (proposal first) so receivers see the
/// proposal in the same network round trip before processing the
/// commit that references it.
///
/// Call this inside `generate_prepared_commit` and an outer state transaction.
/// Store the exact attempt and staged commit in that transaction.
pub(crate) fn stage_app_data_propose_and_commit<Provider: OpenMlsProvider>(
    mls_group: &mut OpenMlsGroup,
    provider: &Provider,
    signer: &impl openmls_traits::signatures::Signer,
    component_id: ComponentId,
    payload: Vec<u8>,
) -> Result<(MlsMessageOut, CommitMessageBundle), GroupAppDataError<Provider::StorageError>> {
    let (mut proposals, bundle) = stage_app_data_proposals_and_commit(
        mls_group,
        provider,
        signer,
        vec![(component_id, payload)],
    )?;
    Ok((proposals.remove(0), bundle))
}

/// Stage all component updates in one commit, using the same pre-commit state.
pub(crate) fn stage_app_data_proposals_and_commit<Provider: OpenMlsProvider>(
    mls_group: &mut OpenMlsGroup,
    provider: &Provider,
    signer: &impl openmls_traits::signatures::Signer,
    updates: Vec<(ComponentId, Vec<u8>)>,
) -> Result<(Vec<MlsMessageOut>, CommitMessageBundle), GroupAppDataError<Provider::StorageError>> {
    // Lazy-batching: we deliberately do NOT block on pre-existing
    // pending proposals. This helper queues a new `AppDataUpdate` then
    // commits via `consume_proposal_store(true)`, sweeping whatever
    // else is in the store — concurrent `AppDataUpdate`s (accumulated
    // into the dict by step 2), leaf-node `Update`s, membership
    // `Add` / `Remove` / `SelfRemove`, PSK, etc. — all into one
    // commit. That's the design: minimize commit count, let the
    // producers of those proposals decide if they need to force their
    // own commit (because they want to send a message right now or
    // grant access immediately). MLS guarantees consistent state
    // convergence on the wire regardless of which commit body carries
    // which proposal; the sender's intent ledger may carry less
    // information than the on-wire commit, but the producer of each
    // folded-in proposal already accepted that outcome by leaving it
    // pending instead of issuing its own commit.
    let mut proposals = Vec::with_capacity(updates.len());
    for (component_id, payload) in updates {
        let operation = AppDataUpdateOperation::Update(payload.into());
        let (proposal, _) = mls_group
            .propose_app_data_update(provider, signer, component_id.as_u16(), operation)
            .map_err(GroupAppDataError::Propose)?;
        proposals.push(proposal);
    }

    // Step 2: compute the per-component dict updates by sweeping every
    // `AppDataUpdate` proposal currently in the store. The store may
    // contain pre-existing `AppDataUpdate` proposals queued by earlier
    // intents (e.g. two members each issuing a `GROUP_MEMBERSHIP`
    // update, or a queued `update_group_name` that hasn't been
    // committed yet); the accumulator chains them via the in-batch
    // map so the final dict bytes match what
    // `process_message_with_app_data` produces on the receive side.
    //
    // Non-`AppDataUpdate` proposals (Add/Remove/Update/PSK/etc.) also
    // get swept by `consume_proposal_store(true)` at step 3 — they
    // ride into the commit natively via OpenMLS and don't contribute
    // to AppData dict updates, so we don't include them in this
    // iteration.
    //
    // Failure mode if OpenMLS ever changes `consume_proposal_store(true)`'s
    // sweep behavior or `pending_proposals()` ordering: sender and
    // receiver compute different final dict bytes for the same
    // component, the commit's confirmation tag mismatches, and
    // receivers reject the commit with `WrongConfirmationTag`. The E2E
    // tests in `groups/tests/test_proposals.rs` under the AppDataUpdate
    // section will fail loudly on any OpenMLS bump that breaks this.
    let pending_tuples: Vec<(openmls::component::ComponentId, AppDataUpdateOperation)> = mls_group
        .pending_proposals()
        .filter_map(|q| match q.proposal() {
            Proposal::AppDataUpdate(p) => Some((p.component_id(), p.operation().clone())),
            _ => None,
        })
        .collect();
    let pending_iter = pending_tuples.iter().map(|(id, op)| (*id, op));
    let app_data_updates =
        accumulate_app_data_updates(mls_group, pending_iter).inspect_err(|e| {
            tracing::error!(
                error = %e,
                "Failed to compute AppDataUpdates for standalone propose+commit"
            );
        })?;

    // Step 3: build a commit that consumes the proposal store (picks up
    // the just-queued proposal). No `add_proposal` call — the proposal
    // is encoded as a `ProposalRef` because it comes from the store, not
    // from inline staging.
    let mut stage = mls_group
        .commit_builder()
        .consume_proposal_store(true)
        .load_psks(provider.storage())?;
    stage.with_app_data_dictionary_updates(app_data_updates);

    let bundle = stage
        .build(provider.rand(), provider.crypto(), signer, |_| true)?
        .stage_commit(provider)?;

    Ok((proposals, bundle))
}

/// Errors surfaced by [`stage_app_data_propose_and_commit`].
///
/// Wrapped into `GroupError` via the `#[from]` impl on
/// `GroupError::AppDataCommit` so the structured source is preserved at
/// the call site (no string-flattening). The `pub(crate)` visibility
/// matches the helper itself; the variant is only re-exported through
/// the public `GroupError` enum.
#[derive(Debug, thiserror::Error)]
pub enum GroupAppDataError<StorageError: std::error::Error> {
    /// `propose_app_data_update(…)` failed when staging the standalone
    /// proposal that precedes the commit.
    #[error("propose error: {0}")]
    Propose(#[from] ProposalError<StorageError>),
    /// `commit_builder().load_psks(…).build(…)` failed.
    #[error("commit create error: {0}")]
    CreateCommit(#[from] openmls::group::CreateCommitError),
    /// `stage_commit(provider)` failed (storage / signature / staging error).
    #[error("commit stage error: {0}")]
    StageCommit(#[from] openmls::group::CommitBuilderStageError<StorageError>),
    /// `apply_app_data_update_payload` failed while pre-computing the new
    /// dict value the commit builder hands to OpenMLS. The most common
    /// cause is a mismatch between the sender's idea of the current dict
    /// state and the receiver's, which would surface as a confirmation
    /// tag mismatch on the wire if it ever escaped.
    #[error("apply payload error: {0}")]
    ApplyPayload(#[from] self::component_source::ComponentSourceError),
}

// Specialize to the concrete SqlKeyStoreError because that's the only
// storage instantiation used (see `GroupError::AppDataCommit` at
// error.rs). It also lets us delegate to `RetryableError<Mls>` impls
// already defined in `xmtp_db::errors` for the inner OpenMLS error
// types — sibling pattern to `GroupError::Proposal(e) => e.is_retryable()`
// — so SQLite-busy storage faults retry instead of permanently failing
// the intent.
impl xmtp_common::RetryableError for GroupAppDataError<xmtp_db::sql_key_store::SqlKeyStoreError> {
    fn is_retryable(&self) -> bool {
        match self {
            // Delegate to the inner OpenMLS error's retryability so
            // SQLite-busy storage faults during propose / stage retry
            // rather than permanently fail the intent. The matching
            // upstream impls live in `xmtp_db::errors`
            // (`RetryableError<Mls>` for `ProposalError` /
            // `CommitBuilderStageError`).
            Self::Propose(e) => xmtp_common::retryable!(e),
            Self::StageCommit(e) => xmtp_common::retryable!(e),
            // Deterministic shape / staging-precondition failures —
            // CreateCommit is upstream-`false`, and ApplyPayload is a
            // sender-side encode failure that won't get better on
            // retry.
            Self::CreateCommit(_) | Self::ApplyPayload(_) => false,
        }
    }
}

/// Compute the [`AppDataUpdates`] required to commit any pending
/// AppDataUpdate proposals in the group's proposal store.
///
/// Walks the proposal store and threads each `Update` / `Remove` through
/// [`accumulate_app_data_updates`]. The result is what callers pass to
/// [`CommitBuilder::with_app_data_dictionary_updates`] when committing
/// pending proposals locally.
///
/// Returns `Ok(None)` when there are no AppDataUpdate proposals pending —
/// this is the common case and lets the caller skip the `with_…` plumbing
/// entirely without changing semantics.
pub(crate) fn pending_app_data_updates(
    mls_group: &OpenMlsGroup,
) -> Result<Option<AppDataUpdates>, ComponentSourceError> {
    let iter = mls_group
        .pending_proposals()
        .filter_map(|queued| match queued.proposal() {
            Proposal::AppDataUpdate(app_data) => {
                Some((app_data.component_id(), app_data.operation()))
            }
            _ => None,
        });
    accumulate_app_data_updates(mls_group, iter)
}

/// Read the committed component registry from the dictionary.
/// Missing registry entries reject component writes by default.
pub(crate) fn load_component_registry(
    mls_group: &OpenMlsGroup,
) -> Result<ComponentRegistry, ComponentSourceError> {
    load_component_registry_from_extensions(mls_group.extensions())
}

/// Returns the group's committed `MIN_SUPPORTED_PROTOCOL_VERSION` floor
/// when it exceeds `own_version`, reading ONLY the pre-commit AppData
/// dict — committed, already-validated state.
///
/// This is the shared trigger for the "pause, don't fork" guards on the
/// receive paths ([`process_message_with_app_data`] before dispatch;
/// `ValidatedCommit::from_staged_commit` before interpreting migrated
/// group state). It is deliberately blind to any floor bump carried by
/// the commit currently being processed: that proposal has not passed
/// the super-admin policy check yet, and a pause triggered by
/// unvalidated input would let any member freeze the group permanently.
/// The commit that *raises* the floor pauses below-floor receivers
/// through the post-policy check at the end of commit validation
/// instead. Consequence for protocol evolution: a release introducing
/// a new wire format must land the group-floor bump in a *strictly
/// earlier* commit than the first commit using that format.
///
/// Lenient on malformed state (non-UTF-8 floor bytes, unparseable
/// semver ⇒ `None`), mirroring `enforce_min_version_monotonicity`'s
/// treatment of malformed priors: garbage must never brick the group.
pub(crate) fn committed_floor_exceeding(
    mls_group: &OpenMlsGroup,
    own: &LibXMTPVersion,
) -> Option<String> {
    committed_floor_exceeding_in_extensions(mls_group.extensions(), own)
}

/// Extensions-only variant of [`committed_floor_exceeding`], split out
/// (like [`load_component_registry_from_extensions`]) so unit tests can
/// exercise the parse-and-compare logic without materializing an
/// `OpenMlsGroup`.
pub(crate) fn committed_floor_exceeding_in_extensions(
    extensions: &openmls::extensions::Extensions<openmls::group::GroupContext>,
    own: &LibXMTPVersion,
) -> Option<String> {
    let bytes = extensions
        .app_data_dictionary()?
        .dictionary()
        .get(&ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION.as_u16())?
        .to_vec();
    let floor = String::from_utf8(bytes).ok()?;
    let floor_version = LibXMTPVersion::parse(&floor).ok()?;
    (floor_version > *own).then_some(floor)
}

/// Read the component registry without loading an OpenMLS group.
pub(crate) fn load_component_registry_from_extensions(
    extensions: &openmls::extensions::Extensions<openmls::group::GroupContext>,
) -> Result<ComponentRegistry, ComponentSourceError> {
    // The committed dictionary is the source of truth for the registry.
    if let Some(ext) = extensions.app_data_dictionary()
        && let Some(bytes) = ext
            .dictionary()
            .get(&ComponentId::COMPONENT_REGISTRY.as_u16())
    {
        return ComponentRegistry::from_bytes(bytes)
            .map_err(|e| ComponentSourceError::MalformedComponentValue {
                component_id: ComponentId::COMPONENT_REGISTRY,
                reason: format!("registry decode: {e}"),
            })
            .inspect(|reg| {
                // Tolerated (preserved-but-invisible) entries mean the
                // dict was written by a newer protocol version or
                // carries a historical invalid entry. Writes to those
                // components fall to deny-by-default; everything else
                // validates normally. Loud so poisoned-registry
                // incidents are diagnosable from logs.
                let unrecognized: Vec<_> = reg.unrecognized_ids().collect();
                if !unrecognized.is_empty() {
                    tracing::warn!(
                        ?unrecognized,
                        "component registry contains unrecognized entries; \
                         treating them as unregistered (deny-by-default)"
                    );
                }
            });
    }

    // Pre-migration or test override.
    #[cfg(any(test, feature = "test-utils"))]
    if let Ok(reg) = TEST_REGISTRY_OVERRIDE.try_with(|r| r.clone()) {
        return Ok(reg);
    }
    Ok(ComponentRegistry::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use openmls::extensions::{
        AppDataDictionary, AppDataDictionaryExtension, Extension, Extensions,
    };

    fn extensions_with_dict(
        entries: &[(u16, Vec<u8>)],
    ) -> Extensions<openmls::group::GroupContext> {
        let mut dict = AppDataDictionary::new();
        for (id, bytes) in entries {
            let _ = dict.insert(*id, bytes.clone());
        }
        Extensions::from_vec(vec![Extension::AppDataDictionary(
            AppDataDictionaryExtension::new(dict),
        )])
        .expect("AppDataDictionary is a valid GroupContext extension")
    }

    fn empty_extensions() -> Extensions<openmls::group::GroupContext> {
        Extensions::from_vec(vec![]).expect("empty extensions are always valid")
    }

    /// Parse a semver string the way the production caller does (once, from
    /// the client's own `pkg_version`). Panics on invalid input — matching
    /// `VersionInfo`, which asserts its own version is valid at construction.
    fn ver(s: &str) -> LibXMTPVersion {
        LibXMTPVersion::parse(s).unwrap()
    }

    // ========================================================================
    // committed_floor_exceeding_in_extensions
    // ========================================================================
    //
    // The shared trigger for the pause-before-parse guards. Two properties
    // are load-bearing: (1) it fires strictly on floor > own — equal or
    // lower floors must not pause; (2) it is lenient on garbage — malformed
    // floor bytes must read as "no floor", never as an error that could
    // wedge the group.

    #[xmtp_common::test(unwrap_try = true)]
    fn floor_above_own_version_fires() {
        let exts = extensions_with_dict(&[(
            ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION.as_u16(),
            b"2.0.0".to_vec(),
        )]);
        assert_eq!(
            committed_floor_exceeding_in_extensions(&exts, &ver("1.11.0")),
            Some("2.0.0".to_string())
        );
        // Prerelease floors order correctly under semver: 1.11.0-dev
        // exceeds 1.10.0 but not 1.11.0.
        let exts = extensions_with_dict(&[(
            ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION.as_u16(),
            b"1.11.0-dev".to_vec(),
        )]);
        assert_eq!(
            committed_floor_exceeding_in_extensions(&exts, &ver("1.10.0")),
            Some("1.11.0-dev".to_string())
        );
        assert_eq!(
            committed_floor_exceeding_in_extensions(&exts, &ver("1.11.0")),
            None
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn floor_at_or_below_own_version_does_not_fire() {
        let exts = extensions_with_dict(&[(
            ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION.as_u16(),
            b"1.11.0".to_vec(),
        )]);
        // Equal: not paused — the floor is inclusive.
        assert_eq!(
            committed_floor_exceeding_in_extensions(&exts, &ver("1.11.0")),
            None
        );
        // Above: not paused.
        assert_eq!(
            committed_floor_exceeding_in_extensions(&exts, &ver("1.12.0")),
            None
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn missing_floor_or_dict_does_not_fire() {
        assert_eq!(
            committed_floor_exceeding_in_extensions(&empty_extensions(), &ver("1.11.0")),
            None
        );
        assert_eq!(
            committed_floor_exceeding_in_extensions(&extensions_with_dict(&[]), &ver("1.11.0")),
            None
        );
        // Dict present with other components but no floor entry.
        let exts = extensions_with_dict(&[(ComponentId::GROUP_NAME.as_u16(), b"name".to_vec())]);
        assert_eq!(
            committed_floor_exceeding_in_extensions(&exts, &ver("1.11.0")),
            None
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn malformed_floor_is_lenient() {
        // Non-UTF-8 bytes → no floor, never an error.
        let exts = extensions_with_dict(&[(
            ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION.as_u16(),
            vec![0xFF, 0xFE],
        )]);
        assert_eq!(
            committed_floor_exceeding_in_extensions(&exts, &ver("1.11.0")),
            None
        );
        // Unparseable floor semver → no floor.
        let exts = extensions_with_dict(&[(
            ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION.as_u16(),
            b"not-a-version".to_vec(),
        )]);
        assert_eq!(
            committed_floor_exceeding_in_extensions(&exts, &ver("1.11.0")),
            None
        );
        // The client's own version can no longer be unparseable here: it is
        // parsed once and asserted valid when `VersionInfo` is built, so this
        // guard only ever compares against a valid `LibXMTPVersion`.
    }

    // ========================================================================
    // load_component_registry_from_extensions
    // ========================================================================
    //
    // These pin the contract that the migration-marker
    // (registry entry presence) and the registry loader
    // (`load_component_registry_from_extensions`, parseability) agree on
    // exactly one shape of disagreement: malformed bytes surface as a
    // hard `MalformedComponentValue` error rather than silently
    // collapsing to an empty registry. An empty registry on a "migrated"
    // group would cause downstream readers (mutable_metadata, validators)
    // to silently lose every dict-backed component, so this invariant is
    // load-bearing.

    #[xmtp_common::test(unwrap_try = true)]
    fn load_registry_no_dict_returns_empty() {
        let reg = load_component_registry_from_extensions(&empty_extensions()).unwrap();
        assert!(reg.is_empty());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn load_registry_dict_without_entry_returns_empty() {
        // Dict present but no COMPONENT_REGISTRY entry => pre-bootstrap.
        // An entry under some *other* component id must not be confused
        // for the registry payload.
        let exts =
            extensions_with_dict(&[(ComponentId::GROUP_NAME.as_u16(), b"Group Name".to_vec())]);
        let reg = load_component_registry_from_extensions(&exts).unwrap();
        assert!(reg.is_empty());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn load_registry_with_valid_bytes_round_trips() {
        let original = ComponentRegistry::new();
        let bytes = original.to_bytes().expect("empty registry serializes");
        let exts = extensions_with_dict(&[(ComponentId::COMPONENT_REGISTRY.as_u16(), bytes)]);
        let loaded = load_component_registry_from_extensions(&exts).unwrap();
        assert_eq!(loaded, original);
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn load_registry_with_malformed_bytes_surfaces_error() {
        // Pin the "fail loud, never return empty" invariant: a
        // malformed `COMPONENT_REGISTRY` value must surface as
        // `MalformedComponentValue` so downstream readers don't carry
        // on with a phantom empty registry against an
        // dictionary with a registry entry.
        let exts = extensions_with_dict(&[(
            ComponentId::COMPONENT_REGISTRY.as_u16(),
            vec![0xff, 0xff, 0xff],
        )]);
        let err = load_component_registry_from_extensions(&exts).unwrap_err();
        assert!(
            matches!(
                err,
                ComponentSourceError::MalformedComponentValue { component_id, .. }
                    if component_id == ComponentId::COMPONENT_REGISTRY
            ),
            "expected MalformedComponentValue for COMPONENT_REGISTRY, got: {err:?}"
        );
    }
}
