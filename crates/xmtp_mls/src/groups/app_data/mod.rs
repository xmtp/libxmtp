//! App-data plumbing for moving group state from group context extensions
//! onto OpenMLS `AppDataUpdate` proposals.
//!
//! This module is the bridge between the per-field intent handlers in
//! `mls_sync` and the OpenMLS app data dictionary. It is intentionally
//! `pub(crate)` — there is no public API for reading or writing arbitrary
//! components. The existing per-field helpers (`update_group_name`,
//! `update_admin_list_action`, …) keep their signatures and route through
//! the appropriate sub-module here when the group has flipped
//! `proposals_enabled`.

// `pub` (rather than `pub(crate)`) so the public `GroupError::ComponentSource`
// variant in `crate::groups::error` doesn't trip the `private_interfaces`
// lint. The functions inside the module remain `pub(crate)`, so the wider
// crate ecosystem still can't read or write arbitrary components — only
// `GroupError` consumers see the error type.
pub(crate) mod bootstrap_validator;
pub mod component_source;
pub mod migration;

use std::collections::BTreeMap;

use openmls::{
    component::ComponentData,
    framing::{ProcessedMessage, ProtocolMessage},
    group::{AppDataUpdates, MlsGroup as OpenMlsGroup, ProcessMessageError},
    messages::proposals::{AppDataUpdateOperation, Proposal},
    messages::proposals_in::{ProposalIn, ProposalOrRefIn},
    // `CommitMessageBundle` lives in `prelude` because the natural path
    // (`openmls::group::commit_builder`) is private to the openmls crate.
    // Re-importing through prelude is the only public path.
    prelude::CommitMessageBundle,
    storage::OpenMlsProvider,
};
use xmtp_mls_common::app_data::{component_id::ComponentId, component_registry::ComponentRegistry};

