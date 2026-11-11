//! Group state stored in the OpenMLS AppData dictionary.
//!
//! This module is the bridge between the per-field intent handlers in
//! `mls_sync` and the OpenMLS app data dictionary. The existing per-field
//! helpers (`update_group_name`, `update_admin_list_action`, …) keep their
//! signatures and route through the appropriate sub-module here. The
//! component codec lives in `xmtp_mls_group_state::app_data::component_source`;
//! its helpers transform bytes and do not change a group by themselves.
//!
//! `membership_upkeep` adds the registry reconciliation and removed-member
//! cleanup that ride along with every commit this client builds.

pub(crate) mod membership_upkeep;
pub mod migration;
pub(crate) mod sender_intents;

use std::{
    borrow::Cow,
    collections::{HashMap, HashSet, hash_map::Entry},
};

use openmls::{
    component::ComponentData,
    extensions::AppDataDictionary,
    framing::{MlsMessageOut, ProcessedMessage, ProtocolMessage},
    group::{
        AppDataUpdates, MlsGroup as OpenMlsGroup, ProcessMessageError, ProposalError,
        QueuedProposal, ResolveAppDataCommitError,
    },
    messages::proposals::{AppDataUpdateOperation, Proposal},
    // `CommitMessageBundle` lives in `prelude` because the natural path
    // (`openmls::group::commit_builder`) is private to the openmls crate.
    // Re-importing through prelude is the only public path.
    prelude::{CommitMessageBundle, ProcessedMessageContent},
    storage::OpenMlsProvider,
};
use xmtp_mls_group_state::app_data::{
    component_id::ComponentId,
    component_registry::ComponentRegistry,
    validation::ComponentWriteValidator,
    working_set::{Checkpoint, ComponentStates, WorkingSet},
};
use xmtp_mls_group_state::group_metadata::DmMembers;

use crate::groups::GroupError;
use xmtp_configuration::ApplicationComponentDefinition;
use xmtp_cryptography::secret::{Zeroize, Zeroizing};
use xmtp_mls_group_state::app_data::component_source::{
    ComponentSourceError, read_from_app_data_dict,
};
use xmtp_mls_group_state::app_data::protocol_floor::committed_floor_exceeding;
use xmtp_mls_group_state::inbox_id::InboxId;
use xmtp_mls_group_state::libxmtp_version::LibXMTPVersion;
use xmtp_mls_validation::commit::{
    AppDataUpdateInCommit, AppDataUpdateList, CommitRuleError, MAX_DICTIONARY_BYTES,
    app_data_update_proposer_leaf, counts_toward_total, dictionary_total,
    extract_commit_participant, membership_inbox_ids, read_committed_metadata,
    validate_dictionary_growth, validate_registry_in, validate_update_in,
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
    /// Failed to decode or apply an incoming `AppDataUpdate` payload
    /// ([`WorkingSet::update`]). Almost always indicates a
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
    /// Converted to `CommitRuleError::ProtocolVersionTooLow` at
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
/// Returns `Ok(None)` when the iterator yields no proposals.
pub(crate) fn accumulate_app_data_updates<'a, I>(
    mls_group: &OpenMlsGroup,
    proposals: I,
) -> Result<Option<AppDataUpdates>, ComponentSourceError>
where
    I: IntoIterator<Item = (openmls::component::ComponentId, &'a AppDataUpdateOperation)>,
{
    let registry = load_component_registry(mls_group)?;
    let mut batch = AppDataBatch::new(mls_group, &registry);
    for (id, operation) in proposals {
        batch.apply(id, operation)?;
    }
    batch.updates()
}

/// The dictionary a sequence of `AppDataUpdate` proposals produces, built
/// one proposal at a time.
///
/// Each component's state chains: a proposal sees the effect of the ones
/// before it as its old value. Every payload decodes against the pre-commit
/// registry, as on the receive side, so a registry change in the same
/// commit does not change how its other updates are read. The states live
/// in a [`WorkingSet`], so each touched component is decoded once and
/// encoded once, however many proposals touch it.
///
/// A state can be the commit-log signer's private key, so each is wiped on
/// drop until [`Self::updates`] hands it to OpenMLS.
pub(crate) struct AppDataBatch<'a> {
    group: &'a OpenMlsGroup,
    set: WorkingSet<'a>,
    /// The dictionary total (META-077) before the commit.
    committed_total: usize,
    /// The dictionary total after the proposals applied so far.
    total: usize,
}

