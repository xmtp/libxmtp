//! Sending, preparing, and querying messages.

use super::mls_sync::publish::OutgoingPreparationError;
use super::*;
use crate::messages::enrichment::find_valid_deletion;

/// An installation refresh before a send applies its membership change even
/// when the Welcomes for the installations it adds cannot be sent. That
/// outcome is logged and dropped here, so the send goes on; the installations
/// stay without a Welcome, as the SEND Known limitations describe.
fn unsent_welcomes_do_not_stop_a_send(result: Result<(), GroupError>) -> Result<(), GroupError> {
    match result {
        Err(GroupError::OutgoingPreparation(OutgoingPreparationError::WelcomesUnsendable)) => {
            tracing::warn!("installations added before a send have no Welcome");
            Ok(())
        }
        other => other,
    }
}

impl<Context> MlsGroup<Context>
where
    Context: XmtpSharedContext,
{
    /// Send a message on this users XMTP [`Client`](crate::client::Client).
    #[xmtp_common::mls_span]
    pub async fn send_message(
        &self,
        message: &[u8],
        opts: send_message_opts::SendMessageOpts,
    ) -> Result<Vec<u8>, GroupError> {
        if !self.is_active()? {
            tracing::warn!("Unable to send a message on an inactive group.");
            return Err(GroupError::GroupInactive);
        }

        // Refuse before any network work or pending proposal commit.
        Self::reject_reserved_transcript_content(message)?;

        self.ensure_not_paused().await?;
        // implements: DMS-020
        self.ensure_dm_peer().await?;
        let update_interval_ns = Some(SEND_MESSAGE_UPDATE_INSTALLATIONS_INTERVAL_NS);
        unsent_welcomes_do_not_stop_a_send(
            self.maybe_update_installations(update_interval_ns).await,
        )?;

        // Check for pending proposals and commit them first
        // OpenMLS blocks message creation when there are pending proposals
        self.commit_pending_proposals_if_any().await?;

        let message_id = self.prepare_message(message, opts)?;

        self.sync_until_last_intent_resolved().await?;

        // implicitly set group consent state to allowed
        self.update_consent_state(ConsentState::Allowed)?;

        Ok(message_id)
    }

    /// Checks for pending MLS proposals and commits them if any exist.
    /// OpenMLS blocks message creation when there are pending proposals,
    /// so we need to commit them first.
    async fn commit_pending_proposals_if_any(&self) -> Result<(), GroupError> {
        let (has_pending, has_pending_add) = self.with_group_snapshot(|openmls_group| {
            Ok((
                openmls_group.pending_proposals().next().is_some(),
                openmls_group.pending_proposals().any(|proposal| {
                    matches!(proposal.proposal(), openmls::prelude::Proposal::Add(_))
                }),
            ))
        })?;

        if has_pending {
            // A rejected commit can leave an accepted installation Add without
            // its membership update. Refresh before committing that proposal,
            // even when the regular installation-check interval has not elapsed.
            if has_pending_add {
                unsent_welcomes_do_not_stop_a_send(self.add_missing_installations().await)?;
            }
            tracing::debug!(
                inbox_id = self.context.inbox_id(),
                group_id = %self.group_id,
                "Found pending proposals, committing before sending message"
            );

            // Queue a CommitPendingProposals intent and wait for it to resolve
            let intent = intents::QueueIntent::commit_pending_proposals().queue(self)?;
            self.sync_until_intent_resolved(intent.id).await?;
        }

        Ok(())
    }

    /// Publish all unpublished messages. This happens by calling `sync_until_last_intent_resolved`
    /// which publishes all pending intents and reads them back from the network.
    #[cfg_attr(any(test, feature = "test-utils"), tracing::instrument(level = "info", fields(inbox_id = self.context.inbox_id()), skip(self)))]
    #[cfg_attr(not(any(test, feature = "test-utils")), xmtp_common::mls_span)]
    pub async fn publish_messages(&self) -> Result<(), GroupError> {
        self.ensure_not_paused().await?;
        let update_interval_ns = Some(SEND_MESSAGE_UPDATE_INSTALLATIONS_INTERVAL_NS);
        unsent_welcomes_do_not_stop_a_send(
            self.maybe_update_installations(update_interval_ns).await,
        )?;
        self.sync_until_last_intent_resolved().await?;

        // implicitly set group consent state to allowed
        self.update_consent_state(ConsentState::Allowed)?;

        Ok(())
    }

    /// Checks the network to see if any group members have identity updates that would cause installations
    /// to be added or removed from the group.
    ///
    /// If so, adds/removes those group members
    pub async fn update_installations(&self) -> Result<(), GroupError> {
        self.ensure_not_paused().await?;
        self.maybe_update_installations(Some(0)).await?;
        Ok(())
    }

    /// Send a message, optimistically returning the ID of the message before the result of a message publish.
    ///
    /// In a DM, await [`Self::ensure_dm_peer`] first. This method does not
    /// add a missing peer, so the peer could never read the message.
    pub fn send_message_optimistic(
        &self,
        message: &[u8],
        opts: send_message_opts::SendMessageOpts,
    ) -> Result<Vec<u8>, GroupError> {
        let message_id = self.prepare_message(message, opts)?;
        Ok(message_id)
    }

    /// Prepare a message for later publishing.
    ///
    /// Stores the message locally with `Unpublished` delivery status but does NOT
    /// create an intent to publish. Use `publish_stored_message` to publish later.
    ///
    /// # Arguments
    /// * `message` - The message content bytes
    /// * `should_push` - Whether to send a push notification when publishing
    /// * `idempotency_key` - Optional caller-supplied key the message id is
    ///   derived from. Defaults to a random key when `None`.
    ///
    /// Returns the message ID.
    ///
    /// In a DM, await [`Self::ensure_dm_peer`] first. This method does not
    /// add a missing peer, so the peer could never read the message.
    pub fn prepare_message_for_later_publish(
        &self,
        message: &[u8],
        should_push: bool,
        idempotency_key: Option<String>,
    ) -> Result<Vec<u8>, GroupError> {
        state_write(self.context.mls_storage(), |tx| {
            self.ensure_active_for_send(tx)?;
            let storage = tx.storage();
            let mut message = self.store_message_for_later_publish(
                &storage.db(),
                message,
                should_push,
                idempotency_key,
            )?;
            message.decrypted_message_bytes.zeroize();
            Ok::<_, GroupError>(Continue(message.id))
        })
        .map(TransactionOutcome::into_continued)
    }

    /// Store an optimistic message using the caller's transaction.
    fn store_message_for_later_publish(
        &self,
        db: &impl DbQuery,
        message: &[u8],
        should_push: bool,
        idempotency_key: Option<String>,
    ) -> Result<StoredGroupMessage, GroupError> {
        Self::reject_reserved_transcript_content(message)?;
        let now = self.context.message_timestamp_ns();
        // Resolve the key once. Random defaults do not depend on clock resolution.
        let idempotency_key = idempotency_key.unwrap_or_else(|| {
            hex::encode(xmtp_common::rand_vec::<DEFAULT_IDEMPOTENCY_KEY_BYTES>())
        });
        let queryable_content_fields = Self::extract_queryable_content_fields(message);

        let message_id = calculate_message_id(self.group_id, message, &idempotency_key);

        // Idempotent: a retry with the same key + content resolves to the same id.
        // Return the existing message rather than failing on the PK conflict, so
        // crash-recovery retries are at-least-once-with-dedup instead of an error.
        if let Some(existing) = db.get_group_message(&message_id)? {
            return Ok(existing);
        }

        let mut group_message = StoredGroupMessage {
            id: message_id.clone(),
            group_id: self.group_id,
            decrypted_message_bytes: message.to_vec(),
            sent_at_ns: now,
            kind: GroupMessageKind::Application,
            sender_installation_id: self.context.installation_id().into(),
            sender_inbox_id: self.context.inbox_id().to_string(),
            delivery_status: DeliveryStatus::Unpublished,
            content_type: queryable_content_fields.content_type,
            version_major: queryable_content_fields.version_major,
            version_minor: queryable_content_fields.version_minor,
            authority_id: queryable_content_fields.authority_id,
            reference_id: queryable_content_fields.reference_id,
            sequence_id: 0,
            envelope_hash: None,
            expiry_ns: None,
            expiry_unknown: false,
            expire_at_ns: None,
            inserted_at_ns: 0,
            should_push,
            idempotency_key,
        };
        // The content can hold a device-sync HMAC root key; a failed insert
        // must not drop it unwiped.
        if let Err(error) = group_message.store(db) {
            group_message.decrypted_message_bytes.zeroize();
            return Err(error.into());
        }
        Ok(group_message)
    }

    /// Publish a previously stored message by ID.
    ///
    /// Creates an intent for the message and publishes it to the network.
    /// Uses the `should_push` value that was stored with the message.
    /// This is a no-op if the message is already published.
    ///
    /// Returns an error if the message is not found.
    #[xmtp_common::mls_span]
    pub async fn publish_stored_message(&self, message_id: &[u8]) -> Result<(), GroupError> {
        if !self.is_active()? {
            return Err(GroupError::GroupInactive);
        }
        self.ensure_not_paused().await?;

        let queued = crate::state_tx::state_write_with_events(
            self.context.mls_storage(),
            self.context.events(),
            |tx, events| {
                self.ensure_active_for_send(tx)?;
                let storage = tx.storage();
                let db = storage.db();
                let mut message = db
                    .get_group_message(message_id)?
                    .filter(|message| message.group_id == self.group_id)
                    .ok_or_else(|| {
                        GroupError::NotFound(NotFound::MessageById(message_id.to_vec()))
                    })?;
                // The content can hold a device-sync HMAC root key, or a
                // commit-log signer an older build recorded, so it is wiped
                // on every exit.
                let outcome = (|| {
                    // An older build stored this row before the creation check.
                    // It fails here and is never queued; the transaction commits.
                    // A saved prepared attempt may already be on the backend, so
                    // its outcome stays unknown and its saved bytes may still publish.
                    // implements: GMOD-054
                    if message.delivery_status != DeliveryStatus::Published
                        && Self::is_reserved_transcript_content(&message.decrypted_message_bytes)
                        && !self.has_saved_attempt(&db, &message.id)?
                    {
                        if message.delivery_status == DeliveryStatus::Unpublished {
                            db.set_delivery_status_to_failed(&message.id)?;
                            self.emit_message_status_changed(
                                message.id.clone(),
                                xmtp_events::MessageStatus::Unpublished,
                                xmtp_events::MessageStatus::Failed,
                                events,
                            );
                        }
                        return Ok(Continue(Err(GroupError::ReservedTranscriptContentType)));
                    }
                    self.queue_stored_message(&db, events, &message)
                        .map(|queued| Continue(Ok(queued)))
                })();
                message.decrypted_message_bytes.zeroize();
                outcome
            },
        )?
        .into_continued()?;
        let Some(intent_id) = queued else {
            return Ok(());
        };

        // Publish
        unsent_welcomes_do_not_stop_a_send(
            self.maybe_update_installations(Some(SEND_MESSAGE_UPDATE_INSTALLATIONS_INTERVAL_NS))
                .await,
        )?;
        self.sync_until_intent_resolved(intent_id).await?;

        // Implicitly set group consent state to allowed
        self.update_consent_state(ConsentState::Allowed)?;

        Ok(())
    }

    /// Delete a message by its ID. Returns the ID of the deletion message.
    ///
    /// Only the original sender or a super admin can delete a message.
    ///
    /// In a DM, await [`Self::ensure_dm_peer`] first. This method does not
    /// add a missing peer, so the peer could never read the deletion.
    ///
    /// # Wire Protocol
    /// The `DeleteMessage` protobuf encodes `message_id` as a hex-encoded string for wire
    /// transmission, while the database stores message IDs as raw bytes. This function handles
    /// the conversion: it accepts raw bytes, hex-encodes them for the wire protocol, and when
    /// processing incoming deletions (in `process_delete_message`), the hex string is decoded
    /// back to bytes for database lookups.
    ///
    /// # Arguments
    /// * `message_id` - The message ID as bytes
    ///
    /// # Returns
    /// The ID of the deletion message
    pub fn delete_message(&self, message_id: Vec<u8>) -> Result<Vec<u8>, GroupError> {
        use error::DeleteMessageError;

        let conn = self.context.db();

        // Load the original message
        let original_msg = conn
            .get_group_message(&message_id)?
            .ok_or_else(|| DeleteMessageError::MessageNotFound(hex::encode(&message_id)))?;

        // Validate message belongs to this group (prevent cross-group deletion)
        if original_msg.group_id.as_slice() != self.group_id.as_slice() {
            return Err(DeleteMessageError::NotAuthorized.into());
        }

        // implements: PROC-056
        if find_valid_deletion(&conn, &original_msg, &self.group_id)?.is_some() {
            return Err(DeleteMessageError::MessageAlreadyDeleted.into());
        }

        let sender_inbox_id = self.context.inbox_id();
        let is_sender = original_msg.sender_inbox_id == sender_inbox_id;
        let is_super_admin = self.live_is_super_admin(sender_inbox_id)?;

        if !is_sender && !is_super_admin {
            return Err(DeleteMessageError::NotAuthorized.into());
        }

        if !crate::messages::enrichment::is_deletable_stored_message(&original_msg) {
            return Err(DeleteMessageError::NonDeletableMessage.into());
        }

        let delete_msg = DeleteMessage {
            message_id: hex::encode(&message_id),
        };

        let encoded_delete = DeleteMessageCodec::encode(delete_msg)?;
        let mut buf = Vec::new();
        encoded_delete.encode(&mut buf)?;

        let deletion_message_id = self.send_message_optimistic(&buf, SendMessageOpts::default())?;

        let is_super_admin_deletion = !is_sender && is_super_admin;

        let deletion = StoredMessageDeletion {
            id: deletion_message_id.clone(),
            group_id: self.group_id,
            deleted_message_id: message_id,
            deleted_by_inbox_id: sender_inbox_id.to_string(),
            is_super_admin_deletion,
            deleted_at_ns: now_ns(),
        };

        crate::state_tx::state_write_with_events(
            self.context.mls_storage(),
            self.context.events(),
            |tx, events| {
                let storage = tx.storage();
                let db = storage.db();
                let already_deleted =
                    find_valid_deletion(&db, &original_msg, &self.group_id)?.is_some();
                deletion.store(&db)?;
                if !already_deleted {
                    crate::subscriptions::internal::emit_deleted_messages(
                        events,
                        vec![original_msg],
                        xmtp_events::DeletionCause::DeletedLocally,
                        Some(deletion.deleted_by_inbox_id.clone()),
                        &db,
                    )?;
                }
                Ok::<_, GroupError>(Continue(()))
            },
        )?;

        Ok(deletion_message_id)
    }

    /// Whether a send intent for this message already has a saved attempt.
    fn has_saved_attempt(&self, db: &impl DbQuery, message_id: &[u8]) -> Result<bool, GroupError> {
        for intent in db.find_group_intents(
            self.group_id,
            Some(vec![IntentState::Published]),
            Some(vec![xmtp_db::group_intent::IntentKind::SendMessage]),
        )? {
            if crate::utils::id::calculate_message_id_for_intent(&intent)?.as_deref()
                == Some(message_id)
                && db.prepared_envelopes(intent.id)?.is_some()
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Whether the outer content type is a reserved transcript type. Only the
    /// `xmtp.org` authority reserves these type ids, at any version.
    // implements: GMOD-054
    pub(in crate::groups) fn is_reserved_transcript_content(message: &[u8]) -> bool {
        encoded_content_type(message)
            .ok()
            .flatten()
            .is_some_and(|content_type| {
                content_type.authority_id == "xmtp.org"
                    && matches!(
                        content_type.type_id.as_str(),
                        "group_updated" | "group_membership_change"
                    )
            })
    }

    /// Refuse a message whose outer content type is a reserved transcript
    /// type. Every send runs it; a caller that does network work before it
    /// queues a message, such as [`Self::ensure_dm_peer`], runs it first.
    pub fn reject_reserved_transcript_content(message: &[u8]) -> Result<(), GroupError> {
        if Self::is_reserved_transcript_content(message) {
            Err(GroupError::ReservedTranscriptContentType)
        } else {
            Ok(())
        }
    }

    /// Helper function to extract queryable content fields from a message
    pub(in crate::groups) fn extract_queryable_content_fields(
        message: &[u8],
    ) -> QueryableContentFields {
        // Return early with default if decoding fails or type is missing
        let Ok(content_type_id) = encoded_content_type(message).inspect_err(|_| {
            tracing::debug!("No queryable content fields, msg not formatted as encoded content")
        }) else {
            return QueryableContentFields::default();
        };
        let content_type_id = content_type_id.unwrap_or_default();
        // Only a type with a reference id needs the payload. Other content,
        // such as a device-sync HMAC update, is never copied.
        if !QueryableContentFields::has_reference(&content_type_id) {
            return QueryableContentFields::from_type(&content_type_id);
        }
        EncodedContent::decode(message)
            .and_then(|content| {
                QueryableContentFields::try_from(content).inspect_err(|e| {
                    tracing::debug!(
                        "Failed to convert EncodedContent to QueryableContentFields: {}",
                        e
                    )
                })
            })
            .unwrap_or_default()
    }

    /// Prepare a [`IntentKind::SendMessage`] intent, and [`StoredGroupMessage`] on this users XMTP [`Client`].
    ///
    /// # Arguments
    /// * message: UTF-8 or encoded message bytes
    /// * opts: Options for sending the message
    // implements: SEND-021
    #[tracing::instrument(skip_all, level = "trace")]
    pub(crate) fn prepare_message(
        &self,
        message: &[u8],
        opts: send_message_opts::SendMessageOpts,
    ) -> Result<Vec<u8>, GroupError> {
        crate::state_tx::state_write_with_events(
            self.context.mls_storage(),
            self.context.events(),
            |tx, events| {
                self.ensure_active_for_send(tx)?;
                let storage = tx.storage();
                let db = storage.db();
                let mut message = self.store_message_for_later_publish(
                    &db,
                    message,
                    opts.should_push,
                    opts.idempotency_key,
                )?;
                // The content can hold a device-sync HMAC root key.
                let queued = self.queue_stored_message(&db, events, &message);
                message.decrypted_message_bytes.zeroize();
                queued?;
                Ok::<_, GroupError>(Continue(message.id))
            },
        )
        .map(TransactionOutcome::into_continued)
    }

    /// Queue the one publish intent of a stored message under the caller's writer.
    ///
    /// A published message is never queued again, and an unresolved intent for
    /// the same envelope is reused. A retry of a failed message returns it to
    /// `Unpublished` in the same transaction as its new intent. Returns that
    /// exact intent's ID, or None when the message is already published.
    // implements: SEND-003
    fn queue_stored_message(
        &self,
        db: &impl DbQuery,
        events: &impl xmtp_events::EventWriter<crate::subscriptions::internal::InternalEvent>,
        message: &StoredGroupMessage,
    ) -> Result<Option<i32>, GroupError> {
        if message.delivery_status == DeliveryStatus::Published {
            return Ok(None);
        }
        let envelope =
            Self::encode_envelope(&message.decrypted_message_bytes, &message.idempotency_key);
        let intent = QueueIntent::send_message()
            .data(Vec::<u8>::from(SendMessageIntentData::new(envelope)))
            .should_push(message.should_push)
            .queue_in(db, self)?;
        if db.set_failed_delivery_status_to_unpublished(&message.id)? > 0 {
            self.emit_message_status_changed(
                message.id.clone(),
                xmtp_events::MessageStatus::Failed,
                xmtp_events::MessageStatus::Unpublished,
                events,
            );
        }
        Ok(Some(intent.id))
    }

    /// Check membership while holding the same writer as the new message or intent.
    // implements: SEND-005
    fn ensure_active_for_send<T: xmtp_db::TransactionalKeyStore>(
        &self,
        tx: &mut crate::state_tx::StateTx<'_, T>,
    ) -> Result<(), GroupError> {
        let stored = tx
            .storage()
            .db()
            .find_group(&self.group_id)?
            .ok_or(NotFound::GroupById(self.group_id))?;
        if stored.membership_state == GroupMembershipState::Restored {
            return Err(GroupError::GroupInactive);
        }
        let active = tx.with_group(self.group_id, |mls_group, _| {
            Ok::<_, GroupError>(mls_group.is_active())
        })?;
        if active {
            Ok(())
        } else {
            Err(GroupError::GroupInactive)
        }
    }

    /// Encode the plaintext envelope of a message. The content can hold a
    /// device-sync HMAC root key, so the proto temporary is wiped after
    /// encoding.
    fn encode_envelope(encoded_msg: &[u8], idempotency_key: &str) -> Vec<u8> {
        let mut envelope = PlaintextEnvelope {
            content: Some(Content::V1(V1 {
                content: encoded_msg.to_vec(),
                idempotency_key: idempotency_key.to_string(),
            })),
        };
        let bytes = envelope.encode_to_vec();
        if let Some(Content::V1(v1)) = &mut envelope.content {
            v1.content.zeroize();
        }
        bytes
    }

    /// Query the database for stored messages. Optionally filtered by time, kind, delivery_status
    /// and limit
    pub fn find_messages(
        &self,
        args: &MsgQueryArgs,
    ) -> Result<Vec<StoredGroupMessage>, GroupError> {
        let conn = self.context.db();
        let messages = conn.get_group_messages(&self.group_id, args)?;
        Ok(messages)
    }

    /// Count the number of stored messages matching the given criteria
    pub fn count_messages(&self, args: &MsgQueryArgs) -> Result<i64, GroupError> {
        let conn = self.context.db();
        let count = conn.count_group_messages(&self.group_id, args)?;
        Ok(count)
    }

    pub fn get_last_read_times(&self) -> Result<LatestMessageTimeBySender, GroupError> {
        let conn = self.context.db();
        let latest_read_receipt =
            conn.get_latest_message_times_by_sender(self.group_id, &[ContentType::ReadReceipt])?;
        Ok(latest_read_receipt)
    }

    /// Load the group reference stored in the local database
    pub fn load(&self) -> Result<StoredGroup, StorageError> {
        let conn = self.context.db();
        if let Some(group) = conn.find_group(&self.group_id)? {
            Ok(group)
        } else {
            tracing::error!("group {} does not exist", hex::encode(self.group_id));
            Err(NotFound::GroupById(self.group_id).into())
        }
    }
}