use self::component_source::{
    ComponentSourceError, apply_app_data_update_payload, read_from_app_data_dict,
};

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
                    ),
                    Some(None) => apply_app_data_update_payload(xmtp_id, payload.as_slice(), None),
                    None => {
                        let from_dict = read_from_app_data_dict(xmtp_id, mls_group);
                        apply_app_data_update_payload(
                            xmtp_id,
                            payload.as_slice(),
                            from_dict.as_deref(),
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
/// `OpenMlsGroup::process_message` returns
/// [`ProcessMessageError::FoundAppDataUpdateProposal`] when a commit
/// contains an `AppDataUpdate` proposal — the application is required to
/// pre-compute the resulting [`AppDataUpdates`] and call
/// [`OpenMlsGroup::process_unverified_message_with_app_data_updates`]
/// instead. This wrapper does the two-step dance:
///
/// 1. `unprotect_message` to get an `UnverifiedMessage`.
/// 2. Walk `committed_proposals()` for `AppDataUpdate`s and hand them to
///    [`accumulate_app_data_updates`] to compute the resulting
///    [`AppDataUpdates`].
/// 3. Call `process_unverified_message_with_app_data_updates` with the
///    resulting `AppDataUpdates` (or `None`).
///
/// Callers replace `mls_group.process_message(provider, message)` with
/// `process_message_with_app_data(mls_group, provider, message)` and get
/// back the same `ProcessedMessage` they used to.
pub(crate) fn process_message_with_app_data<Provider: OpenMlsProvider>(
    mls_group: &mut OpenMlsGroup,
    provider: &Provider,
    message: impl Into<ProtocolMessage>,
) -> Result<ProcessedMessage, ProcessMessageWithAppDataError<Provider::StorageError>> {
    let unverified = mls_group.unprotect_message(provider, message)?;

    let app_data_updates: Option<AppDataUpdates> = match unverified.committed_proposals() {
        Some(proposals) => {
            // Collect owned (id, operation) tuples so the iterator doesn't
            // borrow `mls_group` — `accumulate_app_data_updates` needs `&mls_group`
            // and we'd otherwise conflict with the pending-proposal lookup below.
            //
            // References resolve against the group's proposal store: the
            // receiver already accepted the standalone AppDataUpdate proposal
            // into `pending_proposals`, and OpenMLS's commit-side proposal
            // queue merges inline + referenced proposals before calling
            // `apply_app_data_update_proposals`. If we skipped references
            // here, any commit carrying a by-reference AppDataUpdate would
            // fail with `MissingAppDataUpdates`.
            let mut collected: Vec<(openmls::component::ComponentId, AppDataUpdateOperation)> =
                Vec::new();
            for p in proposals {
                match p {
                    ProposalOrRefIn::Proposal(boxed) => {
                        if let ProposalIn::AppDataUpdate(app_data) = boxed.as_ref() {
                            collected.push((app_data.component_id(), app_data.operation().clone()));
                        }
                    }
                    ProposalOrRefIn::Reference(proposal_ref) => {
                        if let Some(queued) = mls_group
                            .pending_proposals()
                            .find(|q| q.proposal_reference_ref() == proposal_ref.as_ref())
                            && let Proposal::AppDataUpdate(app_data) = queued.proposal()
                        {
                            collected.push((app_data.component_id(), app_data.operation().clone()));
                        }
                    }
                }
            }
            let iter = collected.iter().map(|(id, op)| (*id, op));
            accumulate_app_data_updates(mls_group, iter)?
        }
        None => None,
    };

    Ok(mls_group.process_unverified_message_with_app_data_updates(
        provider,
        unverified,
        app_data_updates,
    )?)
}

/// Stage a commit that bundles a single inline `AppDataUpdate(Update)`
/// proposal AND the resulting AppDataDictionary update.
///
/// This is the shape used by the per-field intent handlers
/// (`MetadataUpdate`, `UpdateAdminList`, …) when `proposals_enabled` is
/// on. Unlike `propose_app_data_update` (which produces a
/// proposal-by-reference that has to be committed in a follow-up sync),
/// this builds a self-contained commit so the propose-and-apply happens in
/// a single network round trip — preserving the "metadata update completes
/// in one sync" semantics that the legacy GCE path provides.
///
/// The caller is expected to wrap this inside `generate_commit_with_rollback`
/// so the staged commit can be extracted and persisted alongside the
/// intent.
pub(crate) fn stage_inline_app_data_commit<Provider: OpenMlsProvider>(
    mls_group: &mut OpenMlsGroup,
    provider: &Provider,
    signer: &impl openmls_traits::signatures::Signer,
    component_id: ComponentId,
    payload: Vec<u8>,
) -> Result<CommitMessageBundle, GroupAppDataError<Provider::StorageError>> {
    use openmls::messages::proposals::AppDataUpdateProposal;

    let openmls_id = component_id.as_u16();

    // The commit builder defaults to `consume_proposal_store(true)` (see
    // openmls's `Initial::default()`), matching the legacy GCE metadata-
    // update path's behavior. So any pending `AppDataUpdate` proposals
    // sitting in the store at the moment this runs will ALSO be swept
    // into the commit — we account for their effects by chaining them
    // with our own inline proposal through `accumulate_app_data_updates`
    // below. Otherwise OpenMLS's `apply_app_data_update_proposals` would
    // silently drop the pending proposals' dict writes.
    //
    // Compute `AppDataUpdates` BEFORE we hand the proposal off to the
    // commit builder. The receiver does the same dance via
    // `process_message_with_app_data`, so both sides must end up with
    // identical dict bytes — otherwise the commit's confirmation tag
    // won't match across peers.
    //
    // The ordering (pending first, inline last) mirrors OpenMLS's commit
    // builder (`group_proposal_store_queue.chain(own_proposals)`) so
    // that if a pending proposal and the inline proposal both target
    // the same component, the inline one wins the final value.
    //
    // Failure mode if OpenMLS ever changes that chain order: the sender
    // and receiver will each compute a different final dict value for the
    // same component, so the commit's confirmation tag won't match across
    // peers. That is an *observable* failure (receivers reject the commit
    // with `WrongConfirmationTag`) — not a silent one — and the E2E tests
    // in `groups/tests/test_proposals.rs` under the AppDataUpdate section
    // would fail loudly on any openmls bump that reordered the chain. A
    // dedicated "pending-vs-inline ordering" test belongs in the migration
    // PR that wires a public standalone-proposal path — without that path
    // there's no public API today to pre-populate the pending store with
    // an AppDataUpdate proposal to test against.
    let inline_operation = AppDataUpdateOperation::Update(payload.clone().into());
    let pending_tuples: Vec<(openmls::component::ComponentId, AppDataUpdateOperation)> = mls_group
        .pending_proposals()
        .filter_map(|q| match q.proposal() {
            Proposal::AppDataUpdate(p) => Some((p.component_id(), p.operation().clone())),
            _ => None,
        })
        .collect();
    let chained = pending_tuples
        .iter()
        .map(|(id, op)| (*id, op))
        .chain(std::iter::once((openmls_id, &inline_operation)));
    let app_data_updates = accumulate_app_data_updates(mls_group, chained).inspect_err(|e| {
        tracing::error!(
            component_id = %component_id,
            error = %e,
            "Failed to compute AppDataUpdates for inline commit"
        );
    })?;

    let mut stage = mls_group
        .commit_builder()
        .add_proposal(Proposal::AppDataUpdate(Box::new(
            AppDataUpdateProposal::update(openmls_id, payload),
        )))
        .load_psks(provider.storage())?;
    stage.with_app_data_dictionary_updates(app_data_updates);

    let bundle = stage
        .build(provider.rand(), provider.crypto(), signer, |_| true)?
        .stage_commit(provider)?;

    Ok(bundle)
}

/// Errors surfaced by [`stage_inline_app_data_commit`].
///
/// Wrapped into `GroupError` via the `#[from]` impl on
/// `GroupError::AppDataCommit` so the structured source is preserved at
/// the call site (no string-flattening). The `pub(crate)` visibility
/// matches the helper itself; the variant is only re-exported through
/// the public `GroupError` enum.
#[derive(Debug, thiserror::Error)]
pub enum GroupAppDataError<StorageError: std::error::Error> {
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

/// True when the group has completed the bootstrap migration from
/// legacy GCE extensions to the AppData dictionary.
///
/// The discriminator is "does the dict have a `COMPONENT_REGISTRY`
/// entry?" — bootstrap writes that entry as its first proposal, so
/// its presence is the ground-truth marker that the group has been
/// migrated.
///
/// This is intentionally distinct from [`MlsGroup::proposals_enabled`]:
/// a group can have `proposals_enabled == true` without having
/// completed a bootstrap commit yet (the foundation-PR window).
/// Read accessors key off this helper instead so they correctly fall
/// back to the legacy GMM extension on proposals-enabled-but-
/// unbootstrapped groups.
pub(crate) fn is_migrated_group(mls_group: &OpenMlsGroup) -> bool {
    is_migrated_extensions(mls_group.extensions())
}

/// Extensions-only variant of [`is_migrated_group`]. Kept in sync so
/// every read-path gate lands on the same predicate (COMPONENT_REGISTRY
/// present in the AppData dict) — consumers that only have an
/// `Extensions` reference (e.g. commit-validation paths walking
/// staged-commit extensions) can call this directly without
/// materializing an `OpenMlsGroup`.
///
/// Test-only override: when a test harness has installed a
/// [`TEST_REGISTRY_OVERRIDE`] scope the group is treated as migrated
/// regardless of what the dict contains. This bridges the gap for
/// tests that exercise post-bootstrap reader semantics without
/// actually running the bootstrap commit (which writes the real
/// `COMPONENT_REGISTRY` entry — that work lives in a follow-up
/// branch). Production paths never hit this branch because the
/// task-local is only initialized inside test scopes.
pub(crate) fn is_migrated_extensions(
    extensions: &openmls::extensions::Extensions<openmls::group::GroupContext>,
) -> bool {
    // Test-only override: treat the group as migrated when a
    // [`TEST_REGISTRY_OVERRIDE`] scope is active *and* the dict has
    // any entry — i.e. at least one post-capability AppDataUpdate has
    // written something. The dict-has-any-entry clause matters so that
    // pre-`enable_proposals()` test steps (which write via the legacy
    // path and leave the dict empty) still see legacy-authoritative
    // semantics.
    #[cfg(any(test, feature = "test-utils"))]
    if TEST_REGISTRY_OVERRIDE.try_with(|_| ()).is_ok() {
        let has_any_entry = extensions
            .app_data_dictionary()
            .map(|ext| !ext.dictionary().is_empty())
            .unwrap_or(false);
        if has_any_entry {
            return true;
        }
    }
    extensions
        .app_data_dictionary()
        .map(|ext| {
            ext.dictionary()
                .contains(&ComponentId::COMPONENT_REGISTRY.as_u16())
        })
        .unwrap_or(false)
}

/// Load the [`ComponentRegistry`] for a group.
///
/// On a migrated group the registry lives in the AppData dict under
/// [`ComponentId::COMPONENT_REGISTRY`]; on unmigrated groups it
/// returns an empty registry (or the test override, when present —
/// see [`TEST_REGISTRY_OVERRIDE`]).
///
/// Returns an error when a `COMPONENT_REGISTRY` entry is present in
/// the dict but its bytes don't decode — silently swallowing that into
/// an empty registry would let [`is_migrated_extensions`] (which only
/// checks key existence) and this loader disagree about whether the
/// group is migrated, and downstream readers built on an empty
/// registry would silently lose every dict-backed component on the
/// migrated path. Surfacing as
/// [`ComponentSourceError::MalformedComponentValue`] keeps the
/// wire-format-violation signal loud and reuses the same variant the
/// rest of the dict-decode helpers already reach for.
///
/// ## Security model while the registry is empty (pre-bootstrap)
///
/// Empty registry is the **strictest** validator state, not the most
/// permissive. Two layers make this safe:
///
/// 1. **Sender gate** (`mls_sync.rs`): the `AppDataUpdate` sender path
///    is guarded by `proposals_enabled(group) && !registry.is_empty()`.
///    In production the second clause is false on unmigrated groups,
///    so the legacy GCE path runs and no `AppDataUpdate` proposals get
///    emitted.
///    (`test_update_group_name_uses_legacy_path_when_registry_is_empty`
///    pins this.)
/// 2. **Receiver deny-by-default**
///    (`xmtp_mls_common::app_data::validation::validate_component_write`):
///    any `AppDataUpdate` whose component has no registry entry is
///    rejected with `ComponentPermissionError::NoRegistryEntry`,
///    surfacing as `CommitValidationError::InsufficientPermissions` in
///    [`validate_app_data_update_proposals_in_commit`]. So even if a
///    Byzantine peer crafts a commit carrying `AppDataUpdate`
///    proposals, honest receivers reject it.
///
/// Hardcoded components (`COMPONENT_REGISTRY`, `SUPER_ADMIN_LIST`)
/// bypass the registry lookup by design — they're super-admin-only in
/// code — so the bootstrap commit (which writes `COMPONENT_REGISTRY`
/// as its first proposal) can land even against an empty registry.
///
/// Test code can inject a populated registry by wrapping its body in
/// `TEST_REGISTRY_OVERRIDE.scope(registry, async { … }).await`.
pub(crate) fn load_component_registry(
    mls_group: &OpenMlsGroup,
) -> Result<ComponentRegistry, ComponentSourceError> {
    load_component_registry_from_extensions(mls_group.extensions())
}

/// Extensions-only variant of [`load_component_registry`]. Mirrors the
/// [`is_migrated_group`] / [`is_migrated_extensions`] split so unit
/// tests can exercise the registry-decode path without materializing
/// an `OpenMlsGroup`.
pub(crate) fn load_component_registry_from_extensions(
    extensions: &openmls::extensions::Extensions<openmls::group::GroupContext>,
) -> Result<ComponentRegistry, ComponentSourceError> {
    // Post-migration: the registry lives in the AppData dict under
    // `COMPONENT_REGISTRY`. A migrated group's dict always has this
    // entry (the bootstrap commit seeds it before flipping
    // proposals_enabled), so if we find it, it's authoritative.
    if let Some(ext) = extensions.app_data_dictionary()
        && let Some(bytes) = ext
            .dictionary()
            .get(&ComponentId::COMPONENT_REGISTRY.as_u16())
    {
        return ComponentRegistry::from_bytes(bytes).map_err(|e| {
            ComponentSourceError::MalformedComponentValue {
                component_id: ComponentId::COMPONENT_REGISTRY,
                reason: format!("registry decode: {e}"),
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
    //! Unit coverage for the migration-marker predicate —
    //! [`is_migrated_extensions`]. These pin the three read-side
    //! invariants from the PR review:
    //!   (a) registry empty / dict missing => legacy-authoritative,
    //!   (b) overlay no-op on unmigrated groups (even if `TEST_REGISTRY_OVERRIDE`
    //!       is set but the dict is empty),
    //!   (c) `COMPONENT_REGISTRY` in dict => migrated
    //!       (production signal, independent of any test override).
    //!
    //! Post-bootstrap reader-see-dict-values coverage lives as an
    //! integration test in `groups/tests/test_proposals.rs` — see
    //! `test_app_data_update_overlays_legacy_gmm_on_conflict` — because
    //! it needs the full MLS commit pipeline.
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

    #[test]
    fn unmigrated_without_override_is_not_migrated() {
        // Invariant (a): no dict, no override → legacy authoritative.
        assert!(!is_migrated_extensions(&empty_extensions()));
        // Dict present but empty → still not migrated.
        assert!(!is_migrated_extensions(&extensions_with_dict(&[])));
    }

    #[test]
    fn dict_without_registry_entry_is_not_migrated() {
        // Invariant (a) corollary: a dict entry for some *other*
        // component isn't enough to flip the gate in production —
        // only `COMPONENT_REGISTRY` counts.
        let exts =
            extensions_with_dict(&[(ComponentId::GROUP_NAME.as_u16(), b"Group Name".to_vec())]);
        assert!(!is_migrated_extensions(&exts));
    }

    #[test]
    fn dict_with_registry_entry_is_migrated() {
        // Invariant (c): production signal. `COMPONENT_REGISTRY` in the
        // dict => migrated, regardless of any test override.
        let exts =
            extensions_with_dict(&[(ComponentId::COMPONENT_REGISTRY.as_u16(), vec![0x01, 0x02])]);
        assert!(is_migrated_extensions(&exts));
    }

    #[tokio::test]
    async fn override_without_dict_entries_is_not_migrated() {
        // Invariant (b): with `TEST_REGISTRY_OVERRIDE` set but the dict
        // empty (i.e. the pre-`enable_proposals()` window of an
        // integration test), the gate stays closed. This is what lets
        // step-1 assertions in `test_app_data_update_overlays_legacy_gmm_on_conflict`
        // still read the legacy GMM value instead of being shadowed by
        // an empty-dict overlay.
        let reg = ComponentRegistry::new();
        TEST_REGISTRY_OVERRIDE
            .scope(reg, async {
                assert!(!is_migrated_extensions(&empty_extensions()));
                assert!(!is_migrated_extensions(&extensions_with_dict(&[])));
            })
            .await;
    }

    #[tokio::test]
    async fn override_with_dict_entry_flips_migrated_in_tests() {
        // Complement to the above: once a test has written at least
        // one component to the dict, the test-override branch flips
        // the gate so subsequent reads route through the overlay.
        let reg = ComponentRegistry::new();
        TEST_REGISTRY_OVERRIDE
            .scope(reg, async {
                let exts = extensions_with_dict(&[(
                    ComponentId::GROUP_NAME.as_u16(),
                    b"Dict Name".to_vec(),
                )]);
                assert!(is_migrated_extensions(&exts));
            })
            .await;
    }

    // ========================================================================
    // load_component_registry_from_extensions
    // ========================================================================
    //
    // These pin the contract that the migration-marker
    // (`is_migrated_extensions`, key-existence) and the registry loader
    // (`load_component_registry_from_extensions`, parseability) agree on
    // exactly one shape of disagreement: malformed bytes surface as a
    // hard `MalformedComponentValue` error rather than silently
    // collapsing to an empty registry. An empty registry on a "migrated"
    // group would cause downstream readers (mutable_metadata, validators)
    // to silently lose every dict-backed component, so this invariant is
    // load-bearing.

    #[test]
    fn load_registry_no_dict_returns_empty() {
        let reg = load_component_registry_from_extensions(&empty_extensions()).unwrap();
        assert!(reg.is_empty());
    }

    #[test]
    fn load_registry_dict_without_entry_returns_empty() {
        // Dict present but no COMPONENT_REGISTRY entry => pre-bootstrap.
        // An entry under some *other* component id must not be confused
        // for the registry payload.
        let exts =
            extensions_with_dict(&[(ComponentId::GROUP_NAME.as_u16(), b"Group Name".to_vec())]);
        let reg = load_component_registry_from_extensions(&exts).unwrap();
        assert!(reg.is_empty());
    }

    #[test]
    fn load_registry_with_valid_bytes_round_trips() {
        let original = ComponentRegistry::new();
        let bytes = original.to_bytes().expect("empty registry serializes");
        let exts = extensions_with_dict(&[(ComponentId::COMPONENT_REGISTRY.as_u16(), bytes)]);
        let loaded = load_component_registry_from_extensions(&exts).unwrap();
        assert_eq!(loaded, original);
    }

    #[test]
    fn load_registry_with_malformed_bytes_surfaces_error() {
        // Pin the "fail loud, never return empty" invariant: a
        // malformed `COMPONENT_REGISTRY` value must surface as
        // `MalformedComponentValue` so downstream readers don't carry
        // on with a phantom empty registry against an
        // `is_migrated_extensions == true` dict.
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