/// A point that [`AppDataBatch::rollback`] returns to.
#[must_use = "roll back or release a checkpoint"]
pub(crate) struct BatchCheckpoint {
    set: Checkpoint,
    total: usize,
}

impl<'a> AppDataBatch<'a> {
    /// A batch of no proposals over `group`. `registry` is its committed
    /// registry ([`load_component_registry`]).
    pub(crate) fn new(group: &'a OpenMlsGroup, registry: &'a ComponentRegistry) -> Self {
        let committed_total = dictionary_total(
            group
                .extensions()
                .app_data_dictionary()
                .map(|e| e.dictionary()),
        );
        Self {
            group,
            set: WorkingSet::new(Cow::Borrowed(registry), move |id| {
                read_from_app_data_dict(id, group)
            }),
            committed_total,
            total: committed_total,
        }
    }

    /// A batch holding the group's pending proposals, in the order a
    /// commit that consumes the proposal store applies them.
    pub(crate) fn pending(
        group: &'a OpenMlsGroup,
        registry: &'a ComponentRegistry,
    ) -> Result<Self, ComponentSourceError> {
        let mut batch = Self::new(group, registry);
        for queued in group.pending_proposals() {
            if let Proposal::AppDataUpdate(proposal) = queued.proposal() {
                batch.apply(proposal.component_id(), proposal.operation())?;
            }
        }
        Ok(batch)
    }

    /// Apply one proposal's operation. A `Remove` never consults the
    /// component's payload rules, so component-level Remove rejections (e.g.
    /// the whole-registry Remove ban) are enforced by commit validation,
    /// which both commit-processing paths run before applying.
    pub(crate) fn apply(
        &mut self,
        id: openmls::component::ComponentId,
        operation: &AppDataUpdateOperation,
    ) -> Result<(), ComponentSourceError> {
        let id = ComponentId::from(id);
        let before = self.counted_len(id);
        self.set.apply(id, operation).inspect_err(|e| {
            tracing::warn!(
                component_id = %id,
                error = %e,
                "Failed to apply AppDataUpdate payload"
            );
        })?;
        self.total = self.total - before + self.counted_len(id);
        Ok(())
    }

    /// Judge `update` as every receiver does after the proposals applied so
    /// far ([`validate_update_in`]), with `membership` after the commit, and
    /// apply it when they accept it. A refused update leaves the batch
    /// unchanged.
    pub(crate) fn validate(
        &mut self,
        validator: &mut ComponentWriteValidator<'_>,
        update: AppDataUpdateInCommit<'_>,
        dm_members: Option<&DmMembers<String>>,
        membership: Option<&HashSet<InboxId>>,
    ) -> Result<(), CommitRuleError> {
        let id = update.component_id;
        let before = self.counted_len(id);
        validate_update_in(&mut self.set, validator, update, dm_members, membership)?
            .map_err(|_| CommitRuleError::InsufficientPermissions)?;
        self.total = self.total - before + self.counted_len(id);
        Ok(())
    }

    /// Validate the registry the proposals applied so far leave
    /// ([`validate_registry_in`]).
    pub(crate) fn validate_registry(&mut self) -> Result<(), CommitRuleError> {
        validate_registry_in(&mut self.set)
    }

    /// The component's value after the proposals applied so far. A
    /// collection a proposal changed is encoded.
    pub(crate) fn state(
        &self,
        id: ComponentId,
    ) -> Result<Option<Zeroizing<Vec<u8>>>, ComponentSourceError> {
        self.set.encode(id)
    }

