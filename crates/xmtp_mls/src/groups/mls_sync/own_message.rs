//! Staging local intents and processing our own messages.

use super::*;

impl<Context> MlsGroup<Context>
where
    Context: XmtpSharedContext,
{
    /// Validate the current prepared attempt using verified local proofs.
    // implements: GMOD-036
    pub(super) fn stage_and_validate_intent(
        &self,
        db: &impl DbQuery,
        mls_group: &openmls::group::MlsGroup,
        intent: &StoredGroupIntent,
        envelope: &GroupMessage,
    ) -> Result<Option<(StagedCommit, ValidatedCommit)>, IntentResolutionError> {
        let GroupMessage {
            message, cursor, ..
        } = &envelope;
        let group_epoch = mls_group.epoch();
        let message_epoch = message.epoch();

        // Staged state is usable only with the exact current prepared base.
        if envelope.is_commit() && intent.state == IntentState::Published {
            let attempt = (|| -> Result<publish::PreparedAttempt, GroupError> {
                let bytes = db.prepared_envelopes(intent.id)?.ok_or(
                    publish::OutgoingPreparationError::MissingPreparedAttempt(intent.id),
                )?;
                let attempt = publish::PreparedAttempt::decode(&bytes)?;
                attempt.validate_intent(intent)?;
                if message_epoch == group_epoch && !attempt.base.matches_epoch(mls_group) {
                    return Err(publish::OutgoingPreparationError::InvalidPreparedAttempt.into());
                }
                Ok(attempt)
            })();
            attempt.map_err(|error| IntentResolutionError {
                processing_error: GroupMessageProcessingError::PreparedAttempt(Box::new(error)),
            })?;
        }

        match intent.kind {
            // GCE proposal phase of CommitPendingProposals: no staged_commit means the
            // message coming back is our GCE proposal, not a commit. Validate epoch only.
            IntentKind::CommitPendingProposals if intent.staged_commit.is_none() => {
                Self::validate_message_epoch(
                    self.context.inbox_id(),
                    intent.id,
                    group_epoch,
                    message_epoch,
                    MAX_PAST_EPOCHS,
                )
                .map_err(|err| IntentResolutionError {
                    processing_error: err,
                })?;
            }

            IntentKind::KeyUpdate
            | IntentKind::UpdateGroupMembership
            | IntentKind::UpdateAdminList
            | IntentKind::MetadataUpdate
            | IntentKind::UpdatePermission
            | IntentKind::ReaddInstallations
            | IntentKind::CommitPendingProposals
            | IntentKind::BootstrapMigration
            | IntentKind::AppDataUpdate => {
                if let Some(published_in_epoch) = intent.published_in_epoch {
                    let group_epoch = group_epoch.as_u64() as i64;
                    let message_epoch = message_epoch.as_u64() as i64;

                    // TODO(rich): Merge into validate_message_epoch()
                    if message_epoch != group_epoch {
                        tracing::warn!(
                            inbox_id = self.context.inbox_id(),
                            installation_id = %self.context.installation_id(),
                            group_id = %self.group_id,
                            cursor = %cursor,
                            intent_id = intent.id,
                            intent_kind = %intent.kind,
                            "Intent for msg = [{cursor}] was published in epoch {} with local save intent epoch of {} but group is currently in epoch {}",
                            message_epoch,
                            published_in_epoch,
                            group_epoch
                        );
                        let processing_error = if message_epoch < group_epoch {
                            GroupMessageProcessingError::OldEpoch(
                                message_epoch as u64,
                                group_epoch as u64,
                            )
                        } else {
                            GroupMessageProcessingError::FutureEpoch(
                                message_epoch as u64,
                                group_epoch as u64,
                            )
                        };

                        return Err(IntentResolutionError { processing_error });
                    }

                    let staged_commit = intent
                        .staged_commit
                        .as_ref()
                        .map_or(
                            Err(GroupMessageProcessingError::IntentMissingStagedCommit),
                            |staged_commit| decode_staged_commit(staged_commit),
                        )
                        .map_err(|err| {
                            // If we can't retrieve the cached staged commit from the intent, we can't
                            // apply it. It is indeterminate whether other members were able to apply it
                            // or not - if they did apply it, then we are forked.
                            tracing::error!(
                                inbox_id = self.context.inbox_id(),
                                installation_id = %self.context.installation_id(),
                                group_id = %self.group_id,
                                cursor = %cursor,
                                intent_id = intent.id,
                                intent_kind = %intent.kind,
                                "Error decoding staged commit for intent, now may be forked: {err:?}",
                            );
                            IntentResolutionError {
                                processing_error: err,
                            }
                        })?;

                    tracing::info!(
                        "[{}] Validating commit for intent {}. Message timestamp: ({})/{}",
                        self.context.inbox_id(),
                        intent.id,
                        envelope.timestamp(),
                        envelope.created_ns
                    );

                    // We just published this commit ourselves, so the committer
                    // is our own leaf — no need to consult the staged commit's
                    // path update field.
                    let maybe_validated_commit = ValidatedCommit::from_staged_commit_local(
                        &self.context,
                        db,
                        &staged_commit,
                        mls_group.own_leaf_index(),
                        mls_group,
                        envelope.sequence_id(),
                    );

                    let validated_commit = match maybe_validated_commit {
                        Err(err) => {
                            tracing::error!(
                                inbox_id = self.context.inbox_id(),
                                installation_id = %self.context.installation_id(),
                                group_id = %self.group_id,
                                cursor = %cursor,
                                intent_id = intent.id,
                                intent_kind = %intent.kind,
                                "Error validating commit for own message. Intent ID [{}]: {err:?}",
                                intent.id,
                            );
                            return Err(IntentResolutionError {
                                processing_error: GroupMessageProcessingError::CommitValidation(
                                    err,
                                ),
                            });
                        }
                        Ok(validated_commit) => validated_commit,
                    };

                    return Ok(Some((staged_commit, validated_commit)));
                }
            }

            IntentKind::SendMessage
            | IntentKind::ProposeMemberUpdate
            | IntentKind::ProposeGroupContextExtensions => {
                // Proposals and messages don't produce commits, just validate epoch
                Self::validate_message_epoch(
                    self.context.inbox_id(),
                    intent.id,
                    group_epoch,
                    message_epoch,
                    MAX_PAST_EPOCHS,
                )
                .map_err(|err| IntentResolutionError {
                    processing_error: err,
                })?;
            }
        }

        Ok(None)
    }

    /// Apply an own message with the current writer. The caller commits its intent
    /// only after all state changes succeed. Errors leave the trial state unchanged.
    #[tracing::instrument(level = "trace", skip_all)]
    pub(super) fn process_own_message(
        &self,
        mls_group: &mut OpenMlsGroup,
        commit: Option<(StagedCommit, ValidatedCommit)>,
        intent: &StoredGroupIntent,
        envelope: &GroupMessage,
        storage: &impl XmtpMlsStorageProvider,
        event_writer: &impl xmtp_events::EventWriter<crate::subscriptions::internal::InternalEvent>,
    ) -> Result<Option<Vec<u8>>, IntentResolutionError> {
        if intent.state == IntentState::Committed
            || intent.state == IntentState::Processed
            || intent.state == IntentState::Error
        {
            tracing::warn!(
                group_id = %self.group_id,
                intent_id = intent.id,
                intent_kind = %intent.kind,
                intent_state = ?intent.state,
                cursor = envelope.cursor.0,
                "Skipping already processed intent {} of kind {} because it is in state {:?}",
                intent.id,
                intent.kind,
                intent.state
            );
            return Err(IntentResolutionError {
                processing_error: GroupMessageProcessingError::IntentAlreadyProcessed,
            });
        }

        // GCE proposal phase of CommitPendingProposals: the GCE proposal was received back
        // from the network. Re-queue the intent to create the actual commit in the next sync.
        if intent.kind == IntentKind::CommitPendingProposals && commit.is_none() {
            tracing::info!(
                "CommitPendingProposals: GCE proposal received back, re-queuing to create commit"
            );
            return Err(IntentResolutionError {
                processing_error: GroupMessageProcessingError::PreCommitProposalPhaseComplete,
            });
        }

        let message_epoch = envelope.message.epoch();
        let GroupMessage { cursor, .. } = envelope;
        let envelope_timestamp_ns = envelope.timestamp();

        tracing::debug!(
            inbox_id = self.context.inbox_id(),
            installation_id = %self.context.installation_id(),
            group_id = %self.group_id,
            cursor = %cursor,
            intent_id = intent.id,
            intent_kind = %intent.kind,
            "[{}]-[{}] processing own message for intent {} / {}, message_epoch: {}",
            self.context.inbox_id(),
            hex::encode(self.group_id),
            intent.id,
            intent.kind,
            message_epoch.clone()
        );

        if let Some((staged_commit, validated_commit)) = commit {
            tracing::info!(
                "[{}] merging pending commit for intent {}",
                self.context.inbox_id(),
                intent.id
            );

            if let Err(err) = mls_group.merge_staged_commit_logged(
                &XmtpOpenMlsProviderRef::new(storage),
                staged_commit,
                &validated_commit,
                cursor.0 as i64,
                self.context.server_configuration().commit_log_enabled(),
            ) {
                tracing::error!("error merging commit: {err}");
                return Err(IntentResolutionError {
                    processing_error: err,
                });
            }
            Self::mark_readd_requests_as_responded(
                storage,
                &self.group_id,
                &validated_commit.readded_installations,
                cursor.0 as i64,
            )
            .map_err(|err| IntentResolutionError {
                processing_error: err.into(),
            })?;

            // If no error committing the change, write a transcript message
            let msg = self
                .save_transcript_message(
                    validated_commit.clone(),
                    envelope_timestamp_ns as u64,
                    *cursor,
                    storage,
                )
                .map_err(|err| IntentResolutionError {
                    processing_error: err,
                })?;

            self.emit_commit_events(
                &validated_commit,
                mls_group.is_active(),
                Some(intent.id),
                storage,
                event_writer,
            )
            .map_err(|err| IntentResolutionError {
                processing_error: err,
            })?;

            // Clean up pending_remove list for removed members
            self.clean_pending_remove_list(storage, &validated_commit.removed_inboxes);

            // Handle super_admin status changes
            self.handle_super_admin_status_change(
                storage,
                mls_group,
                &validated_commit.metadata_changes,
            );

            if let Some((_, payload)) = &msg {
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

            return Ok(msg.map(|(m, _)| m.id));
        }

        let id: Option<Vec<u8>> = calculate_message_id_for_intent(intent)
            .map_err(GroupMessageProcessingError::Intent)
            .map_err(|err| {
                if !err.is_retryable() {
                    tracing::error!(
                        "Message identifier not found for intent {} with kind {}, {err:?}",
                        intent.id,
                        intent.kind
                    );
                }
                IntentResolutionError {
                    processing_error: err,
                }
            })?;
        let Some(id) = id else {
            // The message is likely to be a legacy envelope, probably from legacy device sync.
            // We don't need to set the delivery status for these.
            return Ok(None);
        };
        tracing::debug!("setting message @cursor=[{}] to published", envelope.cursor);
        let message_expire_at_ns = Self::get_message_expire_at_ns(mls_group);
        let previous_status = storage
            .db()
            .get_group_message(&id)
            .map_err(|err| IntentResolutionError {
                processing_error: GroupMessageProcessingError::Storage(err.into()),
            })?
            .map(|message| message.delivery_status);
        storage
            .db()
            .set_delivery_status_to_published(
                &id,
                envelope_timestamp_ns as u64,
                envelope.cursor,
                message_expire_at_ns,
            )
            .map_err(|err| IntentResolutionError {
                processing_error: GroupMessageProcessingError::Storage(err),
            })?;
        if previous_status == Some(DeliveryStatus::Unpublished) {
            self.emit_message_status_changed(
                id.clone(),
                xmtp_events::MessageStatus::Unpublished,
                xmtp_events::MessageStatus::Published,
                event_writer,
            );
        }
        self.process_own_leave_request_message(mls_group, storage, &id, event_writer);
        if !self.conversation_type.is_virtual() {
            event_writer.emit(
                None,
                Some(
                    crate::subscriptions::internal::InternalEvent::MessageStored {
                        group_id: self.group_id,
                        message_id: id.clone(),
                        expires_at_ns: message_expire_at_ns,
                        is_sync: false,
                    },
                ),
            );
        }
        Ok(Some(id))
    }
}
