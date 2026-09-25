//! Receiving envelopes and storing transcript messages.

use super::*;
use crate::messages::decoded_message::DecodedMessage;
use xmtp_db::group_message::SortBy;
use xmtp_db::prelude::QueryGroupMessage;

/// Rows read per page when scanning a DM's group updates for duplicates.
const PAGE_SIZE: i64 = 100;

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

    // implements: GMOD-034
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
            content_type: ContentType::from_identifier(
                &content_type.authority_id,
                &content_type.type_id,
                content_type.version_major,
            ),
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
        // Grows while a page holds a single `inserted_at_ns`, so that a tie
        // group larger than one page still fits and the cursor can advance.
        let mut limit = PAGE_SIZE;
        loop {
            // DMs are stitched, so we don't want to have the same
            // group updates from multiple DMs being saved to the database.
            //
            // Sort by the same column the cursor advances on. The default sort
            // is `SentAt`, which does not agree with an `inserted_at_ns`
            // cursor: the last row by sent time need not hold the largest
            // `inserted_at_ns`, so the cursor could stall and repeat a page
            // forever.
            //
            // Read stored rows, not decoded messages. The page count must be
            // the number of rows the database returned: a decoded view drops
            // rows it cannot decode, which makes a full page look short and
            // lets the rows past its edge be skipped. Decoding one row at a
            // time also skips the reaction and reply lookups that a decoded
            // list runs for every page, which group updates never need.
            let rows = storage.db().get_group_messages(
                &self.group_id,
                &MsgQueryArgs {
                    content_types: Some(vec![ContentType::GroupUpdated]),
                    inserted_after_ns,
                    limit: Some(limit),
                    sort_by: Some(SortBy::InsertedAt),
                    ..Default::default()
                },
            )?;

            let full_page = rows.len() >= limit as usize;
            let last_value = rows.last().map(|row| row.inserted_at_ns);

            // The cursor filter is a strict `>` on `inserted_at_ns`, which the
            // database records at millisecond granularity, so rows written in
            // the same millisecond share a value. On a full page the final
            // value may continue past the page edge, so hold those rows back
            // and let the next page re-read them in full. A short page reached
            // the end of the scan and keeps every row.
            let cutoff = last_value.filter(|_| full_page);
            let consumed = rows
                .into_iter()
                .filter(|row| Some(row.inserted_at_ns) != cutoff);

            let mut highest_consumed = None;
            for row in consumed {
                highest_consumed = Some(row.inserted_at_ns);
                let Ok(msg) = DecodedMessage::try_from(row)
                    .inspect_err(|err| tracing::warn!("Failed to decode group update {err:?}"))
                else {
                    continue;
                };
                let MessageBody::GroupUpdated(update) = msg.content else {
                    continue;
                };

                deduper.consume(&update);
            }

            match highest_consumed {
                // Resume just above the last value read in full.
                Some(value) => {
                    inserted_after_ns = Some(value);
                    limit = PAGE_SIZE;
                }
                // A full page of one repeated value: nothing could be retired,
                // so re-read it larger rather than stall.
                None if full_page => limit = limit.saturating_mul(2),
                None => break,
            }
        }

        Ok(deduper.is_dupe(payload))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tester;
    use xmtp_common::{rand_vec, time::now_ns};
    use xmtp_content_types::{ContentCodec, encryption::sha256, group_updated::GroupUpdatedCodec};
    use xmtp_db::diesel::{ExpressionMethods, QueryDsl, RunQueryDsl};
    use xmtp_db::group_message::{DeliveryStatus, GroupMessageKind};
    use xmtp_db::user_preferences::StoredUserPreferences;
    use xmtp_db::{ConnectionExt, DbConnection, Store};
    use xmtp_proto::types::GroupId;
    use xmtp_proto::xmtp::mls::message_contents::group_updated::Inbox;

    /// Marks the rows a test seeds, so they can be pinned to one `inserted_at_ns`.
    const SEEDED_SENDER: &str = "seeded";

    fn add_update(inbox_id: &str) -> GroupUpdated {
        GroupUpdated {
            added_inboxes: vec![Inbox {
                inbox_id: inbox_id.to_string(),
            }],
            ..Default::default()
        }
    }

    fn encode(update: &GroupUpdated) -> Vec<u8> {
        let mut bytes = Vec::new();
        GroupUpdatedCodec::encode(update.clone())
            .unwrap()
            .encode(&mut bytes)
            .unwrap();
        bytes
    }

    /// Store `rows` as group updates of `group_id`, in order, then pin them to
    /// one `inserted_at_ns` one millisecond above every row the group already
    /// holds.
    fn seed_tied_updates<C: ConnectionExt>(
        db: &DbConnection<C>,
        group_id: GroupId,
        rows: Vec<Vec<u8>>,
    ) {
        for (i, bytes) in rows.into_iter().enumerate() {
            StoredGroupMessage {
                id: sha256(&rand_vec::<12>()),
                group_id,
                decrypted_message_bytes: bytes,
                sent_at_ns: now_ns(),
                kind: GroupMessageKind::MembershipChange,
                sender_installation_id: vec![1, 2, 3],
                sender_inbox_id: SEEDED_SENDER.to_string(),
                delivery_status: DeliveryStatus::Published,
                content_type: ContentType::GroupUpdated,
                version_major: 0,
                version_minor: 0,
                authority_id: "unknown".to_string(),
                reference_id: None,
                sequence_id: i as i64 + 1,
                envelope_hash: None,
                expiry_ns: None,
                expire_at_ns: None,
                inserted_at_ns: 0,
                should_push: true,
                idempotency_key: String::new(),
            }
            .store(db)
            .unwrap();
        }

        // The database stamps `inserted_at_ns` at insert time, and its clock
        // does not agree with `now_ns`. Pin the seeded rows to one value just
        // above every row the group already holds, rather than rely on how
        // fast the loop ran or on the wall clock.
        db.raw_query(|conn| {
            use xmtp_db::schema::group_messages::dsl;
            let newest: Option<i64> = dsl::group_messages
                .filter(dsl::group_id.eq(group_id))
                .select(xmtp_db::diesel::dsl::max(dsl::inserted_at_ns))
                .first(conn)?;
            let shared_inserted_at_ns = newest.unwrap_or_default() + 1_000_000;
            xmtp_db::diesel::update(dsl::group_messages)
                .filter(dsl::group_id.eq(group_id))
                .filter(dsl::sender_inbox_id.eq(SEEDED_SENDER))
                .set(dsl::inserted_at_ns.eq(shared_inserted_at_ns))
                .execute(conn)
        })
        .unwrap();
    }

    /// The startup migration deletes repeated group updates in DMs. Let it
    /// finish before seeding repeated rows, so it cannot thin them out.
    async fn wait_for_startup_cleanup<C: ConnectionExt>(db: DbConnection<C>) {
        xmtp_common::wait_for_eq(
            || async {
                StoredUserPreferences::load(&db)
                    .unwrap()
                    .dm_group_updates_migrated
            },
            true,
        )
        .await
        .unwrap();
    }

    /// Rows written in one millisecond share `inserted_at_ns`, and the cursor
    /// is a strict `>` on that column. A tie group larger than one page must
    /// still be read in full, or the newest update is never consumed and the
    /// duplicate check answers from stale rows.
    #[xmtp_common::test(unwrap_try = true)]
    async fn test_update_already_exists_spans_rows_sharing_one_inserted_at() {
        tester!(alix);
        tester!(bo);
        let (dm, _) = alix.test_talk_in_dm_with(&bo).await?;
        wait_for_startup_cleanup(alix.db()).await;

        let earlier = add_update("earlier");
        let latest = add_update("latest");
        let mut rows = vec![encode(&earlier); PAGE_SIZE as usize];
        rows.extend(vec![encode(&latest); 25]);
        seed_tied_updates(&alix.db(), dm.group_id, rows);

        let storage = alix.context.mls_storage();
        assert!(
            dm.update_already_exists(&latest, storage)?,
            "the newest add sits past the page edge and must still be seen"
        );
        assert!(
            !dm.update_already_exists(&earlier, storage)?,
            "an add that is no longer the newest is not a duplicate"
        );
    }

    /// The page count must come from the rows the database returned, not from
    /// the rows that decoded. A row that fails to decode would otherwise make a
    /// full page look short, so its trailing tie group is consumed as if the
    /// scan had ended, and the rows past the page edge are skipped.
    #[xmtp_common::test(unwrap_try = true)]
    async fn test_update_already_exists_counts_undecodable_rows_toward_the_page() {
        tester!(alix);
        tester!(bo);
        let (dm, _) = alix.test_talk_in_dm_with(&bo).await?;
        wait_for_startup_cleanup(alix.db()).await;

        let earlier = add_update("earlier");
        let latest = add_update("latest");
        // Invalid protobuf, inside the first page.
        let mut rows = vec![vec![0xFF, 0xFE, 0xFD]];
        rows.extend(vec![encode(&earlier); PAGE_SIZE as usize - 1]);
        rows.extend(vec![encode(&latest); 25]);
        seed_tied_updates(&alix.db(), dm.group_id, rows);

        let storage = alix.context.mls_storage();
        assert!(
            dm.update_already_exists(&latest, storage)?,
            "an undecodable row must not shorten the page and hide the rows past its edge"
        );
    }
}