    /// Reject the dictionary the proposals applied so far leave when its
    /// total grows past `max`: receivers' [`MAX_DICTIONARY_BYTES`], or a
    /// commit builder's lower bound.
    // implements: META-077
    pub(crate) fn fits(&self, max: usize) -> Result<(), CommitRuleError> {
        validate_dictionary_growth(self.committed_total, self.total, max)
    }

    /// Mark a point to return to.
    pub(crate) fn checkpoint(&mut self) -> BatchCheckpoint {
        BatchCheckpoint {
            set: self.set.checkpoint(),
            total: self.total,
        }
    }

    /// Undo every proposal applied since `checkpoint`.
    pub(crate) fn rollback(&mut self, checkpoint: BatchCheckpoint) {
        self.set.rollback(checkpoint.set);
        self.total = checkpoint.total;
    }

    /// Keep every proposal applied since `checkpoint`.
    pub(crate) fn release(&mut self, checkpoint: BatchCheckpoint) {
        self.set.release(checkpoint.set);
    }

    /// The state of each component the proposals wrote: its bytes, or
    /// `None` once removed.
    pub(crate) fn into_states(self) -> Result<ComponentStates, ComponentSourceError> {
        self.set.into_states()
    }

    /// The [`AppDataUpdates`] for the commit; `None` when no proposal was
    /// applied.
    pub(crate) fn updates(self) -> Result<Option<AppDataUpdates>, ComponentSourceError> {
        let group = self.group;
        let states = self.into_states()?;
        if states.is_empty() {
            return Ok(None);
        }
        let mut updater = group.app_data_dictionary_updater();
        for (id, value) in states {
            match value {
                // The bytes move to OpenMLS without a copy.
                Some(mut bytes) => updater.set(ComponentData::from_parts(
                    id.as_u16(),
                    std::mem::take(&mut *bytes).into(),
                )),
                None => updater.remove(&id.as_u16()),
            }
        }
        Ok(updater.changes())
    }

    /// The length component `id` adds to the dictionary total after the
    /// proposals applied so far.
    fn counted_len(&self, id: ComponentId) -> usize {
        if counts_toward_total(id) {
            self.set.snapshot_len(id).unwrap_or(0)
        } else {
            0
        }
    }
}

/// Most pending `AppDataUpdate`s of one class, membership or other, that one
/// commit references: about 34 KiB of references.
pub(crate) const MAX_APP_DATA_PROPOSALS: usize = 1_000;

/// Bytes of envelope each class of pending `AppDataUpdate`s may take, at
/// about 34 bytes per reference: a quarter of the envelope.
const BYTES_PER_CAPPED_PROPOSAL: usize = 136;

/// Most pending `AppDataUpdate`s of one class that a commit references under
/// `limits`, so both classes together take at most half the envelope.
// implements: GMOD-047
pub(crate) fn proposal_cap(limits: &xmtp_configuration::LimitsConfiguration) -> usize {
    MAX_APP_DATA_PROPOSALS.min(limits.max_envelope_bytes / BYTES_PER_CAPPED_PROPOSAL)
}

/// Most bytes the components other than `GROUP_MEMBERSHIP` may hold after a
/// commit this client builds under `limits`: half an envelope, so a Welcome,
/// which carries the whole dictionary, still fits. Receivers keep
/// [`MAX_DICTIONARY_BYTES`].
// implements: META-078
pub(crate) fn sender_dictionary_max(limits: &xmtp_configuration::LimitsConfiguration) -> usize {
    MAX_DICTIONARY_BYTES.min(limits.max_envelope_bytes / 2)
}

/// The pending proposals one commit can carry, in store order, and the
/// states they produce.
pub(crate) struct Committable<'a> {
    pub(crate) proposals: Vec<&'a QueuedProposal>,
    /// The state of each component the kept proposals write: its bytes, or
    /// `None` once removed.
    pub(crate) states: ComponentStates,
}

