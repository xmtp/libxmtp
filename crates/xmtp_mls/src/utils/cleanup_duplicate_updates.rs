use std::collections::HashSet;
use std::hash::{DefaultHasher, Hash, Hasher};

use tracing::info;
use xmtp_db::diesel::prelude::*;
use xmtp_db::{ConnectionExt, DbConnection};
use xmtp_db::{
    group::{ConversationType, GroupQueryArgs, QueryGroup},
    group_message::{ContentType, MsgQueryArgs},
    prelude::{QueryGroupMessage, QueryUserPreferences},
};

use crate::groups::mls_sync::GroupMessageProcessingError;
use crate::messages::decoded_message::MessageBody;
use crate::messages::enrichment::enrich_messages;

const BATCH_SIZE: i64 = 100;

pub async fn perform<C>(db: DbConnection<C>)
where
    C: ConnectionExt,
{
    if let Err(err) = perform_inner(db).await {
        tracing::error!("Duplicate cleanup task failed: {err:?}");
    }
}

async fn perform_inner<C>(db: DbConnection<C>) -> Result<(), GroupMessageProcessingError>
where
    C: ConnectionExt,
{
    let prefs = db.load_user_preferences().await?;
    if prefs.dm_group_updates_migrated {
        info!("DM group updates migration has already been performed. Skipping.");
        return Ok(());
    }

    let mut group_offset = 0;
    let mut groups;
    loop {
        groups = db
            .find_groups_by_id_paged(
                &GroupQueryArgs {
                    conversation_type: Some(ConversationType::Dm),
                    limit: Some(BATCH_SIZE),
                    ..Default::default()
                },
                group_offset,
            )
            .await?;

        if groups.is_empty() {
            break;
        }

        for group in groups {
            let mut sent_after_ns = None;
            let mut msgs;
            let mut originals: HashSet<u64> = HashSet::default();

            loop {
                msgs = db
                    .get_group_messages(
                        &group.id,
                        &MsgQueryArgs {
                            content_types: Some(vec![ContentType::GroupUpdated]),
                            sent_after_ns,
                            limit: Some(BATCH_SIZE),
                            ..Default::default()
                        },
                    )
                    .await?;

                {
                    let Some(msg) = msgs.last() else {
                        break;
                    };
                    sent_after_ns = Some(msg.sent_at_ns);
                }

                let msgs = enrich_messages(&db, &group.id, msgs).await?;

                for msg in msgs {
                    let MessageBody::GroupUpdated(update) = msg.content else {
                        continue;
                    };

                    let mut hasher = DefaultHasher::new();
                    update.hash(&mut hasher);
                    if originals.insert(hasher.finish()) {
                        continue;
                    }

                    db.raw_query(|conn| {
                        xmtp_db::diesel::delete(xmtp_db::schema::group_messages::table)
                            .filter(xmtp_db::schema::group_messages::id.eq(&msg.metadata.id))
                            .execute(conn)
                    })?;

                    tokio::task::yield_now().await;
                }
            }

            tokio::task::yield_now().await;
        }

        group_offset += BATCH_SIZE;
    }

    db.set_dm_group_updates_migrated().await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::tester;
    use prost::Message;

    use super::*;
    use xmtp_common::{rand_vec, time::now_ns};
    use xmtp_content_types::{ContentCodec, encryption::sha256, group_updated::GroupUpdatedCodec};
    use xmtp_db::{
        Store,
        group_message::{DeliveryStatus, GroupMessageKind, StoredGroupMessage},
    };
    use xmtp_proto::xmtp::mls::message_contents::{
        GroupUpdated,
        group_updated::{Inbox, MetadataFieldChange},
    };

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_cleanup_works_as_expected() {
        tester!(alix);
        tester!(bo);
        let mut sequence_id = 0;

        let mut gen_update_msg = |group_id, payload| {
            let mut encoded_payload = Vec::new();
            GroupUpdatedCodec::encode(payload)?.encode(&mut encoded_payload)?;
            sequence_id += 1;

            StoredGroupMessage {
                id: sha256(&rand_vec::<12>()),
                group_id,
                decrypted_message_bytes: encoded_payload,
                sent_at_ns: now_ns(),
                kind: GroupMessageKind::MembershipChange,
                sender_installation_id: vec![1, 2, 3],
                sender_inbox_id: "123".to_string(),
                delivery_status: DeliveryStatus::Published,
                content_type: ContentType::GroupUpdated,
                version_major: 0,
                version_minor: 0,
                authority_id: "unknown".to_string(),
                reference_id: None,
                sequence_id,
                originator_id: 0,
                expire_at_ns: None,
                inserted_at_ns: 0,
                should_push: true,
                idempotency_key: String::new(),
            }
        };

        let (dm, _) = alix.test_talk_in_dm_with(&bo).await?;
        dm.sync().await?;
        let old_updates = dm
            .find_messages_v2(&MsgQueryArgs {
                content_types: Some(vec![ContentType::GroupUpdated]),
                ..Default::default()
            })
            .await?;

        // Insert some duplicate group_updated messages
        let payload1 = GroupUpdated {
            added_inboxes: vec![Inbox {
                inbox_id: "123".to_string(),
            }],
            ..Default::default()
        };
        let payload2 = GroupUpdated {
            metadata_field_changes: vec![MetadataFieldChange {
                field_name: "expiration_setting".to_string(),
                old_value: None,
                new_value: Some("123".to_string()),
            }],

            ..Default::default()
        };

        let mut duplicates = vec![];

        for i in 0..3 {
            let msg1 = gen_update_msg(dm.group_id, payload1.clone());
            msg1.store(&alix.db())?;
            let msg2 = gen_update_msg(dm.group_id, payload2.clone());
            msg2.store(&alix.db())?;

            if i > 0 {
                duplicates.push(msg1.id);
                duplicates.push(msg2.id);
            }
        }

        // Client startup already ran this migration and recorded it, so clear the
        // flag to exercise the cleanup itself. (Before the flag was written with
        // an upsert it never stuck on a fresh database, and this test passed only
        // because the "one-time" migration silently re-ran on every start.)
        clear_migrated_flag(&alix.db())?;
        perform(alix.db()).await;

        let msgs = dm
            .find_messages_v2(&MsgQueryArgs {
                content_types: Some(vec![ContentType::GroupUpdated]),
                ..Default::default()
            })
            .await?;

        for msg in &msgs {
            assert!(
                !duplicates.contains(&msg.metadata.id),
                "A duplicate has remained in the db {:?}",
                msg.metadata.id
            );
        }
        // Make sure the old update didn't get deleted. The +2 is for the 2 dummy updates.
        assert_eq!(msgs.len(), old_updates.len() + 2);

        // Let's insert another duplicate and make sure it stays this time.
        // We don't want the perform to run more than once.
        let msg = gen_update_msg(dm.group_id, payload1.clone());
        msg.store(&alix.db())?;
        perform(alix.db()).await;

        // The duplicate should remain because perform will only clean up once.
        let msgs = dm
            .find_messages_v2(&MsgQueryArgs {
                content_types: Some(vec![ContentType::GroupUpdated]),
                ..Default::default()
            })
            .await?;
        assert!(msgs.iter().any(|m| m.metadata.id == msg.id));
    }

    /// Clears the one-time flag so a test can run the migration itself.
    fn clear_migrated_flag<C: ConnectionExt>(
        db: &DbConnection<C>,
    ) -> Result<(), GroupMessageProcessingError> {
        db.raw_query(|conn| {
            xmtp_db::diesel::update(xmtp_db::schema::user_preferences::table)
                .set(xmtp_db::schema::user_preferences::dm_group_updates_migrated.eq(false))
                .execute(conn)
        })?;
        Ok(())
    }

    /// The migration is one-time: once recorded, a later run must not touch
    /// anything. This is what the flag exists for, and what a bare `UPDATE`
    /// against a missing preferences row failed to deliver.
    #[xmtp_common::test(unwrap_try = true)]
    async fn test_cleanup_runs_only_once() {
        tester!(alix);

        // The startup run recorded it; a second run is a no-op.
        assert!(
            alix.db()
                .load_user_preferences()
                .await?
                .dm_group_updates_migrated
        );

        clear_migrated_flag(&alix.db())?;
        perform(alix.db()).await;
        assert!(
            alix.db()
                .load_user_preferences()
                .await?
                .dm_group_updates_migrated,
            "running the migration records that it ran"
        );
    }
}
