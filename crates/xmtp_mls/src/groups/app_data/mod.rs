#![expect(dead_code)]
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
pub mod component_source;

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

/// Load the [`ComponentRegistry`] for a group.
///
/// Returns an empty registry in production until the migration PR lands
/// — see `docs/plans/2026-04-10-app-data-migration-plan.md` for the
/// bootstrap-commit design that will synthesize and persist the
/// registry inside the AppData dict under
/// [`ComponentId::COMPONENT_REGISTRY`].
///
/// ## Security model while the registry is empty
///
/// Empty registry is the **strictest** validator state, not the most
/// permissive. Two layers make this safe:
///
/// 1. **Sender gate** (`mls_sync.rs`): the `AppDataUpdate` sender path
///    is guarded by `proposals_enabled(group) && !registry.is_empty()`.
///    In production the second clause is false, so the legacy GCE path
///    runs and no `AppDataUpdate` proposals get emitted.
///    (`test_update_group_name_uses_legacy_path_when_registry_is_empty`
///    pins this.)
/// 2. **Receiver deny-by-default** (`xmtp_mls_common::app_data::
///    validation::validate_component_write`): any `AppDataUpdate` whose
///    component has no registry entry is rejected with
///    `ComponentPermissionError::NoRegistryEntry`, surfacing as
///    `CommitValidationError::InsufficientPermissions` in
///    [`validate_app_data_update_proposals_in_commit`]. So even if a
///    Byzantine peer crafts a commit carrying `AppDataUpdate`
///    proposals, honest receivers reject it.
///
/// Hardcoded components (`COMPONENT_REGISTRY`, `SUPER_ADMIN_LIST`)
/// bypass the registry lookup by design — they're super-admin-only in
/// code — so the migration PR's bootstrap commit (which writes
/// `COMPONENT_REGISTRY` as its first proposal) can land even against
/// an empty registry.
///
/// Test code can inject a populated registry by wrapping its body in
/// `TEST_REGISTRY_OVERRIDE.scope(registry, async { … }).await`.
pub(crate) fn load_component_registry(_mls_group: &OpenMlsGroup) -> ComponentRegistry {
    #[cfg(any(test, feature = "test-utils"))]
    if let Ok(reg) = TEST_REGISTRY_OVERRIDE.try_with(|r| r.clone()) {
        return reg;
    }
    ComponentRegistry::new()
}