impl<'a> Committable<'a> {
    /// The pending proposals a commit can carry: each reference once, as a
    /// commit covers it, and each `AppDataUpdate` only if every receiver
    /// accepts it after the proposals kept before it. Receivers check the
    /// list rule ([`AppDataUpdateList`]), the proposer's policy against the
    /// chained value and the post-commit membership ([`validate_update_in`]),
    /// and the registry and total the commit leaves.
    ///
    /// Receivers judge a standalone proposal alone, against the committed
    /// dictionary. Proposals accepted one by one can still make an invalid
    /// commit together, such as two Inserts of one key, an Update and a
    /// Remove of one component, or two super-admin removals that leave
    /// none. Every member rejects a commit that carries them.
    ///
    /// Each kept prefix is a commit receivers accept, so a proposal is
    /// judged without looking ahead: a later proposal that would repair it
    /// may itself be omitted, and the intent's own operations follow.
    ///
    /// No client proposes a leaf Update, and receivers check its credential
    /// and that it keeps the replaced leaf's signature key, so a commit never
    /// carries one: the commit builder would refuse a leaf it cannot use.
    ///
    /// The post-commit membership comes from the kept proposals, so the
    /// filter runs again over the kept ones until that membership stops
    /// changing. Each further pass keeps fewer, so this ends.
    ///
    /// `cap` bounds the non-membership `AppDataUpdate`s kept, and
    /// `dictionary_max` the dictionary total they leave; commit building
    /// passes [`proposal_cap`] and [`sender_dictionary_max`]. Selection caps
    /// membership deltas before this runs.
    // implements: GMOD-047, META-078
    pub(crate) fn pending(
        group: &'a OpenMlsGroup,
        cap: usize,
        dictionary_max: usize,
    ) -> Result<Self, GroupError> {
        let registry = load_component_registry(group)?;
        let mut seen = HashSet::new();
        let mut budget = cap;
        let mut proposals: Vec<_> = group
            .pending_proposals()
            .filter(|queued| seen.insert(queued.proposal_reference_ref()))
            // implements: GMOD-047
            // Selection has already capped the membership deltas. Keep the
            // oldest other AppDataUpdates; the rest expire with the epoch.
            .filter(|queued| match queued.proposal() {
                Proposal::AppDataUpdate(update)
                    if update.component_id() != ComponentId::GROUP_MEMBERSHIP.as_u16() =>
                {
                    let keep = budget > 0;
                    budget = budget.saturating_sub(1);
                    keep
                }
                _ => true,
            })
            .collect();
        let mut membership = membership_inbox_ids(read_from_app_data_dict(
            ComponentId::GROUP_MEMBERSHIP,
            group,
        ));
        loop {
            let (kept, batch) = Self::keep(
                group,
                &registry,
                proposals,
                membership.as_ref(),
                dictionary_max,
            )?;
            let kept_membership = membership_inbox_ids(
                batch
                    .state(ComponentId::GROUP_MEMBERSHIP)?
                    .as_deref()
                    .map(Vec::as_slice),
            );
            if kept_membership == membership {
                return Ok(Self {
                    proposals: kept,
                    states: batch.into_states()?,
                });
            }
            membership = kept_membership;
            proposals = kept;
        }
    }

