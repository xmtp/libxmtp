//! Prepare and publish durable outgoing attempts.

use super::*;
use xmtp_db::group_intent::QueryPreparedEnvelope;

mod dependencies;
mod prepared;
mod rejection;
#[cfg(test)]
mod tests;
mod welcomes;
use dependencies::{PublishDependencies, PublishRequirements};
pub(crate) use prepared::{
    OutgoingPreparationError, PreparedAttempt, PreparedBase, PreparedProposal,
};

/// The next durable attempt, or immutable inputs that need external resolution.
enum NextPublish {
    Prepared(StoredGroupIntent, PreparedAttempt),
    Resolve(PublishRequirements),
}

impl<Context: XmtpSharedContext> MlsGroup<Context> {
    /// Persist each attempt with its crypto state before publication. An ambiguous
    /// result retries the saved bytes. A state-changing attempt blocks later work
    /// until ordered processing resolves it.
    #[xmtp_common::mls_span]
    pub(in crate::groups) async fn publish_intents(&self) -> Result<(), GroupError> {
        let mut sent = HashSet::new();
        let mut rejected_request = None;
        let upper_id = self
            .context
            .db()
            .find_group_intents(
                self.group_id,
                Some(vec![IntentState::ToPublish, IntentState::Published]),
                Some(IntentKind::all().collect()),
            )?
            .last()
            .map(|intent| intent.id);
        let Some(upper_id) = upper_id else {
            return Ok(());
        };

        loop {
            let next = crate::state_tx::state_write(self.context.mls_storage(), |tx| {
                tx.with_group(self.group_id, |group, storage| {
                    let intents = storage.db().find_group_intents(
                        self.group_id,
                        Some(vec![IntentState::ToPublish, IntentState::Published]),
                        Some(IntentKind::all().collect()),
                    )?;
                    // A prepared state change must reach ordered resolution first.
                    let blocking = intents.iter().find(|intent| {
                        intent.state == IntentState::Published
                            && intent.kind != IntentKind::SendMessage
                    });
                    let intent = blocking.or_else(|| {
                        intents
                            .iter()
                            .find(|intent| intent.id <= upper_id && !sent.contains(&intent.id))
                    });
                    let Some(intent) = intent else {
                        return Ok(Continue(None));
                    };
                    if sent.contains(&intent.id) {
                        return Ok(Continue(None));
                    }
                    if intent.state == IntentState::Published {
                        let bytes = storage
                            .db()
                            .prepared_envelopes(intent.id)?
                            .ok_or(OutgoingPreparationError::MissingPreparedAttempt(intent.id))?;
                        let attempt = PreparedAttempt::decode(&bytes)?;
                        attempt.validate_intent(intent)?;
                        return Ok(Continue(Some(NextPublish::Prepared(
                            intent.clone(),
                            attempt,
                        ))));
                    }
                    if storage.db().prepared_envelopes(intent.id)?.is_some() {
                        return Err(OutgoingPreparationError::InvalidPreparedAttempt.into());
                    }
                    let requirements = PublishRequirements::capture(group, intent)?;
                    Ok::<_, GroupError>(Continue(Some(NextPublish::Resolve(requirements))))
                })
            })?
            .into_continued();
            let Some(next) = next else {
                return rejected_request.map_or(Ok(()), Err);
            };
            let (intent, attempt) = match next {
                NextPublish::Prepared(intent, attempt) => (intent, attempt),
                NextPublish::Resolve(requirements) => {
                    let mut dependencies = self.resolve_publish_dependencies(&requirements).await?;
                    let result = self.prepare_publish_attempt(&requirements, &mut dependencies);
                    match result {
                        Err(GroupError::OutgoingPreparation(
                            OutgoingPreparationError::StateChanged,
                        )) => {
                            continue;
                        }
                        Err(error)
                            if matches!(
                                error,
                                GroupError::WrappedApi(
                                    xmtp_api::ApiError::EnvelopeTooLarge
                                        | xmtp_api::ApiError::UnitTooLarge
                                        | xmtp_api::ApiError::InvalidRequest(_)
                                )
                            ) =>
                        {
                            if self.reject_unprepared_request(&requirements)? {
                                sent.insert(requirements.intent.id);
                                rejected_request.get_or_insert(error);
                            }
                            continue;
                        }
                        Err(error) => return Err(error),
                        Ok(None) => {
                            sent.insert(requirements.intent.id);
                            continue;
                        }
                        Ok(Some(attempt)) => (requirements.intent, attempt),
                    }
                }
            };
            sent.insert(intent.id);
            if attempt.receipts.is_none() {
                // The writer and every mutable MLS object were dropped above.
                // An error keeps this exact attempt eligible for retry.
                let receipts = self
                    .context
                    .api()
                    .send_group_messages(vec![attempt.publish_unit()?])
                    .await?;
                self.record_publish_receipts(&intent, &attempt, receipts)?;
            }
            if intent.kind != IntentKind::SendMessage {
                return rejected_request.map_or(Ok(()), Err);
            }
        }
    }

    /// Fail a locally invalid request only if its intent and MLS base still match.
    /// The failed preparation has already rolled back all crypto changes.
    fn reject_unprepared_request(
        &self,
        requirements: &PublishRequirements,
    ) -> Result<bool, GroupError> {
        // The failed preparation transaction already rolled back its ratchets.
        // Only definite local request-shape errors can enter this path.
        crate::state_tx::state_write(self.context.mls_storage(), |tx| {
            tx.with_group(self.group_id, |group, storage| {
                let db = storage.db();
                let Some(current) =
                    Fetch::<StoredGroupIntent>::fetch(&db, &requirements.intent.id)?
                else {
                    return Ok(Continue(false));
                };
                if current.state != IntentState::ToPublish
                    || current.data != requirements.intent.data
                    || current.kind != requirements.intent.kind
                    || PreparedBase::capture(group)? != requirements.base
                    || db.prepared_envelopes(current.id)?.is_some()
                {
                    return Ok(Continue(false));
                }
                let message_id = calculate_message_id_for_intent(&current)?;
                db.set_group_intent_error_and_fail_msg(&current, message_id)?;
                Ok::<_, GroupError>(Continue(true))
            })
        })
        .map(TransactionOutcome::into_continued)
    }

