//! Validating and processing messages from other members.

use super::*;
use xmtp_mls_validation::commit::CommitRuleError;

impl<Context> MlsGroup<Context>
where
    Context: XmtpSharedContext,
{
    #[tracing::instrument(level = "trace", skip_all)]
    pub(super) fn validate_and_process_external_message(
        &self,
        mls_group: &mut OpenMlsGroup,
        envelope: &GroupMessage,
        storage: &impl XmtpMlsStorageProvider,
        deferred_events: &mut DeferredEvents,
    ) -> Result<MessageIdentifier, GroupMessageProcessingError> {
        #[cfg(any(test, feature = "test-utils"))]
        {
            use crate::utils::test_mocks_helpers::maybe_mock_wrong_epoch_for_tests;
            maybe_mock_wrong_epoch_for_tests()?;
        }

        let provider = XmtpOpenMlsProviderRef::new(storage);

        let GroupMessage {
            cursor, message, ..
        } = envelope;
        let envelope_timestamp_ns = envelope.timestamp();

        let processed_message = crate::groups::app_data::process_message_with_app_data(
            mls_group,
            &provider,
            message.clone(),
            self.context.version_info().pkg_semver(),
        )
        .map_err(GroupMessageProcessingError::from_app_data_processing)?;

        let (sender_inbox_id, sender_installation_id) =
            extract_message_sender(mls_group, &processed_message, envelope_timestamp_ns as u64)?;

        tracing::info!(
            inbox_id = self.context.inbox_id(),
            installation_id = %self.context.installation_id(),sender_inbox_id = sender_inbox_id,
            sender_installation_id = hex::encode(&sender_installation_id),
            group_id = %self.group_id,
            group_epoch = mls_group.epoch().as_u64(),
            message_epoch = processed_message.epoch().as_u64(),
            msg_group_id = hex::encode(processed_message.group_id().as_slice()),
            cursor = %cursor,
            "[{}] extracted sender inbox id: {}",
            self.context.inbox_id(),
            sender_inbox_id
        );

        let validated_commit = match &processed_message.content() {
            ProcessedMessageContent::StagedCommitMessage(staged_commit) => {
                // OpenMLS already verified the framing signature against the
                // sender's leaf during `process_message`, and `extract_message_sender`
                // above asserted `Sender::Member` — so this match is exhaustive
                // for the cases that reach here.
                let committer_leaf_index = match processed_message.sender() {
                    openmls::prelude::Sender::Member(idx) => *idx,
                    _ => {
                        return Err(GroupMessageProcessingError::CommitValidation(
                            CommitValidationError::Rule(CommitRuleError::ActorNotMember),
                        ));
                    }
                };
                let validated_commit = ValidatedCommit::from_staged_commit_local(
                    &self.context,
                    &storage.db(),
                    staged_commit,
                    committer_leaf_index,
                    mls_group,
                    envelope.sequence_id(),
                )?;

                Some(validated_commit)
            }
            ProcessedMessageContent::ProposalMessage(queued_proposal) => {
                self.validate_received_proposal(mls_group, queued_proposal)?;
                None
            }
            _ => None,
        };

        self.process_external_message(
            mls_group,
            processed_message,
            envelope,
            validated_commit,
            storage,
            deferred_events,
        )
    }

    /// Apply the same policy to received own and external proposals.
    pub(super) fn validate_received_proposal(
        &self,
        group: &OpenMlsGroup,
        proposal: &openmls::group::QueuedProposal,
    ) -> Result<(), GroupMessageProcessingError> {
        if let Some(version) = xmtp_mls_common::app_data::protocol_floor::committed_floor_exceeding(
            group,
            self.context.version_info().pkg_semver(),
        ) {
            return Err(
                CommitValidationError::Rule(CommitRuleError::ProtocolVersionTooLow(version)).into(),
            );
        }
        let policies =
            crate::groups::group_permissions::policy_set_from_dictionary(group.extensions())
                .map_err(|error| {
                    CommitValidationError::installed_state(CommitValidationError::Rule(
                        CommitRuleError::GroupMutablePermissions(error),
                    ))
                })?;
        let seed =
            xmtp_mls_common::app_data::component_source::read_group_metadata_from_dict(group)
                .map_err(CommitValidationError::installed_state)?
                .ok_or_else(|| {
                    CommitValidationError::installed_state(
                        xmtp_mls_common::group_metadata::GroupMetadataError::MissingExtension,
                    )
                })?;
        let immutable = xmtp_mls_common::group_metadata::GroupMetadata::try_from(
            xmtp_proto::xmtp::mls::message_contents::GroupMetadataV1 {
                conversation_type: seed.conversation_type,
                creator_inbox_id: seed.creator_inbox_id,
                creator_account_address: String::new(),
                dm_members: seed.dm_members,
                oneshot_message: seed.oneshot,
            },
        )
        .map_err(CommitValidationError::installed_state)?;
        let mutable =
            xmtp_mls_common::app_data::component_source::extract_group_mutable_metadata_capability_aware(
                group,
            )
            .map_err(|error| {
                CommitValidationError::installed_state(
                    xmtp_mls_common::group_mutable_metadata::GroupMutableMetadataError::from(error),
                )
            })?;
        validate_proposal(proposal, group, &policies.policies, &immutable, &mutable)?;
        Ok(())
    }

    /// Process an external message
    /// returns a MessageIdentifier, identifying the message processed if any.
    #[tracing::instrument(level = "trace", skip_all)]
    fn process_external_message(
        &self,
        mls_group: &mut OpenMlsGroup,
        processed_message: ProcessedMessage,
        message_envelope: &GroupMessage,
        validated_commit: Option<ValidatedCommit>,
        storage: &impl XmtpMlsStorageProvider,
        deferred_events: &mut DeferredEvents,
    ) -> Result<MessageIdentifier, GroupMessageProcessingError> {
        let GroupMessage { cursor, .. } = &message_envelope;
        let envelope_timestamp_ns = message_envelope.timestamp();
        let msg_epoch = processed_message.epoch().as_u64();
        let msg_group_id = processed_message.group_id().as_slice().to_vec();
        let (sender_inbox_id, sender_installation_id) =
            extract_message_sender(mls_group, &processed_message, envelope_timestamp_ns as u64)?;

        let mut identifier = MessageIdentifierBuilder::from(message_envelope);
        match processed_message.into_content() {
            ProcessedMessageContent::ApplicationMessage(application_message) => {
                log_event!(
                    Event::MLSReceivedApplicationMessage,
                    self.context.installation_id(),
                    inbox_id = self.context.inbox_id(),
                    sender_inbox_id,
                    sender_installation_id,
                    group_id = self.group_id,
                    epoch = mls_group.epoch().as_u64(),
                    message_epoch = msg_epoch,
                    msg_group_id,
                    cursor = %cursor,
                );
                let message_bytes = application_message.into_bytes();

                let mut bytes = Bytes::from(message_bytes);
                let envelope = PlaintextEnvelope::decode(&mut bytes)?;

                match envelope.content {
                    Some(Content::V1(V1 {
                        idempotency_key,
                        content,
                    })) => {
                        let message_id =
                            calculate_message_id(self.group_id, &content, &idempotency_key);
                        let queryable_content_fields =
                            Self::extract_queryable_content_fields(&content);

                        let message = StoredGroupMessage {
                            id: message_id.clone(),
                            group_id: self.group_id,
                            decrypted_message_bytes: content,
                            sent_at_ns: envelope_timestamp_ns,
                            kind: GroupMessageKind::Application,
                            sender_installation_id,
                            sender_inbox_id: sender_inbox_id.clone(),
                            delivery_status: DeliveryStatus::Published,
                            content_type: queryable_content_fields.content_type,
                            version_major: queryable_content_fields.version_major,
                            version_minor: queryable_content_fields.version_minor,
                            authority_id: queryable_content_fields.authority_id,
                            reference_id: queryable_content_fields.reference_id,
                            sequence_id: cursor.0 as i64,
                            envelope_hash: None,
                            expiry_ns: None,
                            expire_at_ns: Self::get_message_expire_at_ns(mls_group),
                            inserted_at_ns: 0, // Will be set by database
                            should_push: true,
                            // Persist the key from the wire envelope — the exact
                            // key this message id was derived from.
                            idempotency_key,
                        };
                        message.store_or_ignore(&storage.db())?;
                        identifier.internal_id(message_id);

                        // A disappearing message was just persisted with a known
                        // future deadline; wake the disappearing worker after the
                        // txn commits so it re-arms its timer to that deadline.
                        if message.expire_at_ns.is_some() {
                            deferred_events.wake_worker(WorkerKind::DisappearingMessages);
                        }

                        // If this message was sent by us on another installation, check if it
                        // belongs to a sync group, and if it is - notify the worker.
                        if sender_inbox_id == self.context.inbox_id() {
                            tracing::info!(
                                installation_id = hex::encode(self.context.installation_id()),
                                "new sync group message event"
                            );
                            if let Some(StoredGroup {
                                conversation_type: ConversationType::Sync,
                                ..
                            }) = storage.db().find_group(&self.group_id)?
                            {
                                // Send this event after the transaction completes
                                deferred_events.add_worker_event(SyncWorkerEvent::NewSyncGroupMsg);
                            }
                        }
                        if message.content_type == ContentType::LeaveRequest {
                            self.process_leave_request_message(
                                mls_group,
                                storage,
                                &message,
                                Some(deferred_events),
                            )?;
                        }

                        if message.content_type == ContentType::DeleteMessage {
                            self.process_delete_message(mls_group, storage, &message)?;
                        }

                        Ok::<_, GroupMessageProcessingError>(())
                    }
                    Some(Content::V2(V2 { .. })) => {
                        // V2 was used for DeviceSync V1, which is now removed.
                        // Device Sync V2 reverted back to using V1 envelopes.
                        Ok::<_, GroupMessageProcessingError>(())
                    }
                    None => {
                        return Err(GroupMessageProcessingError::InvalidPayload);
                    }
                }
            }
            ProcessedMessageContent::ProposalMessage(proposal_ptr) => {
                tracing::debug!(
                    inbox_id = self.context.inbox_id(),
                    installation_id = %self.context.installation_id(),
                    group_id = %self.group_id,
                    proposal_type = ?proposal_ptr.proposal().proposal_type(),
                    "Received and storing proposal in proposal store"
                );
                // Explicitly persist the proposal to the key store so it survives group reloads.
                // process_message() only stores proposals in-memory; without this call,
                // they are lost when the group is reloaded from storage.
                mls_group.store_pending_proposal(storage, *proposal_ptr)?;
                Ok(())
            }
            ProcessedMessageContent::ExternalJoinProposalMessage(_external_proposal_ptr) => {
                Ok(())
                // intentionally left blank.
            }
            ProcessedMessageContent::OwnPrivateMessage => {
                // Our own fanned-back private message with no matching
                // published intent (the intent path handles the normal case —
                // this arm is reached only once the intent record is gone).
                // The own sender ratchet is encryption-only, so the content
                // is undecryptable by design; upstream surfaces it as a typed
                // skip hint where older openmls produced an opaque
                // decryption error.
                tracing::info!(
                    inbox_id = self.context.inbox_id(),
                    installation_id = %self.context.installation_id(),
                    group_id = %self.group_id,
                    cursor = %cursor,
                    "skipping own fanned-back private message without a matching intent"
                );
                Err(GroupMessageProcessingError::OwnMessageWithoutAttempt)
            }
            ProcessedMessageContent::OwnPendingCommit => {
                // Only produced for public-framed commits; unreachable under
                // libxmtp's pure-ciphertext wire format policy.
                Err(GroupMessageProcessingError::UnexpectedProcessedContent(
                    "OwnPendingCommit under a ciphertext-only wire format policy",
                ))
            }
            ProcessedMessageContent::UnresolvedAppDataCommit(_) => {
                // `process_message_with_app_data` resolves app-data commits
                // into `StagedCommitMessage` before returning; this cannot
                // reach the apply phase.
                Err(GroupMessageProcessingError::UnexpectedProcessedContent(
                    "UnresolvedAppDataCommit escaped process_message_with_app_data",
                ))
            }
            ProcessedMessageContent::StagedCommitMessage(staged_commit) => {
                let staged_commit = *staged_commit;
                let validated_commit =
                    validated_commit.expect("Needs to be present when this is a staged commit");

                log_event!(
                    Event::MLSReceivedStagedCommit,
                    self.context.installation_id(),
                    inbox_id = self.context.inbox_id(),
                    sender_inbox = sender_inbox_id,
                    sender_installation_id,
                    group_id = self.group_id,
                    epoch = mls_group.epoch().as_u64(),
                    message_epoch = msg_epoch,
                    msg_group_id,
                    cursor = %cursor,
                    hash = #message_envelope.payload_hash
                );

                identifier.group_context(staged_commit.group_context().clone());

                mls_group.merge_staged_commit_logged(
                    &XmtpOpenMlsProviderRef::new(storage),
                    staged_commit,
                    &validated_commit,
                    cursor.0 as i64,
                    self.context.server_configuration().commit_log_enabled(),
                )?;

                Self::mark_readd_requests_as_responded(
                    storage,
                    &self.group_id,
                    &validated_commit.readded_installations,
                    cursor.0 as i64,
                )?;

                let transcript = self.save_transcript_message(
                    validated_commit.clone(),
                    envelope_timestamp_ns as u64,
                    *cursor,
                    storage,
                )?;

                // remove left/removed members from the pending_remove list
                self.clean_pending_remove_list(storage, &validated_commit.removed_inboxes);

                // Handle super_admin status changes for the current user
                // If promoted: check for pending remove members and mark group accordingly
                // If demoted: clear the pending leave request status
                self.handle_super_admin_status_change(
                    storage,
                    mls_group,
                    &validated_commit.metadata_changes,
                );

                if let Some((msg, payload)) = transcript {
                    identifier.internal_id(msg.id);

                    log_event!(
                        Event::MLSProcessedStagedCommit,
                        self.context.installation_id(),
                        group_id = self.group_id,
                        epoch = mls_group.epoch().as_u64(),
                        epoch_auth = mls_group.epoch_authenticator().as_slice(),
                        actor_installation_id = validated_commit.actor.installation_id,
                        added_inboxes = $payload.added_inboxes,
                        removed_inboxes = $payload.removed_inboxes,
                        left_inboxes = $payload.left_inboxes,
                        metadata_changes = $payload.metadata_field_changes,
                        cursor = cursor.0,

                    );
                }

                Ok(())
            }
        }?;
        identifier.build()
    }
}