    /// Keep each of `proposals` a receiver accepts after the ones kept
    /// before it, with `membership` after the commit. One batch and one
    /// validator judge them all: a refused proposal is rolled back, so each
    /// component is decoded once and each policy question answered once.
    fn keep<'r>(
        group: &'r OpenMlsGroup,
        registry: &'r ComponentRegistry,
        proposals: Vec<&'a QueuedProposal>,
        membership: Option<&HashSet<InboxId>>,
        dictionary_max: usize,
    ) -> Result<(Vec<&'a QueuedProposal>, AppDataBatch<'r>), GroupError> {
        let (immutable, mutable) = read_committed_metadata(group)?;
        let mut batch = AppDataBatch::new(group, registry);
        let mut validator = ComponentWriteValidator::new(registry);
        let mut list = AppDataUpdateList::default();
        let mut proposers = HashMap::new();
        let mut kept = Vec::with_capacity(proposals.len());
        for queued in proposals {
            if matches!(queued.proposal(), Proposal::Update(_)) {
                continue;
            }
            if let Proposal::AppDataUpdate(update) = queued.proposal() {
                let id = ComponentId::from(update.component_id());
                let operation = update.operation();
                if !list.admits(id, operation, || {
                    read_from_app_data_dict(id, group).is_some()
                }) {
                    continue;
                }
                let Ok(leaf) = app_data_update_proposer_leaf(queued.sender()) else {
                    continue;
                };
                let proposer = match proposers.entry(*leaf) {
                    Entry::Occupied(entry) => entry.into_mut(),
                    Entry::Vacant(entry) => {
                        match extract_commit_participant(leaf, group, &immutable, &mutable) {
                            Ok(proposer) => entry.insert(proposer),
                            Err(_) => continue,
                        }
                    }
                };
                let update = AppDataUpdateInCommit {
                    component_id: id,
                    operation,
                    actor: (&*proposer).into(),
                    proposer_inbox_id: &proposer.inbox_id,
                };
                let checkpoint = batch.checkpoint();
                let accepted = batch
                    .validate(
                        &mut validator,
                        update,
                        immutable.dm_members.as_ref(),
                        membership,
                    )
                    .and_then(|()| {
                        if id == ComponentId::COMPONENT_REGISTRY {
                            batch.validate_registry()
                        } else {
                            Ok(())
                        }
                    })
                    .and_then(|()| batch.fits(dictionary_max));
                if accepted.is_err() {
                    batch.rollback(checkpoint);
                    continue;
                }
                batch.release(checkpoint);
                list.push(id, operation);
            }
            kept.push(queued);
        }
        Ok((kept, batch))
    }
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
    catalogue: &[ApplicationComponentDefinition],
    limits: &xmtp_configuration::LimitsConfiguration,
    component_id: ComponentId,
    payload: Vec<u8>,
) -> Result<(MlsMessageOut, CommitMessageBundle), GroupAppDataError<Provider::StorageError>> {
    let (mut proposals, bundle) = stage_app_data_proposals_and_commit(
        mls_group,
        provider,
        signer,
        catalogue,
        limits,
        vec![(component_id, AppDataUpdateOperation::Update(payload.into()))],
    )?;
    Ok((proposals.remove(0), bundle))
}

/// Propose every `updates` operation and stage one commit of them, using
/// the same pre-commit state. The commit also carries this client's
/// membership upkeep for `catalogue` inline.
pub(crate) fn stage_app_data_proposals_and_commit<Provider: OpenMlsProvider>(
    mls_group: &mut OpenMlsGroup,
    provider: &Provider,
    signer: &impl openmls_traits::signatures::Signer,
    catalogue: &[ApplicationComponentDefinition],
    limits: &xmtp_configuration::LimitsConfiguration,
    updates: Vec<(ComponentId, AppDataUpdateOperation)>,
) -> Result<(Vec<MlsMessageOut>, CommitMessageBundle), GroupAppDataError<Provider::StorageError>> {
    // Lazy-batching: we deliberately do NOT block on pre-existing
    // pending proposals. This helper queues a new `AppDataUpdate` then
    // commits via `consume_proposal_store(true)`, sweeping whatever
    // else is in the store — concurrent `AppDataUpdate`s (accumulated
    // into the dict by step 2), membership `Add` / `Remove`, etc. — all
    // into one commit, less what `omit_uncommittable_proposals` omitted.
    // That's the design: minimize commit count, let the producers of
    // those proposals decide if they need to force their own commit
    // (because they want to send a message right now or grant access
    // immediately). MLS guarantees consistent state
    // convergence on the wire regardless of which commit body carries
    // which proposal; the sender's intent ledger may carry less
    // information than the on-wire commit, but the producer of each
    // folded-in proposal already accepted that outcome by leaving it
    // pending instead of issuing its own commit.
    let mut proposals = Vec::with_capacity(updates.len());
    for (component_id, operation) in updates {
        let (proposal, _) = mls_group
            .propose_app_data_update(provider, signer, component_id.as_u16(), operation)
            .map_err(GroupAppDataError::Propose)?;
        proposals.push(proposal);
    }

    // Step 2: registry reconciliation and cleanup after any swept removal,
    // with the dict updates of every `AppDataUpdate` in the store (earlier
    // intents' queued proposals included) followed by the inline upkeep, so
    // the final dict bytes match what `process_message_with_app_data`
    // produces on the receive side. If OpenMLS ever changes
    // `consume_proposal_store(true)`'s sweep or `pending_proposals()`
    // ordering, receivers reject the commit with `WrongConfirmationTag`; the
    // AppDataUpdate E2E tests in `groups/tests/test_proposals.rs` catch that.
    let (upkeep, app_data_updates) =
        membership_upkeep::membership_upkeep(mls_group, catalogue, limits)
            .map_err(|e| GroupAppDataError::Upkeep(Box::new(e)))?;

    // Step 3: build a commit that consumes the proposal store. The
    // just-queued proposals are encoded as `ProposalRef`s; only the upkeep
    // is inline.
    let mut stage = crate::groups::mls_ext::commit_without_group_info(
        mls_group,
        provider.storage(),
        |builder| builder.consume_proposal_store(true).add_proposals(upkeep),
    )?;
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
    ApplyPayload(#[from] xmtp_mls_group_state::app_data::component_source::ComponentSourceError),
    /// Computing the removed-member cleanup for a swept removal failed, or
    /// the commit would grow the dictionary past its total.
    #[error("membership upkeep error: {0}")]
    Upkeep(#[source] Box<crate::groups::GroupError>),
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
            Self::Upkeep(e) => xmtp_common::retryable!(e),
            // Deterministic shape / staging-precondition failures —
            // CreateCommit is upstream-`false`, and ApplyPayload is a
            // sender-side encode failure that won't get better on
            // retry.
            Self::CreateCommit(_) | Self::ApplyPayload(_) => false,
        }
    }
}