    /// Reload and check the base under one writer, then atomically save crypto
    /// state, exact envelopes, proposal records, and the published intent.
    fn prepare_publish_attempt(
        &self,
        requirements: &PublishRequirements,
        dependencies: &mut PublishDependencies,
    ) -> Result<Option<PreparedAttempt>, GroupError> {
        crate::state_tx::state_write(self.context.mls_storage(), |tx| {
            tx.with_group(self.group_id, |group, storage| {
                let db = storage.db();
                let Some(intent) = Fetch::<StoredGroupIntent>::fetch(&db, &requirements.intent.id)?
                else {
                    return Ok(Continue(None));
                };
                if intent.state != IntentState::ToPublish {
                    return Err(OutgoingPreparationError::StateChanged.into());
                }
                if intent.data != requirements.intent.data
                    || intent.kind != requirements.intent.kind
                    || intent.should_push != requirements.intent.should_push
                    || PreparedBase::capture(group)? != requirements.base
                {
                    return Err(OutgoingPreparationError::StateChanged.into());
                }
                if db.prepared_envelopes(intent.id)?.is_some() {
                    return Err(OutgoingPreparationError::InvalidPreparedAttempt.into());
                }
                if group.pending_commit().is_some() {
                    return Err(OutgoingPreparationError::UnexpectedPendingCommit.into());
                }
                let has_pending_change = db
                    .find_group_intents(
                        self.group_id,
                        Some(vec![IntentState::Published]),
                        Some(IntentKind::all().collect()),
                    )?
                    .iter()
                    .any(|other| other.kind != IntentKind::SendMessage);
                if has_pending_change {
                    return Err(OutgoingPreparationError::StateChanged.into());
                }
                dependencies.validate_local(&self.context, storage, self.group_id)?;
                let original_proposals = group
                    .pending_proposals()
                    .map(|proposal| proposal.proposal_reference_ref().clone())
                    .collect::<Vec<_>>();
                let Some(data) =
                    self.get_publish_intent_data(storage, group, &intent, dependencies)?
                else {
                    if Fetch::<StoredGroupIntent>::fetch(&db, &intent.id)?
                        .is_some_and(|current| current.state == IntentState::ToPublish)
                    {
                        db.set_group_intent_processed(intent.id)?;
                    }
                    return Ok(Continue(None));
                };
                // Read the persisted proposal order under the same writer before
                // recording the exact outgoing batch.
                group.reload(storage)?;
                let new_proposals = group
                    .pending_proposals()
                    .filter(|proposal| {
                        !original_proposals.contains(proposal.proposal_reference_ref())
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                let non_proposal_payloads = usize::from(data.staged_commit.is_some())
                    + usize::from(intent.kind == IntentKind::SendMessage);
                if new_proposals.len() + non_proposal_payloads != data.payloads_to_publish.len() {
                    return Err(OutgoingPreparationError::InvalidPreparedAttempt.into());
                }
                // The pinned OpenMLS ProposalStore is insertion-ordered. The SQL
                // proposal-reference list keeps the same order across reloads.
                // Each sender emits standalone proposals first, in that order.
                let proposals = new_proposals
                    .iter()
                    .zip(&data.payloads_to_publish)
                    .map(|(proposal, payload)| {
                        Ok(PreparedProposal {
                            payload_hash: sha256(payload).to_vec(),
                            proposal: xmtp_db::db_serialize(proposal)?,
                        })
                    })
                    .collect::<Result<Vec<_>, GroupError>>()?;
                for proposal in new_proposals {
                    group
                        .remove_pending_proposal(storage, proposal.proposal_reference_ref())
                        .map_err(|error| match error {
                            openmls::group::RemoveProposalError::Storage(error) => {
                                GroupError::SqlKeyStore(error)
                            }
                            openmls::group::RemoveProposalError::ProposalNotFound => {
                                OutgoingPreparationError::InvalidPreparedAttempt.into()
                            }
                        })?;
                }
                let payload = data
                    .payloads_to_publish
                    .last()
                    .ok_or(GroupError::UninitializedResult)?;
                let envelopes = self.prepare_group_envelopes_in(
                    &db,
                    data.payloads_to_publish
                        .iter()
                        .map(|payload| (payload.as_slice(), data.should_send_push_notification))
                        .collect(),
                )?;
                let attempt = PreparedAttempt {
                    version: 2,
                    base: requirements.base.clone(),
                    payload_hash: sha256(payload).to_vec(),
                    envelopes: envelopes
                        .iter()
                        .map(prost::Message::encode_to_vec)
                        .collect(),
                    proposals,
                    receipts: None,
                    welcomes: None,
                    rejection: None,
                };
                // Validate size and shape before any ratchet writes can commit.
                attempt.publish_unit()?;
                let encoded = xmtp_db::db_serialize(&attempt)?;
                if !db.compare_and_set_prepared_envelopes(intent.id, None, Some(&encoded))? {
                    return Err(OutgoingPreparationError::StateChanged.into());
                }
                db.set_group_intent_published(
                    intent.id,
                    &attempt.payload_hash,
                    data.post_commit_action,
                    data.staged_commit,
                    data.group_epoch as i64,
                )?;
                Ok::<_, GroupError>(Continue(Some(attempt)))
            })
        })
        .map(TransactionOutcome::into_continued)
    }

    /// Create one attempt under the caller's writer. This performs no network I/O.
    /// `None` means that the intent makes no change to the current group.
    #[allow(clippy::type_complexity)]
    #[tracing::instrument(level = "trace", skip_all)]
    fn get_publish_intent_data(
        &self,
        storage: &impl XmtpMlsStorageProvider,
        openmls_group: &mut OpenMlsGroup,
        intent: &StoredGroupIntent,
        dependencies: &mut PublishDependencies,
    ) -> Result<Option<PublishIntentData>, GroupError> {
        let provider = XmtpOpenMlsProviderRef::new(storage);
        match intent.kind {
            IntentKind::UpdateGroupMembership => {
                let intent_data =
                    UpdateGroupMembershipIntentData::try_from(intent.data.as_slice())?;
                let signer = &self.context.identity().installation_keys;
                apply_update_group_membership_intent(
                    storage,
                    openmls_group,
                    intent_data,
                    dependencies.take_changes()?,
                    signer,
                )
            }
            IntentKind::SendMessage => {
                // We can safely assume all SendMessage intents have data
                let intent_data = SendMessageIntentData::from_bytes(intent.data.as_slice())?;
                // Pending proposals are handled at the API level (in send_message)
                // by committing them before creating the SendMessage intent
                let group_epoch = openmls_group.epoch().as_u64();
                let msg = openmls_group.create_message(
                    &provider,
                    &self.context.identity().installation_keys,
                    intent_data.message.as_slice(),
                )?;

                Ok(Some(PublishIntentData {
                    payloads_to_publish: vec![msg.tls_serialize_detached()?],
                    post_commit_action: None,
                    staged_commit: None,
                    should_send_push_notification: intent.should_push,
                    group_epoch,
                }))
            }
            IntentKind::KeyUpdate => {
                let keys = self.context.identity().installation_keys.clone();
                let (bundle, staged_commit, group_epoch) =
                    generate_prepared_commit(storage, openmls_group, |group, provider| {
                        group.self_update(provider, &keys, LeafNodeParameters::default())
                    })?;
                Ok(Some(PublishIntentData {
                    payloads_to_publish: vec![bundle.commit().tls_serialize_detached()?],
                    staged_commit,
                    post_commit_action: None,
                    should_send_push_notification: intent.should_push,
                    group_epoch,
                }))
            }
            IntentKind::MetadataUpdate => {
                let metadata_intent = UpdateMetadataIntentData::try_from(intent.data.clone())?;

                // Compare-and-swap guard. This runs on every publish attempt,
                // including the republish after an intent loses an epoch race,
                // which is exactly when the frozen `field_value` has gone
                // stale. Abandoning here is what stops the intent from
                // silently overwriting whatever landed in the meantime.
                //
                // Marked `Superseded` (not `Error`) and reported as
                // `Ok(None)`, so it is terminal without burning publish
                // attempts or aborting the publish loop for the other intents
                // queued on this group.
                if let Some(expected) = &metadata_intent.expected_field_value {
                    // A failed read is not evidence the guard was violated —
                    // leave the intent alone and let the normal error path
                    // surface whatever is actually wrong. The outer `Option`
                    // separates "unreadable" from "readable but unset".
                    if let Some(committed) =
                        Self::read_metadata_field(openmls_group, &metadata_intent.field_name)
                        && committed.as_deref() != Some(expected.as_str())
                    {
                        tracing::info!(
                            group_id = %self.group_id,
                            intent_id = intent.id,
                            field = %metadata_intent.field_name,
                            "abandoning guarded metadata update: committed value no longer matches"
                        );
                        storage.db().set_group_intent_superseded(intent.id)?;
                        return Ok(None);
                    }
                }

                // Route through AppDataUpdate only on migrated groups,
                // via the same `is_migrated_group` predicate the
                // UpdateAdminList / UpdatePermission gates use.
                // `is_migrated_group` (not `registry.is_empty()`) is the
                // correct migration signal: `ComponentRegistry::is_empty()`
                // ignores preserved-but-unrecognized entries, so a migrated
                // group whose entries were all tolerated as unrecognized
                // would misreport as empty and mis-route to legacy.
                let is_migrated = crate::groups::app_data::is_migrated_group(openmls_group);
                tracing::debug!(
                    group_id = %self.group_id,
                    is_migrated,
                    path = if is_migrated {
                        "app_data_update"
                    } else {
                        "legacy_gce"
                    },
                    "MetadataUpdate intent routing"
                );
                if is_migrated {
                    // Publish a STANDALONE AppDataUpdate proposal followed
                    // by a commit that references it (XIP §1.5.2 / §3.4).
                    // Both wire messages go in one publish batch — the
                    // proposal comes first so the receiver has it in its
                    // pending store before processing the commit.
                    use crate::groups::app_data::{
                        component_source::{
                            ComponentMutation, ComponentSourceError,
                            encode_app_data_update_payload, metadata_field_to_component_id,
                        },
                        stage_app_data_propose_and_commit,
                    };

                    let component_id = metadata_field_to_component_id(&metadata_intent.field_name)
                        .ok_or_else(|| {
                            GroupError::ComponentSource(ComponentSourceError::UnknownMetadataField(
                                metadata_intent.field_name.clone(),
                            ))
                        })?;

                    let payload = encode_app_data_update_payload(&ComponentMutation::Bytes {
                        component_id,
                        new_value: metadata_intent.field_value.as_bytes(),
                    })?;

                    let signer = self.context.identity().installation_keys.clone();
                    let ((proposal_msg, bundle), staged_commit, group_epoch) =
                        generate_prepared_commit(
                            storage,
                            openmls_group,
                            move |group, provider| -> Result<_, GroupError> {
                                Ok(stage_app_data_propose_and_commit(
                                    group,
                                    provider,
                                    &signer,
                                    component_id,
                                    payload,
                                )?)
                            },
                        )?;

                    let (commit, welcome, _group_info) = bundle.into_messages();
                    // A metadata-only AppDataUpdate commit has no add/remove
                    // proposals, so OpenMLS should never synthesize a welcome
                    // alongside it. If that ever changes, dropping it here
                    // would silently strand installations that expected one.
                    debug_assert!(
                        welcome.is_none(),
                        "MetadataUpdate via AppDataUpdate must not produce a welcome"
                    );
                    return Ok(Some(PublishIntentData {
                        payloads_to_publish: vec![
                            proposal_msg.tls_serialize_detached()?,
                            commit.tls_serialize_detached()?,
                        ],
                        staged_commit,
                        post_commit_action: None,
                        should_send_push_notification: intent.should_push,
                        group_epoch,
                    }));
                }

                let mutable_metadata_extensions = build_extensions_for_metadata_update(
                    openmls_group,
                    metadata_intent.field_name,
                    metadata_intent.field_value,
                )?;

                let keys = self.context.identity().installation_keys.clone();
                let ((commit, _, _), staged_commit, group_epoch) =
                    generate_prepared_commit(storage, openmls_group, |group, provider| {
                        group.update_group_context_extensions(
                            provider,
                            mutable_metadata_extensions.clone(),
                            &keys,
                        )
                    })?;

                let commit_bytes = commit.tls_serialize_detached()?;

                Ok(Some(PublishIntentData {
                    payloads_to_publish: vec![commit_bytes],
                    staged_commit,
                    post_commit_action: None,
                    should_send_push_notification: intent.should_push,
                    group_epoch,
                }))
            }
            IntentKind::UpdateAdminList => {
                let admin_list_update_intent =
                    UpdateAdminListIntentData::try_from(intent.data.clone())?;

                // Mirror the MetadataUpdate dual-routing gate: only
                // route through AppDataUpdate on groups whose AppData
                // dict has the `COMPONENT_REGISTRY` entry (the
                // bootstrap-commit marker). Otherwise stay on the
                // legacy GCE path so unmigrated peers continue to
                // validate via the legacy `GroupMutableMetadata`
                // extension. Single shared predicate via
                // `is_migrated_group` keeps every send/receive/validate
                // path honest about what "migrated" means.
                let is_migrated = crate::groups::app_data::is_migrated_group(openmls_group);
                tracing::debug!(
                    group_id = %self.group_id,
                    is_migrated,
                    path = if is_migrated {
                        "app_data_update"
                    } else {
                        "legacy_gce"
                    },
                    "UpdateAdminList intent routing"
                );
                if is_migrated {
                    let signer = self.context.identity().installation_keys.clone();
                    let publish =
                        crate::groups::app_data::sender_intents::apply_update_admin_list_app_data_intent(
                            storage,
                            openmls_group,
                            admin_list_update_intent,
                            signer,
                            intent.should_push,
                        )?;
                    return Ok(Some(publish));
                }

                // Legacy GCE path on unmigrated groups.
                let mutable_metadata_extensions = build_extensions_for_admin_lists_update(
                    openmls_group,
                    admin_list_update_intent,
                )?;

                let keys = self.context.identity().installation_keys.clone();
                let ((commit, _, _), staged_commit, group_epoch) =
                    generate_prepared_commit(storage, openmls_group, |group, provider| {
                        group.update_group_context_extensions(
                            provider,
                            mutable_metadata_extensions.clone(),
                            &keys,
                        )
                    })?;

                let commit_bytes = commit.tls_serialize_detached()?;

                Ok(Some(PublishIntentData {
                    payloads_to_publish: vec![commit_bytes],
                    staged_commit,
                    post_commit_action: None,
                    should_send_push_notification: intent.should_push,
                    group_epoch,
                }))
            }
            IntentKind::UpdatePermission => {
                let update_permissions_intent =
                    UpdatePermissionIntentData::try_from(intent.data.clone())?;

                // Mirror the MetadataUpdate / UpdateAdminList dual-
                // routing gate via the shared `is_migrated_group`
                // predicate.
                let is_migrated = crate::groups::app_data::is_migrated_group(openmls_group);
                tracing::debug!(
                    group_id = %self.group_id,
                    is_migrated,
                    path = if is_migrated {
                        "app_data_update"
                    } else {
                        "legacy_gce"
                    },
                    "UpdatePermission intent routing"
                );
                if is_migrated {
                    let signer = self.context.identity().installation_keys.clone();
                    let publish =
                        crate::groups::app_data::sender_intents::apply_update_permission_app_data_intent(
                            storage,
                            openmls_group,
                            update_permissions_intent,
                            signer,
                            intent.should_push,
                        )?;
                    return Ok(Some(publish));
                }

                // Legacy GCE path on unmigrated groups.
                let group_permissions_extensions = build_extensions_for_permissions_update(
                    openmls_group,
                    update_permissions_intent,
                )?;

                let keys = self.context.identity().installation_keys.clone();
                let ((commit, _, _), staged_commit, group_epoch) =
                    generate_prepared_commit(storage, openmls_group, |group, provider| {
                        group.update_group_context_extensions(
                            provider,
                            group_permissions_extensions.clone(),
                            &keys,
                        )
                    })?;

                let commit_bytes = commit.tls_serialize_detached()?;
                Ok(Some(PublishIntentData {
                    payloads_to_publish: vec![commit_bytes],
                    staged_commit,
                    post_commit_action: None,
                    should_send_push_notification: intent.should_push,
                    group_epoch,
                }))
            }
            IntentKind::ReaddInstallations => {
                let intent_data = ReaddInstallationsIntentData::try_from(intent.data.as_slice())?;
                let signer = &self.context.identity().installation_keys;
                apply_readd_installations_intent(
                    storage,
                    openmls_group,
                    intent_data,
                    dependencies.take_changes()?,
                    signer,
                )
            }
            IntentKind::ProposeMemberUpdate => {
                if !self.proposals_enabled(openmls_group) {
                    return Err(GroupError::from(CommitValidationError::ProposalsNotEnabled));
                }

                // Detect whether this is a migrated group. On
                // migrated groups, in addition to the Add/Remove
                // proposals below, we also emit an
                // `AppDataUpdate(GROUP_MEMBERSHIP)` proposal carrying
                // the membership delta. The subsequent
                // `CommitPendingProposals` intent sweeps everything
                // into a single commit. Bootstrap removed the legacy
                // `GROUP_MEMBERSHIP_EXTENSION_ID` extension, so the
                // legacy GCE proposal that `CommitPendingProposals`
                // would otherwise emit is no-op on migrated groups —
                // the AppData path carries the source of truth.
                //
                // Uses the canonical `is_migrated_group` predicate
                // (presence of the `COMPONENT_REGISTRY` entry) to
                // match every other send/receive/validate gate.
                let is_migrated = crate::groups::app_data::is_migrated_group(openmls_group);

                let intent_data = ProposeMemberUpdateIntentData::try_from(intent.data.as_slice())?;
                let group_epoch = openmls_group.epoch().as_u64();
                let signer = &self.context.identity().installation_keys;
                let mut proposal_payloads = Vec::new();

                // The membership the AppDataUpdate proposal will encode on
                // migrated groups. We mutate this as we process adds/removes
                // so it ends up reflecting only the inbox_ids that actually
                // got Add proposals (i.e. had at least one key package that
                // fetched successfully) plus any explicit removes — not the
                // raw intent.
                let extensions: Extensions<GroupContext> = openmls_group.extensions().clone();
                let old_group_membership = extract_group_membership(&extensions)?;
                let mut new_membership = old_group_membership.clone();

                // Handle adds
                if !intent_data.add_inbox_ids.is_empty() {
                    let latest_sequence_ids = &dependencies.latest_sequence_ids;
                    let changes_with_kps = dependencies
                        .changes
                        .as_ref()
                        .ok_or(OutgoingPreparationError::InvalidPreparedAttempt)?;

                    // If we failed to fetch key packages for all installations, error
                    if !changes_with_kps.failed_installations.is_empty()
                        && changes_with_kps.new_key_packages.is_empty()
                    {
                        return Err(GroupError::FailedToVerifyInstallations(
                            FailedInstallationIds(changes_with_kps.failed_installations.clone()),
                        ));
                    }

                    // Compute the inbox_ids that actually got at least one
                    // key package — those are the only ones that should
                    // appear in the AppDataUpdate payload below. An
                    // inbox_id whose installations all failed kp fetch has
                    // no MLS leaf in the commit, so claiming membership
                    // for it would diverge dict and tree state.
                    let added_inbox_ids = update_group_membership::inbox_ids_from_new_key_packages(
                        &changes_with_kps.new_key_packages,
                    );
                    for inbox_id in &intent_data.add_inbox_ids {
                        if !added_inbox_ids.contains(inbox_id) {
                            continue;
                        }
                        let sequence_id = latest_sequence_ids
                            .get(inbox_id.as_str())
                            .copied()
                            .ok_or(GroupError::MissingSequenceId)?;
                        new_membership.add(inbox_id.clone(), sequence_id as u64);
                    }

                    // Carry forward the failed-installations set on
                    // the local `new_membership`. On the legacy path
                    // this drives the GCE proposal that
                    // `CommitPendingProposals` emits against
                    // GROUP_MEMBERSHIP_EXTENSION_ID, where
                    // failed_installations is part of the wire form.
                    // On the migrated path the AppDataUpdate payload
                    // built by `build_group_membership_app_data_payload`
                    // intentionally does NOT propagate
                    // failed_installations (see that function's doc);
                    // we still set it here so the equality check at
                    // the AppDataUpdate emit site below
                    // (`old_group_membership != new_membership`)
                    // detects kp-failure-only deltas, and so the
                    // unmigrated and migrated branches share one
                    // `new_membership` value.
                    new_membership.failed_installations =
                        changes_with_kps.failed_installations.clone();

                    // Generate add proposals for each key package
                    for key_package in &changes_with_kps.new_key_packages {
                        let (proposal_msg, _proposal_ref) = openmls_group
                            .propose_add_member(&provider, signer, key_package)
                            .map_err(GroupError::ProposeAddMember)?;
                        proposal_payloads.push(proposal_msg.tls_serialize_detached()?);
                    }
                }

                // Handle removes
                if !intent_data.remove_inbox_ids.is_empty() {
                    let inbox_ids_to_remove: HashSet<_> =
                        intent_data.remove_inbox_ids.iter().cloned().collect();
                    let mut members_to_remove = Vec::new();
                    for member in openmls_group.members() {
                        let credential = BasicCredential::try_from(member.credential.clone())?;
                        let member_inbox_id = parse_credential(credential.identity())?;
                        if inbox_ids_to_remove.contains(&member_inbox_id) {
                            members_to_remove.push(member.index);
                        }
                    }

                    // Generate remove proposals for collected members
                    for member_index in members_to_remove {
                        let (proposal_msg, _proposal_ref) = openmls_group
                            .propose_remove_member(&provider, signer, member_index)
                            .map_err(GroupError::ProposeRemoveMember)?;
                        proposal_payloads.push(proposal_msg.tls_serialize_detached()?);
                    }

                    for inbox_id in &intent_data.remove_inbox_ids {
                        new_membership.remove(inbox_id);
                    }
                }

                if proposal_payloads.is_empty() {
                    tracing::debug!(
                        inbox_id = self.context.inbox_id(),
                        group_id = %self.group_id,
                        add_inbox_ids = ?intent_data.add_inbox_ids,
                        remove_inbox_ids = ?intent_data.remove_inbox_ids,
                        "ProposeMemberUpdate produced no proposals (members may already be in desired state)"
                    );
                    return Ok(None);
                }

                // On migrated groups, emit a parallel
                // `AppDataUpdate(GROUP_MEMBERSHIP)` proposal carrying
                // the membership delta we computed above (filtered to
                // kp-successful adds + explicit removes + carried-
                // forward failed_installations). The subsequent
                // `CommitPendingProposals` intent sweeps Add/Remove
                // and AppDataUpdate proposals into one commit and
                // skips the legacy GCE proposal (since the legacy
                // GROUP_MEMBERSHIP_EXTENSION_ID is gone post-bootstrap).
                if is_migrated && old_group_membership != new_membership {
                    use crate::groups::mls_sync::update_group_membership::build_group_membership_app_data_payload;

                    let payload = build_group_membership_app_data_payload(
                        &old_group_membership,
                        &new_membership,
                    )?;
                    let (proposal_msg, _) = openmls_group
                        .propose_app_data_update(
                            &provider,
                            signer,
                            xmtp_mls_common::app_data::component_id::ComponentId::GROUP_MEMBERSHIP
                                .as_u16(),
                            openmls::messages::proposals::AppDataUpdateOperation::Update(
                                payload.into(),
                            ),
                        )
                        .map_err(GroupError::Proposal)?;
                    proposal_payloads.push(proposal_msg.tls_serialize_detached()?);
                }

                // Note: The GroupContextExtensions proposal to update membership is created
                // by CommitPendingProposals, not here (and is no-op on migrated groups since
                // the legacy GROUP_MEMBERSHIP_EXTENSION_ID is gone).

                Ok(Some(PublishIntentData {
                    payloads_to_publish: proposal_payloads,
                    staged_commit: None,
                    post_commit_action: None,
                    should_send_push_notification: intent.should_push,
                    group_epoch,
                }))
            }
            IntentKind::ProposeGroupContextExtensions => {
                // No proposals_enabled guard here — ProposeGroupContextExtensions is used
                // by enable_proposals() to bootstrap proposal support on the group.
                //
                // This arm handles the legacy propose-by-reference flow
                // only. The one-time AppData-migration bootstrap commit
                // is routed through [`IntentKind::BootstrapMigration`]
                // instead — keep them distinct so the commit-producing
                // path never fires accidentally when a caller just
                // wants a standalone GCE proposal.
                let intent_data =
                    ProposeGroupContextExtensionsIntentData::try_from(intent.data.as_slice())?;
                let group_epoch = openmls_group.epoch().as_u64();

                // Deserialize the extensions using tls_codec
                use openmls::prelude::tls_codec::Deserialize;
                let new_extensions =
                    Extensions::tls_deserialize(&mut intent_data.extensions_bytes.as_slice())?;

                let signer = &self.context.identity().installation_keys;
                let (proposal_msg, _proposal_ref) = openmls_group
                    .propose_group_context_extensions(&provider, new_extensions, signer)
                    .map_err(GroupError::Proposal)?;

                Ok(Some(PublishIntentData {
                    payloads_to_publish: vec![proposal_msg.tls_serialize_detached()?],
                    staged_commit: None,
                    post_commit_action: None,
                    should_send_push_notification: intent.should_push,
                    group_epoch,
                }))
            }
            IntentKind::BootstrapMigration => {
                // One-time AppData-migration bootstrap: bundles one
                // GCE proposal (that strips the four legacy XMTP
                // extensions and adds AppDataDictionary to
                // RequiredCapabilities) with an `AppDataUpdate`
                // proposal per well-known component.
                // Routed on an explicit [`IntentKind::BootstrapMigration`]
                // rather than shape-sniffing `ProposeGroupContextExtensions`
                // payloads so a future non-bootstrap GCE intent with
                // similar extension shape can't accidentally trigger
                // the bootstrap path.
                //
                // Receive-side validation lives in
                // `validated_commit.rs` (`is_bootstrap_commit` routes
                // into `validate_bootstrap_and_build`, which drives
                // `bootstrap_validator::validate_bootstrap_commit`).
                if crate::groups::app_data::is_migrated_group(openmls_group) {
                    return Ok(None);
                }
                let _intent_data =
                    ProposeGroupContextExtensionsIntentData::try_from(intent.data.as_slice())?;
                let mut new_extensions = openmls_group.extensions().clone();
                for extension in [
                    ExtensionType::Unknown(xmtp_configuration::MUTABLE_METADATA_EXTENSION_ID),
                    ExtensionType::Unknown(xmtp_configuration::GROUP_PERMISSIONS_EXTENSION_ID),
                    ExtensionType::Unknown(xmtp_configuration::GROUP_MEMBERSHIP_EXTENSION_ID),
                    ExtensionType::ImmutableMetadata,
                ] {
                    new_extensions.remove(extension);
                }
                crate::groups::update_required_capabilities_for_bootstrap(&mut new_extensions)?;
                let component_values = dependencies
                    .bootstrap_components
                    .take()
                    .ok_or(OutgoingPreparationError::InvalidPreparedAttempt)?;

                let signer = self.context.identity().installation_keys.clone();
                let (bundle, staged_commit, group_epoch): (
                    openmls::prelude::CommitMessageBundle,
                    Option<Vec<u8>>,
                    u64,
                ) = generate_prepared_commit(
                    storage,
                    openmls_group,
                    move |group, provider| -> Result<_, GroupError> {
                        Ok(crate::groups::app_data::migration::stage_bootstrap_commit(
                            group,
                            provider,
                            &signer,
                            &component_values,
                            new_extensions,
                        )?)
                    },
                )?;
                let (commit, _, _) = bundle.into_messages();
                Ok(Some(PublishIntentData {
                    payloads_to_publish: vec![commit.tls_serialize_detached()?],
                    staged_commit,
                    post_commit_action: None,
                    should_send_push_notification: intent.should_push,
                    group_epoch,
                }))
            }
            IntentKind::AppDataUpdate => {
                // Generic AppData write: full-replace or 3-way-merge
                // delta. The handler decodes the intent payload, computes
                // the final wire bytes (running residual computation for
                // DeltaWithBase), and stages an
                // `AppDataUpdate(component_id, payload)` proposal +
                // commit. All AppData writes go through the same path.
                if !crate::groups::app_data::is_migrated_group(openmls_group) {
                    return Err(GroupError::ProposalsNotSupported(
                        "AppDataUpdate intent requires the group to be migrated to AppData. \
                         Call `enable_proposals` first."
                            .into(),
                    ));
                }
                let intent_data = crate::groups::intents::AppDataUpdateIntentData::try_from(
                    intent.data.as_slice(),
                )?;
                let signer = self.context.identity().installation_keys.clone();
                let publish =
                    crate::groups::app_data::sender_intents::apply_app_data_update_intent(
                        storage,
                        openmls_group,
                        intent_data,
                        signer,
                        intent.should_push,
                    )?;
                Ok(Some(publish))
            }
            IntentKind::CommitPendingProposals => {
                use xmtp_id::key_package::VerifiedKeyPackageV2;

                let _intent_data =
                    CommitPendingProposalsIntentData::try_from(intent.data.as_slice())?;

                // Check if there are any pending proposals to commit
                if openmls_group.pending_proposals().next().is_none() {
                    tracing::debug!("No pending proposals to commit");
                    return Ok(None);
                }

                let signer = &self.context.identity().installation_keys;

                // Get current group membership
                let current_extensions: Extensions<GroupContext> =
                    openmls_group.extensions().clone();
                let current_membership = extract_group_membership(&current_extensions)?;

                // Analyze pending proposals to determine membership changes and collect installations
                let mut inbox_ids_to_add: Vec<String> = Vec::new();
                let mut inbox_ids_to_remove: Vec<String> = Vec::new();
                let mut installations_to_welcome: Vec<Installation> = Vec::new();
                let mut key_packages_to_add: Vec<openmls::key_packages::KeyPackage> = Vec::new();

                for proposal_ref in openmls_group.pending_proposals() {
                    match proposal_ref.proposal() {
                        Proposal::Add(add_proposal) => {
                            let key_package = add_proposal.key_package();
                            let credential = BasicCredential::try_from(
                                key_package.leaf_node().credential().clone(),
                            )?;
                            let inbox_id = parse_credential(credential.identity())?;
                            if !inbox_ids_to_add.contains(&inbox_id)
                                && current_membership.get(&inbox_id).is_none()
                            {
                                inbox_ids_to_add.push(inbox_id);

                                // Collect the key package for proposal support check
                                key_packages_to_add.push(key_package.clone());

                                // Extract installation info from the key package for welcome sending
                                if let Ok(verified_kp) =
                                    VerifiedKeyPackageV2::try_from(key_package.clone())
                                    && let Ok(installation) =
                                        Installation::from_verified_key_package(&verified_kp)
                                {
                                    installations_to_welcome.push(installation);
                                }
                            }
                        }
                        Proposal::Remove(remove_proposal) => {
                            if let Some(member) = openmls_group.member_at(remove_proposal.removed())
                            {
                                let credential = BasicCredential::try_from(member.credential)?;
                                let inbox_id = parse_credential(credential.identity())?;
                                if !inbox_ids_to_remove.contains(&inbox_id) {
                                    inbox_ids_to_remove.push(inbox_id);
                                }
                            }
                        }
                        _ => {}
                    }
                }

                // Build the updated membership
                let mut new_membership = current_membership.clone();

                // Add new members with their latest sequence IDs
                if !inbox_ids_to_add.is_empty() {
                    let latest_sequence_ids = &dependencies.latest_sequence_ids;

                    for inbox_id in &inbox_ids_to_add {
                        let sequence_id = latest_sequence_ids
                            .get(inbox_id.as_str())
                            .copied()
                            .ok_or(GroupError::MissingSequenceId)?;
                        new_membership.add(inbox_id.clone(), sequence_id as u64);
                    }
                }

                // Remove members
                for inbox_id in &inbox_ids_to_remove {
                    new_membership.remove(inbox_id);
                }

                // Compute failed installations for added members so they are
                // tracked in the GCE and can be retried in future updates.
                // calculate_membership_changes_with_keypackages merges
                // current_membership.failed_installations with any new failures.
                if !inbox_ids_to_add.is_empty() {
                    let changes_with_kps = dependencies.take_changes()?;

                    new_membership.failed_installations = changes_with_kps.failed_installations;
                }

                // Determine if membership changes require a GCE proposal
                let membership_changed =
                    !inbox_ids_to_add.is_empty() || !inbox_ids_to_remove.is_empty();

                // Check if a pending GCE already has the correct membership.
                // Compare only the `members` field, not `failed_installations`,
                // since failed_installations can change between Phase 1 and Phase 2
                // due to transient network conditions.
                let has_pending_gce_with_membership = membership_changed
                    && openmls_group.pending_proposals().any(|p| {
                        if let Proposal::GroupContextExtensions(gce) = p.proposal() {
                            extract_group_membership(gce.extensions())
                                .map(|m| m.members == new_membership.members)
                                .unwrap_or(false)
                        } else {
                            false
                        }
                    });

                // Detect migrated state. On migrated groups the legacy
                // `GROUP_MEMBERSHIP_EXTENSION_ID` is gone; membership
                // updates flow as AppDataUpdate proposals (already
                // emitted by `ProposeMemberUpdate` and sitting in the
                // pending queue). The GCE proposal that this branch
                // would otherwise build to update the legacy extension
                // is skipped — the commit just sweeps the pending
                // AppDataUpdate alongside the Add/Remove proposals.
                let is_migrated_for_commit =
                    crate::groups::app_data::is_migrated_extensions(openmls_group.extensions());

                if membership_changed && !has_pending_gce_with_membership && !is_migrated_for_commit
                {
                    // === GCE needed: batch GCE proposal + commit in one publish ===
                    // Create GCE proposal and commit locally inside one
                    // generate_prepared_commit call, returning both payloads.

                    // Check for any existing pending GCE (might have non-membership changes).
                    // If one exists, use its extensions as base to preserve those changes.
                    let base_extensions = openmls_group
                        .pending_proposals()
                        .find_map(|p| {
                            if let Proposal::GroupContextExtensions(gce) = p.proposal() {
                                Some(gce.extensions().clone())
                            } else {
                                None
                            }
                        })
                        .unwrap_or_else(|| openmls_group.extensions().clone());

                    // Build extensions with membership update on top of the base
                    let mut new_extensions = base_extensions;
                    new_extensions
                        .add_or_replace(build_group_membership_extension(&new_membership))?;

                    // Check if proposals need to be disabled due to new members not supporting them
                    let proposals_currently_enabled = self.proposals_enabled(openmls_group);
                    if proposals_currently_enabled && !key_packages_to_add.is_empty() {
                        let new_members_support_proposals = self
                            .validate_key_packages_support_proposals(&key_packages_to_add)
                            .is_ok();

                        if !new_members_support_proposals {
                            tracing::info!(
                                "Disabling proposals: new members don't support the AppData dictionary extension"
                            );
                            new_extensions.remove(ExtensionType::AppDataDictionary);
                            update_required_capabilities_for_proposals(&mut new_extensions, false)?;
                        }
                    }

                    let new_membership_for_filter = new_membership.clone();
                    let signer = self.context.identity().installation_keys.clone();
                    let ((gce_payload, bundle), staged_commit, group_epoch) =
                        generate_prepared_commit(
                            storage,
                            openmls_group,
                            |group, provider| -> Result<_, GroupError> {
                                // Create GCE proposal locally
                                let (gce_msg, _) = group
                                    .propose_group_context_extensions(
                                        provider,
                                        new_extensions.clone(),
                                        &signer,
                                    )
                                    .map_err(GroupError::Proposal)?;
                                let gce_payload = gce_msg.tls_serialize_detached()?;

                                // Create commit consuming all proposals (including GCE).
                                // `build_commit_with_pending_app_data_updates` pre-computes
                                // the AppData dictionary writes from any queued
                                // `AppDataUpdate` proposals so the commit builder can
                                // apply them in lockstep. See plan §11.
                                let bundle = build_commit_with_pending_app_data_updates(
                                    group,
                                    provider,
                                    &signer,
                                    |qp| match qp.proposal() {
                                        Proposal::GroupContextExtensions(gce) => {
                                            extract_group_membership(gce.extensions())
                                                .map(|m| {
                                                    m.members == new_membership_for_filter.members
                                                })
                                                .unwrap_or(false)
                                        }
                                        _ => true,
                                    },
                                )?;

                                Ok((gce_payload, bundle))
                            },
                        )?;

                    let (commit, maybe_welcome, _group_info) = bundle.into_messages();
                    let staged_commit =
                        staged_commit.ok_or_else(|| GroupError::MissingPendingCommit)?;

                    let post_commit_action = match maybe_welcome {
                        Some(welcome_message) => {
                            tracing::debug!(
                                num_installations = installations_to_welcome.len(),
                                "Creating post commit action with installations to welcome"
                            );
                            Some(PostCommitAction::from_welcome(
                                welcome_message,
                                installations_to_welcome,
                            )?)
                        }
                        None => None,
                    };

                    tracing::debug!(
                        inbox_ids_to_add = ?inbox_ids_to_add,
                        inbox_ids_to_remove = ?inbox_ids_to_remove,
                        "Publishing batched GCE proposal + commit"
                    );

                    Ok(Some(PublishIntentData {
                        payloads_to_publish: vec![gce_payload, commit.tls_serialize_detached()?],
                        staged_commit: Some(staged_commit),
                        post_commit_action: post_commit_action.map(|action| action.to_bytes()),
                        should_send_push_notification: intent.should_push,
                        group_epoch,
                    }))
                } else {
                    // === No GCE needed (or matching GCE already in store) ===
                    // Create and publish the commit directly.

                    let new_membership_for_filter = new_membership.clone();
                    let (bundle, staged_commit, group_epoch) = generate_prepared_commit(
                        storage,
                        openmls_group,
                        |group, provider| -> Result<_, GroupError> {
                            // See plan §11 — this commit path also has to thread
                            // queued `AppDataUpdate` proposals' dict writes in
                            // lockstep with the commit build.
                            build_commit_with_pending_app_data_updates(
                                group,
                                provider,
                                signer,
                                |qp| match qp.proposal() {
                                    Proposal::GroupContextExtensions(gce) => {
                                        if !membership_changed {
                                            // No membership changes: include all GCEs
                                            return true;
                                        }
                                        // Only include GCE with correct membership
                                        // (compare members only, not failed_installations)
                                        extract_group_membership(gce.extensions())
                                            .map(|m| m.members == new_membership_for_filter.members)
                                            .unwrap_or(false)
                                    }
                                    _ => true,
                                },
                            )
                        },
                    )?;
                    let (commit, maybe_welcome, _group_info) = bundle.into_messages();

                    let staged_commit =
                        staged_commit.ok_or_else(|| GroupError::MissingPendingCommit)?;

                    // Build post commit action if there's a welcome message
                    let post_commit_action = match maybe_welcome {
                        Some(welcome_message) => {
                            tracing::debug!(
                                num_installations = installations_to_welcome.len(),
                                "Creating post commit action with installations to welcome"
                            );
                            Some(PostCommitAction::from_welcome(
                                welcome_message,
                                installations_to_welcome,
                            )?)
                        }
                        None => None,
                    };

                    tracing::debug!(
                        membership_changed,
                        "Publishing commit with pending proposals"
                    );

                    Ok(Some(PublishIntentData {
                        payloads_to_publish: vec![commit.tls_serialize_detached()?],
                        staged_commit: Some(staged_commit),
                        post_commit_action: post_commit_action.map(|action| action.to_bytes()),
                        should_send_push_notification: intent.should_push,
                        group_epoch,
                    }))
                }
            }
        }
    }
}
