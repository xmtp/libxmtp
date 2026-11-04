//! Publish intent data: the payload and post-commit data an intent publishes.

use super::*;

impl<Context: XmtpSharedContext> MlsGroup<Context> {
    /// Create one attempt under the caller's writer. This performs no network I/O.
    /// `None` means that the intent makes no change to the current group.
    #[allow(clippy::type_complexity)]
    #[tracing::instrument(level = "trace", skip_all)]
    // implements: META-065
    pub(super) fn get_publish_intent_data(
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
                    &self
                        .context
                        .server_configuration()
                        .configuration()
                        .application_components,
                    signer,
                )
            }
            IntentKind::SendMessage => {
                // We can safely assume all SendMessage intents have data
                let intent_data = SendMessageIntentData::from_bytes(intent.data.as_slice())?;
                // An older build queued this intent before the creation check.
                // implements: GMOD-054
                // The envelope content can hold a device-sync HMAC root key;
                // the decoded envelope wipes it on drop.
                if let Ok(PlaintextEnvelope {
                    content: Some(Content::V1(v1)),
                }) = PlaintextEnvelope::decode(intent_data.message.as_slice())
                    && Self::is_reserved_transcript_content(&v1.content)
                {
                    return Err(GroupError::ReservedTranscriptContentType);
                }
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
                        let (upkeep, updates) =
                            crate::groups::app_data::membership_upkeep::membership_upkeep(
                                group,
                                &self
                                    .context
                                    .server_configuration()
                                    .configuration()
                                    .application_components,
                            )?;
                        let mut stage = crate::groups::mls_ext::commit_without_group_info(
                            group,
                            provider.storage(),
                            |builder| {
                                builder
                                    .leaf_node_parameters(LeafNodeParameters::default())
                                    .consume_proposal_store(true)
                                    .add_proposals(upkeep)
                            },
                        )
                        .map_err(CommitToPendingProposalsError::from)?;
                        stage.with_app_data_dictionary_updates(updates);
                        let bundle = stage
                            .build(provider.rand(), provider.crypto(), &keys, |_| true)
                            .map_err(CommitToPendingProposalsError::from)?
                            .stage_commit(provider)
                            .map_err(CommitToPendingProposalsError::from)?;
                        Ok::<_, GroupError>(bundle)
                    })?;
                let (commit, welcome, _group_info) = bundle.into_messages();
                let post_commit_action = update_group_membership::welcome_post_commit_action(
                    welcome,
                    staged_commit.as_deref(),
                )?;
                Ok(Some(PublishIntentData {
                    payloads_to_publish: vec![commit.tls_serialize_detached()?],
                    staged_commit,
                    post_commit_action,
                    should_send_push_notification: intent.should_push,
                    group_epoch,
                }))
            }
            IntentKind::MetadataUpdate => {
                let metadata_intent =
                    UpdateMetadataIntentData::try_from(intent.data.as_slice().to_vec())?;

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
                use crate::groups::app_data::stage_app_data_propose_and_commit;
                use xmtp_mls_group_state::app_data::component_source::metadata_field_to_component_id;

                let component_id = metadata_field_to_component_id(&metadata_intent.field_name)
                    .ok_or_else(|| {
                        GroupError::ComponentSource(ComponentSourceError::UnknownMetadataField(
                            metadata_intent.field_name.clone(),
                        ))
                    })?;

                let payload =
                    xmtp_mls_group_state::app_data::creation::encode_metadata_attribute_value(
                        component_id,
                        &metadata_intent.field_value,
                    )
                    .map_err(crate::groups::app_data::migration::BootstrapSynthesisError::from)?;

                let signer = self.context.identity().installation_keys.clone();
                let catalogue = &self
                    .context
                    .server_configuration()
                    .configuration()
                    .application_components;
                let ((proposal_msg, bundle), staged_commit, group_epoch) =
                    generate_prepared_commit(
                        storage,
                        openmls_group,
                        // `payload` can be the commit-log signer. It stays in
                        // `Zeroizing` until it moves into OpenMLS, so a
                        // return before this closure runs still wipes it.
                        move |group, provider| -> Result<_, GroupError> {
                            let mut payload = payload;
                            Ok(stage_app_data_propose_and_commit(
                                group,
                                provider,
                                &signer,
                                catalogue,
                                component_id,
                                std::mem::take(&mut *payload),
                            )?)
                        },
                    )?;

                let (commit, welcome, _group_info) = bundle.into_messages();
                let post_commit_action = update_group_membership::welcome_post_commit_action(
                    welcome,
                    staged_commit.as_deref(),
                )?;
                Ok(Some(PublishIntentData {
                    payloads_to_publish: vec![
                        proposal_msg.tls_serialize_detached()?,
                        commit.tls_serialize_detached()?,
                    ],
                    staged_commit,
                    post_commit_action,
                    should_send_push_notification: intent.should_push,
                    group_epoch,
                }))
            }
            IntentKind::UpdateAdminList => {
                let admin_list_update_intent =
                    UpdateAdminListIntentData::try_from(intent.data.as_slice().to_vec())?;

                let signer = self.context.identity().installation_keys.clone();
                let publish =
                    crate::groups::app_data::sender_intents::apply_update_admin_list_app_data_intent(
                        storage,
                        openmls_group,
                        admin_list_update_intent,
                        &self
                            .context
                            .server_configuration()
                            .configuration()
                            .application_components,
                        signer,
                        intent.should_push,
                    )?;
                Ok(Some(publish))
            }
            IntentKind::UpdatePermission => {
                let update_permissions_intent =
                    UpdatePermissionIntentData::try_from(intent.data.as_slice().to_vec())?;

                let signer = self.context.identity().installation_keys.clone();
                let publish =
                    crate::groups::app_data::sender_intents::apply_update_permission_app_data_intent(
                        storage,
                        openmls_group,
                        update_permissions_intent,
                        &self
                            .context
                            .server_configuration()
                            .configuration()
                            .application_components,
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
                    &self
                        .context
                        .server_configuration()
                        .configuration()
                        .application_components,
                    signer,
                )
            }
            IntentKind::ProposeMemberUpdate => {
                let intent_data = ProposeMemberUpdateIntentData::try_from(intent.data.as_slice())?;
                update_group_membership::reject_super_admin_removal(
                    openmls_group,
                    &intent_data.remove_inbox_ids,
                )?;
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
                        if openmls_group.pending_proposals().any(|proposal| {
                            matches!(proposal.proposal(), Proposal::Add(add) if add.key_package() == key_package)
                        }) {
                            continue;
                        }
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
                        if openmls_group.pending_proposals().any(|proposal| {
                            matches!(proposal.proposal(), Proposal::Remove(remove) if remove.removed() == member_index)
                        }) {
                            continue;
                        }
                        let (proposal_msg, _proposal_ref) = openmls_group
                            .propose_remove_member(&provider, signer, member_index)
                            .map_err(GroupError::ProposeRemoveMember)?;
                        proposal_payloads.push(proposal_msg.tls_serialize_detached()?);
                    }

                    for inbox_id in &intent_data.remove_inbox_ids {
                        new_membership.remove(inbox_id);
                    }
                }

                if old_group_membership != new_membership {
                    use crate::groups::mls_sync::update_group_membership::build_group_membership_app_data_payload;

                    let payload = build_group_membership_app_data_payload(
                        &storage.db(),
                        openmls_group,
                        &old_group_membership,
                        &new_membership,
                    )?;
                    if !update_group_membership::has_pending_membership_delta(
                        openmls_group,
                        &payload,
                    )? {
                        let (proposal_msg, _) = openmls_group
                            .propose_app_data_update(
                                &provider,
                                signer,
                                xmtp_mls_group_state::app_data::component_id::ComponentId::GROUP_MEMBERSHIP
                                    .as_u16(),
                                openmls::messages::proposals::AppDataUpdateOperation::Update(
                                    payload.into(),
                                ),
                            )
                            .map_err(GroupError::Proposal)?;
                        proposal_payloads.push(proposal_msg.tls_serialize_detached()?);
                    }
                }

                // A repeated request can already be represented by accepted
                // proposals. Keep their original references and proposers.
                if proposal_payloads.is_empty() {
                    return Ok(None);
                }

                Ok(Some(PublishIntentData {
                    payloads_to_publish: proposal_payloads,
                    staged_commit: None,
                    post_commit_action: None,
                    should_send_push_notification: intent.should_push,
                    group_epoch,
                }))
            }
            IntentKind::ProposeGroupContextExtensions => Err(CommitValidationError::Rule(
                CommitRuleError::UnsupportedProposalType(ProposalType::GroupContextExtensions),
            )
            .into()),
            IntentKind::BootstrapMigration => Err(CommitValidationError::Rule(
                CommitRuleError::UnsupportedProposalType(ProposalType::GroupContextExtensions),
            )
            .into()),
            IntentKind::AppDataUpdate => {
                let intent_data = crate::groups::intents::AppDataUpdateIntentData::try_from(
                    intent.data.as_slice(),
                )?;
                let own =
                    xmtp_mls_group_state::inbox_id::InboxId::from_hex(self.context.inbox_id())
                        .map_err(|e| GroupError::ComponentSource(e.into()))?;
                crate::groups::app_data::sender_intents::apply_app_data_update_intent(
                    storage,
                    openmls_group,
                    intent_data,
                    own,
                    &self
                        .context
                        .server_configuration()
                        .configuration()
                        .application_components,
                    self.context.identity().installation_keys.clone(),
                    intent.should_push,
                )
            }
            IntentKind::CommitPendingProposals => {
                let _intent_data =
                    CommitPendingProposalsIntentData::try_from(intent.data.as_slice())?;

                // Check if there are any pending proposals to commit
                // An omitted proposal expires with the epoch, so a commit
                // that carries no proposal is still valid work.
                if openmls_group.pending_proposals().next().is_none()
                    && !dependencies.selection.active
                    && !dependencies.omitted_uncommittable
                {
                    tracing::debug!("No pending proposals to commit");
                    return Ok(None);
                }

                let signer = &self.context.identity().installation_keys;

                let (bundle, staged_commit, group_epoch) =
                    generate_prepared_commit(storage, openmls_group, |group, provider| {
                        build_commit_with_pending_app_data_updates(
                            group,
                            provider,
                            signer,
                            &self
                                .context
                                .server_configuration()
                                .configuration()
                                .application_components,
                            |_| true,
                        )
                    })?;
                let (commit, maybe_welcome, _group_info) = bundle.into_messages();
                let staged_commit =
                    staged_commit.ok_or_else(|| GroupError::MissingPendingCommit)?;
                let post_commit_action = update_group_membership::welcome_post_commit_action(
                    maybe_welcome,
                    Some(&staged_commit),
                )?;

                Ok(Some(PublishIntentData {
                    payloads_to_publish: vec![commit.tls_serialize_detached()?],
                    staged_commit: Some(staged_commit),
                    post_commit_action,
                    should_send_push_notification: intent.should_push,
                    group_epoch,
                }))
            }
        }
    }
}