/// The dictionary a commit built now starts from: the committed one with
/// the updates of the pending proposals it can carry under `cap` and
/// `dictionary_max` applied (see [`Committable::pending`]). The builder
/// passes [`proposal_cap`] and [`sender_dictionary_max`], so a proposal it
/// would omit does not count.
pub(crate) fn pending_dictionary(
    mls_group: &OpenMlsGroup,
    cap: usize,
    dictionary_max: usize,
) -> Result<PendingDictionary, GroupError> {
    let mut dictionary = PendingDictionary(
        mls_group
            .extensions()
            .app_data_dictionary()
            .map(|extension| extension.dictionary().clone())
            .unwrap_or_default(),
    );
    for (id, value) in Committable::pending(mls_group, cap, dictionary_max)?.states {
        let replaced = match value {
            // The bytes move into the dictionary, which wipes them on drop.
            Some(mut value) => dictionary
                .0
                .insert(id.as_u16(), std::mem::take(&mut *value)),
            None => dictionary.0.remove(&id.as_u16()),
        };
        wipe_component_value(replaced);
    }
    Ok(dictionary)
}

/// A copy of a group's AppData dictionary. Its `GROUP_ROOT_KEY` entry is the
/// group root, and in groups older clients created its `COMMIT_LOG_SIGNER`
/// entry is a private signing key, so every entry is wiped on drop.
pub(crate) struct PendingDictionary(AppDataDictionary);

impl std::ops::Deref for PendingDictionary {
    type Target = AppDataDictionary;

    fn deref(&self) -> &AppDataDictionary {
        &self.0
    }
}

impl Drop for PendingDictionary {
    fn drop(&mut self) {
        let ids: Vec<_> = self.0.entries().map(|entry| entry.id()).collect();
        for id in ids {
            wipe_component_value(self.0.remove(&id));
        }
    }
}

/// Wipe a component value taken out of an OpenMLS dictionary. The value
/// converts into its own buffer without a copy.
fn wipe_component_value(value: Option<impl Into<Vec<u8>>>) {
    if let Some(value) = value {
        value.into().zeroize();
    }
}

/// Read the committed component registry from the dictionary.
/// Missing registry entries reject component writes by default.
pub(crate) fn load_component_registry(
    mls_group: &OpenMlsGroup,
) -> Result<ComponentRegistry, ComponentSourceError> {
    load_component_registry_from_extensions(mls_group.extensions())
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
