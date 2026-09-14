//! Sending, preparing, and querying messages.

use super::*;

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

        self.ensure_not_paused().await?;
        let update_interval_ns = Some(SEND_MESSAGE_UPDATE_INSTALLATIONS_INTERVAL_NS);
        self.maybe_update_installations(update_interval_ns).await?;

        // Check for pending proposals and commit them first
        // OpenMLS blocks message creation when there are pending proposals
        self.commit_pending_proposals_if_any().await?;

        let message_id =
            self.prepare_message(message, opts, |key| Self::into_envelope(message, key))?;

        self.sync_until_last_intent_resolved().await?;

        // implicitly set group consent state to allowed
        self.update_consent_state(ConsentState::Allowed)?;

        Ok(message_id)
    }

    /// Checks for pending MLS proposals and commits them if any exist.
    /// OpenMLS blocks message creation when there are pending proposals,
    /// so we need to commit them first.
    async fn commit_pending_proposals_if_any(&self) -> Result<(), GroupError> {
        let has_pending = self.with_group_snapshot(|openmls_group| {
            Ok::<bool, GroupError>(openmls_group.pending_proposals().next().is_some())
        })?;

        if has_pending {
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
        self.maybe_update_installations(update_interval_ns).await?;
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
    pub fn send_message_optimistic(
        &self,
        message: &[u8],
        opts: send_message_opts::SendMessageOpts,
    ) -> Result<Vec<u8>, GroupError> {
        let message_id =
            self.prepare_message(message, opts, |key| Self::into_envelope(message, key))?;
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
    pub fn prepare_message_for_later_publish(
        &self,
        message: &[u8],
        should_push: bool,
        idempotency_key: Option<String>,
    ) -> Result<Vec<u8>, GroupError> {
        state_write(self.context.mls_storage(), |tx| {
            let storage = tx.storage();
            let message = self.store_message_for_later_publish(
                &storage.db(),
                message,
                should_push,
                idempotency_key,
            )?;
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
        let now = now_ns();
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

        let group_message = StoredGroupMessage {
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
            expire_at_ns: None,
            inserted_at_ns: 0,
            should_push,
            idempotency_key,
        };
        group_message.store(db)?;
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

        let queued = state_write(self.context.mls_storage(), |tx| {
            let storage = tx.storage();
            let db = storage.db();
            let message = db
                .get_group_message(message_id)?
                .filter(|message| message.group_id == self.group_id)
                .ok_or_else(|| GroupError::NotFound(NotFound::MessageById(message_id.to_vec())))?;
            if message.delivery_status == DeliveryStatus::Published {
                return Ok(Continue(false));
            }
            let envelope =
                Self::into_envelope(&message.decrypted_message_bytes, &message.idempotency_key);
            let intent_data: Vec<u8> = SendMessageIntentData::new(envelope.encode_to_vec()).into();
            QueueIntent::send_message()
                .data(intent_data)
                .should_push(message.should_push)
                .queue_in(&db, self)?;
            Ok::<_, GroupError>(Continue(true))
        })?
        .into_continued();
        if !queued {
            return Ok(());
        }

        // Publish
        self.maybe_update_installations(Some(SEND_MESSAGE_UPDATE_INSTALLATIONS_INTERVAL_NS))
            .await?;
        self.sync_until_last_intent_resolved().await?;

        // Implicitly set group consent state to allowed
        self.update_consent_state(ConsentState::Allowed)?;

        Ok(())
    }

    /// Delete a message by its ID. Returns the ID of the deletion message.
    ///
    /// Only the original sender or a super admin can delete a message.
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

        // Check if message is already deleted
        if conn.is_message_deleted(&message_id)? {
            return Err(DeleteMessageError::MessageAlreadyDeleted.into());
        }

        let sender_inbox_id = self.context.inbox_id();
        let is_sender = original_msg.sender_inbox_id == sender_inbox_id;
        let is_super_admin = self.is_super_admin(sender_inbox_id.to_string())?;

        if !is_sender && !is_super_admin {
            return Err(DeleteMessageError::NotAuthorized.into());
        }

        if !original_msg.kind.is_deletable() || !original_msg.content_type.is_deletable() {
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

        deletion.store(&conn)?;

        Ok(deletion_message_id)
    }

    /// Helper function to extract queryable content fields from a message
    pub(in crate::groups) fn extract_queryable_content_fields(
        message: &[u8],
    ) -> QueryableContentFields {
        // Return early with default if decoding fails or type is missing
        EncodedContent::decode(message)
            .inspect_err(|_| {
                tracing::debug!("No queryable content fields, msg not formatted as encoded content")
            })
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
    /// * envelope: closure that returns context-specific [`PlaintextEnvelope`]. Closure accepts
    ///   timestamp attached to intent & stored message.
    #[tracing::instrument(skip_all, level = "trace")]
    pub(crate) fn prepare_message<F>(
        &self,
        message: &[u8],
        opts: send_message_opts::SendMessageOpts,
        envelope: F,
    ) -> Result<Vec<u8>, GroupError>
    where
        F: FnOnce(&str) -> PlaintextEnvelope,
    {
        state_write(self.context.mls_storage(), |tx| {
            let storage = tx.storage();
            let db = storage.db();
            let stored_message = self.store_message_for_later_publish(
                &db,
                message,
                opts.should_push,
                opts.idempotency_key,
            )?;
            if stored_message.delivery_status == DeliveryStatus::Published {
                return Ok(Continue(stored_message.id));
            }
            // Create envelope using the stored idempotency key so the id stays consistent
            let plain_envelope = envelope(&stored_message.idempotency_key);
            let mut encoded_envelope = vec![];
            plain_envelope.encode(&mut encoded_envelope)?;

            // Queue the intent (use should_push from stored message)
            let intent_data: Vec<u8> = SendMessageIntentData::new(encoded_envelope).into();
            QueueIntent::send_message()
                .data(intent_data)
                .should_push(stored_message.should_push)
                .queue_in(&db, self)?;

            Ok::<_, GroupError>(Continue(stored_message.id))
        })
        .map(TransactionOutcome::into_continued)
    }

    fn into_envelope(encoded_msg: &[u8], idempotency_key: &str) -> PlaintextEnvelope {
        PlaintextEnvelope {
            content: Some(Content::V1(V1 {
                content: encoded_msg.to_vec(),
                idempotency_key: idempotency_key.to_string(),
            })),
        }
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

    /// Query the database for stored messages. Optionally filtered by time, kind, delivery_status
    /// and limit
    pub fn find_messages_with_reactions(
        &self,
        args: &MsgQueryArgs,
    ) -> Result<Vec<StoredGroupMessageWithReactions>, GroupError> {
        let conn = self.context.db();
        let messages = conn.get_group_messages_with_reactions(&self.group_id, args)?;
        Ok(messages)
    }

    /// Query for enriched messages (with reactions, replies, and deletion status)
    #[xmtp_common::mls_span]
    pub fn find_enriched_messages(
        &self,
        args: &MsgQueryArgs,
    ) -> Result<Vec<crate::messages::decoded_message::DecodedMessage>, EnrichMessageError> {
        let conn = self.context.db();
        let messages = conn.get_group_messages(&self.group_id, args)?;
        let enriched =
            crate::messages::enrichment::enrich_messages(conn, &self.group_id, messages)?;
        Ok(enriched)
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
