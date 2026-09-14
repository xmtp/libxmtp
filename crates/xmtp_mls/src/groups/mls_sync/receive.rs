//! Receiving envelopes and storing transcript messages.

use super::*;

impl<Context> MlsGroup<Context>
where
    Context: XmtpSharedContext,
{
    /// Wait for a fixed network prefix. The summary is local history, not proof of completion.
    #[tracing::instrument(skip_all, level = "trace")]
    pub async fn receive(&self) -> Result<ProcessSummary, GroupError> {
        use xmtp_db::delivery::{DeliveryScope, QueryDelivery};
        let db = self.context.db();
        let mut position = db.current_delivery_cursor()?;
        crate::subscriptions::barrier::receive_through_current(
            &self.context,
            vec![xmtp_proto::types::Topic::new_group_message(self.group_id)],
        )
        .await?;
        let upper = db.current_delivery_cursor()?;
        let settings = self.context.incoming_runtime().policy();
        let mut summary = ProcessSummary::default();
        loop {
            let rows = db.replay_delivery_messages_bounded(
                position,
                &DeliveryScope::Groups(vec![self.group_id]),
                xmtp_common::time::now_ns(),
                settings.max_local_read_rows,
                settings.max_local_read_bytes,
            )?;
            if rows.is_empty() {
                break;
            }
            let mut reached_upper = false;
            for row in rows {
                if row.cursor.delivery_sequence > upper.delivery_sequence {
                    reached_upper = true;
                    break;
                }
                position = row.cursor;
                let message = row.message;
                summary.add_id(message.cursor());
                summary.add(MessageIdentifier {
                    cursor: message.cursor(),
                    group_id: message.group_id,
                    created_ns: chrono::DateTime::from_timestamp_nanos(message.sent_at_ns),
                    previously_processed: false,
                    internal_id: Some(message.id),
                    group_context: None,
                    intent_kind: None,
                });
            }
            if reached_upper || position.delivery_sequence >= upper.delivery_sequence {
                break;
            }
        }
        Ok(summary)
    }

    #[tracing::instrument(skip_all, level = "trace")]
    pub(super) fn maybe_update_cursor(
        &self,
        db: &impl DbQuery,
        message: &xmtp_proto::types::GroupMessage,
    ) -> Result<bool, StorageError> {
        let updated = db.update_cursor(
            message.group_id,
            xmtp_db::refresh_state::EntityKind::ApplicationMessage,
            message.cursor,
        )?;
        if updated {
            log_event!(
                Event::GroupCursorUpdate,
                self.context.installation_id(),
                group_id = message.group_id.as_slice(),
                cursor = message.cursor.0,
            );
        } else {
            tracing::debug!("no cursor update required");
        }
        Ok(updated)
    }

    pub(super) fn save_transcript_message(
        &self,
        validated_commit: ValidatedCommit,
        timestamp_ns: u64,
        cursor: Cursor,
        storage: &impl XmtpMlsStorageProvider,
    ) -> Result<Option<(StoredGroupMessage, GroupUpdated)>, GroupMessageProcessingError> {
        if validated_commit.is_empty() {
            return Ok(None);
        }
        let sender_installation_id = validated_commit.actor_installation_id();
        let sender_inbox_id = validated_commit.actor_inbox_id();

        let pending_remove_users = &storage.db().get_pending_remove_users(&self.group_id)?;
        let payload: GroupUpdated = validated_commit.into_with(pending_remove_users);
        tracing::info!("Storing transcript message");
        let encoded_payload = GroupUpdatedCodec::encode(payload.clone())?;
        let mut encoded_payload_bytes = Vec::new();
        encoded_payload.encode(&mut encoded_payload_bytes)?;

        let message_id = calculate_message_id(
            self.group_id,
            encoded_payload_bytes.as_slice(),
            &timestamp_ns.to_string(),
        );
        let content_type = encoded_payload.r#type.unwrap_or_else(|| {
            tracing::warn!("Missing content type in encoded payload, using default values");
            // Default content type values
            xmtp_proto::xmtp::mls::message_contents::ContentTypeId {
                authority_id: "unknown".to_string(),
                type_id: "unknown".to_string(),
                version_major: 0,
                version_minor: 0,
            }
        });

        self.handle_metadata_update_from_commit(&payload.metadata_field_changes, storage)?;

        // When a DM is stitched, it can repeat group updates. We want to prevent saving those messages.
        if self.update_already_exists(&payload, storage)? {
            return Ok(None);
        }

        let msg = StoredGroupMessage {
            id: message_id,
            group_id: self.group_id,
            decrypted_message_bytes: encoded_payload_bytes,
            sent_at_ns: timestamp_ns as i64,
            kind: GroupMessageKind::MembershipChange,
            sender_installation_id,
            sender_inbox_id,
            delivery_status: DeliveryStatus::Published,
            content_type: content_type.type_id.into(),
            version_major: content_type.version_major as i32,
            version_minor: content_type.version_minor as i32,
            authority_id: content_type.authority_id.to_string(),
            reference_id: None,
            sequence_id: cursor.0 as i64,
            envelope_hash: None,
            expiry_ns: None,
            expire_at_ns: None,
            inserted_at_ns: 0, // Will be set by database
            should_push: true,
            // Matches the key used to derive `message_id` above.
            idempotency_key: timestamp_ns.to_string(),
        };

        msg.store_or_ignore(&storage.db())?;
        Ok(Some((msg, payload)))
    }

    fn update_already_exists(
        &self,
        payload: &GroupUpdated,
        storage: &impl XmtpMlsStorageProvider,
    ) -> Result<bool, GroupMessageProcessingError> {
        if self.dm_id.is_none() || payload.added_inboxes.is_empty() {
            // Only dedupe for DMs.
            // Only dedupe for group adds.
            return Ok(false);
        }

        let mut deduper = GroupUpdateDeduper::default();
        let mut inserted_after_ns = None;
        let mut msgs;
        loop {
            // DMs are stitched, so we don't want to have the same
            // group updates from multiple DMs being saved to the database.
            msgs = self.find_messages_v2_with_conn(
                &MsgQueryArgs {
                    content_types: Some(vec![ContentType::GroupUpdated]),
                    inserted_after_ns,
                    limit: Some(100),
                    ..Default::default()
                },
                storage.db(),
            )?;

            let Some(msg) = msgs.last() else {
                break;
            };
            inserted_after_ns = Some(msg.metadata.inserted_at_ns);

            for msg in msgs {
                let MessageBody::GroupUpdated(update) = msg.content else {
                    continue;
                };

                deduper.consume(&update);
            }
        }

        Ok(deduper.is_dupe(payload))
    }
}
