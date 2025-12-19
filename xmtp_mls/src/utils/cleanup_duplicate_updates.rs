use std::collections::HashSet;
use std::hash::{DefaultHasher, Hash, Hasher};

use tracing::info;
use xmtp_db::diesel::prelude::*;
use xmtp_db::user_preferences::StoredUserPreferences;
use xmtp_db::{ConnectionExt, DbConnection};
use xmtp_db::{
    group::{ConversationType, GroupQueryArgs, QueryGroup},
    group_message::{ContentType, MsgQueryArgs},
    prelude::QueryGroupMessage,
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
    let prefs = StoredUserPreferences::load(&db)?;
    if prefs.dm_group_updates_migrated {
        info!("DM group updates migration has already been performed. Skipping.");
        return Ok(());
    }

    let mut group_offset = 0;
    let mut groups;
    loop {
        groups = db.find_groups_by_id_paged(
            GroupQueryArgs {
                conversation_type: Some(ConversationType::Dm),
                limit: Some(BATCH_SIZE),
                ..Default::default()
            },
            group_offset,
        )?;

        if groups.is_empty() {
            break;
        }

        for group in groups {
            let mut sent_after_ns = None;
            let mut msgs;
            let mut originals: HashSet<u64> = HashSet::default();

            loop {
                msgs = db.get_group_messages(
                    &group.id,
                    &MsgQueryArgs {
                        content_types: Some(vec![ContentType::GroupUpdated]),
                        sent_after_ns,
                        limit: Some(BATCH_SIZE),
                        ..Default::default()
                    },
                )?;

                {
                    let Some(msg) = msgs.last() else {
                        break;
                    };
                    sent_after_ns = Some(msg.sent_at_ns);
                }

                let msgs = enrich_messages(&db, &group.id, msgs)?;

                for msg in msgs {
                    let MessageBody::GroupUpdated(update) = msg.content else {
                        continue;
                    };

                    let mut hasher = DefaultHasher::new();
                    update.added_inboxes.hash(&mut hasher);
                    update.removed_inboxes.hash(&mut hasher);
                    update.metadata_field_changes.hash(&mut hasher);
                    if originals.insert(hasher.finish()) {
                        continue;
                    }

                    db.raw_query_write(|conn| {
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

    db.raw_query_write(|conn| {
        xmtp_db::diesel::update(xmtp_db::schema::user_preferences::table)
            .set(xmtp_db::schema::user_preferences::dm_group_updates_migrated.eq(true))
            .execute(conn)
    })?;

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

    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

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
            }
        };

        let (dm, _) = alix.test_talk_in_dm_with(&bo).await?;

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
            let msg1 = gen_update_msg(dm.group_id.clone(), payload1.clone());
            msg1.store(&alix.db())?;
            let msg2 = gen_update_msg(dm.group_id.clone(), payload2.clone());
            msg2.store(&alix.db())?;

            if i > 0 {
                duplicates.push(msg1.id);
                duplicates.push(msg2.id);
            }
        }

        perform(alix.db()).await;

        let msgs = dm.find_messages_v2(&MsgQueryArgs {
            content_types: Some(vec![ContentType::GroupUpdated]),
            ..Default::default()
        })?;

        for msg in msgs {
            assert!(
                !duplicates.contains(&msg.metadata.id),
                "A duplicate has remained in the db {:?}",
                msg.metadata.id
            );
        }

        // Let's insert another duplicate and make sure it stays this time.
        // We don't want the perform to run more than once.
        let msg = gen_update_msg(dm.group_id.clone(), payload1.clone());
        msg.store(&alix.db())?;
        perform(alix.db()).await;

        // The duplicate should remain because perform will only clean up once.
        let msgs = dm.find_messages_v2(&MsgQueryArgs {
            content_types: Some(vec![ContentType::GroupUpdated]),
            ..Default::default()
        })?;
        assert!(msgs.iter().any(|m| m.metadata.id == msg.id));
    }
}
