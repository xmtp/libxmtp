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
        // Nothing this client prepared is published once
        // it has latched. The intent stays queued for a client that can.
        self.context.server_configuration().check()?;
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
                    .send_group_messages(vec![attempt.publish_unit(self.context.api().limits())?])
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
                dependencies.validate_local(storage)?;
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
                attempt.publish_unit(self.context.api().limits())?;
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
    // implements: META-065
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

                // Publish a standalone AppDataUpdate proposal followed by a
                // commit that references it. Both wire messages go in one
                // publish batch, with the proposal first.
                use crate::groups::app_data::{
                    component_source::{
                        ComponentMutation, ComponentSourceError, encode_app_data_update_payload,
                        metadata_field_to_component_id,
                    },
                    stage_app_data_propose_and_commit,
                };

                let component_id = metadata_field_to_component_id(&metadata_intent.field_name)
                    .ok_or_else(|| {
                        GroupError::ComponentSource(ComponentSourceError::UnknownMetadataField(
                            metadata_intent.field_name.clone(),
                        ))
                    })?;

                let value = xmtp_mls_common::app_data::creation::encode_metadata_attribute_value(
                    component_id,
                    &metadata_intent.field_value,
                )
                .map_err(crate::groups::app_data::migration::BootstrapSynthesisError::from)?;
                let payload = encode_app_data_update_payload(&ComponentMutation::Bytes {
                    component_id,
                    new_value: &value,
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
                debug_assert!(
                    welcome.is_none(),
                    "MetadataUpdate via AppDataUpdate must not produce a welcome"
                );
                Ok(Some(PublishIntentData {
                    payloads_to_publish: vec![
                        proposal_msg.tls_serialize_detached()?,
                        commit.tls_serialize_detached()?,
                    ],
                    staged_commit,
                    post_commit_action: None,
                    should_send_push_notification: intent.should_push,
                    group_epoch,
                }))
            }
            IntentKind::UpdateAdminList => {
                let admin_list_update_intent =
                    UpdateAdminListIntentData::try_from(intent.data.clone())?;

                let signer = self.context.identity().installation_keys.clone();
                let publish =
                    crate::groups::app_data::sender_intents::apply_update_admin_list_app_data_intent(
                        storage,
                        openmls_group,
                        admin_list_update_intent,
                        signer,
                        intent.should_push,
                    )?;
                Ok(Some(publish))
            }
            IntentKind::UpdatePermission => {
                let update_permissions_intent =
                    UpdatePermissionIntentData::try_from(intent.data.clone())?;

                let signer = self.context.identity().installation_keys.clone();
                let publish =
                    crate::groups::app_data::sender_intents::apply_update_permission_app_data_intent(
                        storage,
                        openmls_group,
                        update_permissions_intent,
                        signer,
                        intent.should_push,
                    )?;
                Ok(Some(publish))
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

                    // Keep the failed-installations set on the local
                    // membership so the equality check detects key-package
                    // failure-only deltas.
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

                if old_group_membership != new_membership {
                    use crate::groups::mls_sync::update_group_membership::build_group_membership_app_data_payload;

                    let payload = build_group_membership_app_data_payload(
                        &storage.db(),
                        openmls_group,
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

                Ok(Some(PublishIntentData {
                    payloads_to_publish: proposal_payloads,
                    staged_commit: None,
                    post_commit_action: None,
                    should_send_push_notification: intent.should_push,
                    group_epoch,
                }))
            }
            IntentKind::ProposeGroupContextExtensions => {
                Err(CommitValidationError::UnsupportedProposalType(
                    ProposalType::GroupContextExtensions,
                )
                .into())
            }
            IntentKind::BootstrapMigration => Err(CommitValidationError::UnsupportedProposalType(
                ProposalType::GroupContextExtensions,
            )
            .into()),
            IntentKind::AppDataUpdate => {
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

                // Collect installations for any Add proposals in the pending set.
                let mut inbox_ids_to_add: Vec<String> = Vec::new();
                let mut installations_to_welcome: Vec<Installation> = Vec::new();

                for proposal_ref in openmls_group.pending_proposals() {
                    if let Proposal::Add(add_proposal) = proposal_ref.proposal() {
                        let key_package = add_proposal.key_package();
                        let credential = BasicCredential::try_from(
                            key_package.leaf_node().credential().clone(),
                        )?;
                        let inbox_id = parse_credential(credential.identity())?;
                        if !inbox_ids_to_add.contains(&inbox_id)
                            && current_membership.get(&inbox_id).is_none()
                        {
                            inbox_ids_to_add.push(inbox_id);

                            // Extract installation info from the key package for welcome sending.
                            if let Ok(verified_kp) =
                                VerifiedKeyPackageV2::try_from(key_package.clone())
                                && let Ok(installation) =
                                    Installation::from_verified_key_package(&verified_kp)
                            {
                                installations_to_welcome.push(installation);
                            }
                        }
                    }
                }

                let (bundle, staged_commit, group_epoch) =
                    generate_prepared_commit(storage, openmls_group, |group, provider| {
                        build_commit_with_pending_app_data_updates(group, provider, signer, |_| {
                            true
                        })
                    })?;
                let (commit, maybe_welcome, _group_info) = bundle.into_messages();
                let staged_commit =
                    staged_commit.ok_or_else(|| GroupError::MissingPendingCommit)?;
                let post_commit_action = match maybe_welcome {
                    Some(welcome_message) => Some(PostCommitAction::from_welcome(
                        welcome_message,
                        installations_to_welcome,
                    )?),
                    None => None,
                };

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
