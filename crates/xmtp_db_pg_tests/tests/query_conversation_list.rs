//! The sqlx `QueryConversationList` impl.
//!
//! One method, but it is the only query in the port that reads a **view**, and
//! the view is where most of the behavior lives: `conversation_list` pairs each
//! group with its latest *readable* message via a `ROW_NUMBER()` window. These
//! tests pin the view's contribution as much as the query's, since a Postgres
//! rewrite of either would fail the same way — a plausible-looking list with the
//! wrong message attached.

use xmtp_db::consent_record::{ConsentState, ConsentType};
use xmtp_db::conversation_list::{ConversationListItem, QueryConversationList};
use xmtp_db::group::{
    ConversationType, GroupMembershipState, GroupQueryArgs, GroupQueryOrderBy, QueryGroup,
    StoredGroup,
};
use xmtp_db::group_message::{ContentType, DeliveryStatus, GroupMessageKind};
use xmtp_db::pg::PgDb;
use xmtp_db_pg_tests::fresh_db;
use xmtp_proto::types::GroupId;

fn gid(n: u8) -> GroupId {
    GroupId::from([n; 16])
}

async fn make_group(db: &PgDb, id: GroupId, created_at_ns: i64, dm_id: Option<&str>) {
    let mut builder = StoredGroup::builder();
    builder
        .id(id)
        .created_at_ns(created_at_ns)
        .membership_state(GroupMembershipState::Allowed)
        .added_by_inbox_id("inbox");
    if let Some(dm_id) = dm_id {
        builder.dm_id(Some(dm_id.to_string()));
    }
    db.insert_or_replace_group(builder.build().unwrap())
        .await
        .unwrap();
}

async fn make_virtual_group(db: &PgDb, id: GroupId, kind: ConversationType) {
    db.insert_or_replace_group(
        StoredGroup::builder()
            .id(id)
            .created_at_ns(0)
            .membership_state(GroupMembershipState::Allowed)
            .added_by_inbox_id("inbox")
            .conversation_type(kind)
            .build()
            .unwrap(),
    )
    .await
    .unwrap();
}

/// `delivery_status` is always set explicitly: the column defaults to 0, which
/// is not a `DeliveryStatus` variant, so a row taking the default cannot be
/// decoded.
async fn insert_message(
    db: &PgDb,
    id: u8,
    group_id: GroupId,
    sent_at_ns: i64,
    content_type: ContentType,
) {
    let mut c = db.conn().await.unwrap();
    sqlx::query(
        "INSERT INTO group_messages \
         (id, group_id, decrypted_message_bytes, sent_at_ns, kind, sender_installation_id, \
          sender_inbox_id, delivery_status, content_type, version_major, version_minor, \
          authority_id, originator_id, sequence_id, inserted_at_ns, should_push, idempotency_key) \
         VALUES ($1,$2,$3,$4,$5,$6,'sender',$7,$8,1,0,'xmtp.org',1,$9,$4,TRUE,'')",
    )
    .bind(vec![id])
    .bind(group_id)
    .bind(vec![id])
    .bind(sent_at_ns)
    .bind(GroupMessageKind::Application)
    .bind(vec![1u8])
    .bind(DeliveryStatus::Published)
    .bind(content_type)
    .bind(id as i64)
    .execute(&mut *c)
    .await
    .unwrap();
}

async fn set_consent(db: &PgDb, id: &GroupId, state: ConsentState) {
    let mut c = db.conn().await.unwrap();
    sqlx::query(
        "INSERT INTO consent_records (entity_type, state, entity, consented_at_ns) \
         VALUES ($1, $2, $3, 0)",
    )
    .bind(ConsentType::ConversationId)
    .bind(state)
    .bind(hex::encode(id.as_slice()))
    .execute(&mut *c)
    .await
    .unwrap();
}

fn ids(items: &[ConversationListItem]) -> Vec<GroupId> {
    items.iter().map(|item| item.id).collect()
}

// --- the view's own behavior ------------------------------------------------

/// Each conversation carries its latest message, and a group with none still
/// appears with every message column NULL.
#[tokio::test]
async fn each_conversation_carries_its_latest_message() {
    let db = fresh_db("cl_latest").await;
    make_group(&db, gid(1), 10, None).await;
    make_group(&db, gid(2), 20, None).await;
    insert_message(&db, 1, gid(1), 100, ContentType::Text).await;
    insert_message(&db, 2, gid(1), 300, ContentType::Text).await;
    insert_message(&db, 3, gid(1), 200, ContentType::Text).await;

    let list = db
        .fetch_conversation_list(&GroupQueryArgs::default())
        .await
        .unwrap();

    let with_messages = list.iter().find(|item| item.id == gid(1)).unwrap();
    assert_eq!(with_messages.message_id, Some(vec![2]));
    assert_eq!(with_messages.sent_at_ns, Some(300));
    assert_eq!(with_messages.kind, Some(GroupMessageKind::Application));
    assert_eq!(with_messages.content_type, Some(ContentType::Text));
    assert_eq!(with_messages.sender_inbox_id.as_deref(), Some("sender"));

    let empty = list.iter().find(|item| item.id == gid(2)).unwrap();
    assert_eq!(empty.message_id, None);
    assert_eq!(empty.sent_at_ns, None);
    assert_eq!(empty.kind, None);
    assert_eq!(empty.created_at_ns, 20, "group columns are still populated");
}

/// The view ranks only the content types it considers readable — the list
/// `IN (0, 1, 4, 6, 7, 8, 9, 10)`, which is byte-identical in both schemas.
/// Membership changes, group-updated events and read receipts are excluded;
/// reactions deliberately are not.
#[tokio::test]
async fn only_readable_content_types_become_the_preview() {
    let db = fresh_db("cl_readable").await;
    make_group(&db, gid(1), 10, None).await;
    insert_message(&db, 1, gid(1), 100, ContentType::Text).await;
    insert_message(&db, 2, gid(1), 200, ContentType::GroupUpdated).await;
    insert_message(&db, 3, gid(1), 300, ContentType::ReadReceipt).await;
    insert_message(&db, 4, gid(1), 400, ContentType::GroupMembershipChange).await;

    let list = db
        .fetch_conversation_list(&GroupQueryArgs::default())
        .await
        .unwrap();
    assert_eq!(
        list[0].message_id,
        Some(vec![1]),
        "the newest *readable* message"
    );

    insert_message(&db, 5, gid(1), 500, ContentType::Reaction).await;
    let list = db
        .fetch_conversation_list(&GroupQueryArgs::default())
        .await
        .unwrap();
    assert_eq!(
        list[0].message_id,
        Some(vec![5]),
        "a reaction is readable and can be the preview"
    );
}

// --- filters ----------------------------------------------------------------

#[tokio::test]
async fn virtual_conversations_are_excluded_unless_asked_for() {
    let db = fresh_db("cl_virtual").await;
    make_group(&db, gid(1), 10, None).await;
    make_virtual_group(&db, gid(2), ConversationType::Sync).await;
    make_virtual_group(&db, gid(3), ConversationType::Oneshot).await;

    let default = db
        .fetch_conversation_list(&GroupQueryArgs::default())
        .await
        .unwrap();
    assert_eq!(ids(&default), vec![gid(1)]);

    let with_sync = db
        .fetch_conversation_list(&GroupQueryArgs {
            include_sync_groups: true,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(ids(&with_sync), vec![gid(1), gid(2)]);

    let only_sync = db
        .fetch_conversation_list(&GroupQueryArgs {
            conversation_type: Some(ConversationType::Sync),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(ids(&only_sync), vec![gid(2)]);
}

/// Stitched DMs collapse to the row with the newest `last_message_ns` --
/// maintained on `groups` by a trigger, and reached here through a correlated
/// subquery because the view does not expose it.
#[tokio::test]
async fn stitched_dms_collapse_to_the_most_recently_active() {
    let db = fresh_db("cl_dm_dedup").await;
    make_group(&db, gid(1), 10, Some("dm:a:b")).await;
    make_group(&db, gid(2), 20, Some("dm:a:b")).await;
    make_group(&db, gid(3), 30, Some("dm:c:d")).await;
    make_group(&db, gid(4), 40, None).await;
    // The trigger on group_messages advances groups.last_message_ns.
    insert_message(&db, 1, gid(1), 100, ContentType::Text).await;
    insert_message(&db, 2, gid(2), 500, ContentType::Text).await;

    let list = db
        .fetch_conversation_list(&GroupQueryArgs::default())
        .await
        .unwrap();
    let mut found = ids(&list);
    found.sort();
    assert_eq!(found, vec![gid(2), gid(3), gid(4)]);

    let all = db
        .fetch_conversation_list(&GroupQueryArgs {
            include_duplicate_dms: true,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(all.len(), 4);
}

/// Both stitched DMs are unused, so `last_message_ns` is NULL on each and the
/// id breaks the tie. Without the `COALESCE(..., 0)` the comparison would be
/// NULL and both rows would survive.
#[tokio::test]
async fn stitched_dms_with_no_messages_break_the_tie_on_id() {
    let db = fresh_db("cl_dm_null").await;
    make_group(&db, gid(1), 10, Some("dm:a:b")).await;
    make_group(&db, gid(2), 20, Some("dm:a:b")).await;

    let list = db
        .fetch_conversation_list(&GroupQueryArgs::default())
        .await
        .unwrap();
    assert_eq!(ids(&list), vec![gid(2)], "the higher id wins");
}

#[tokio::test]
async fn orders_newest_first_by_creation_or_activity() {
    let db = fresh_db("cl_order").await;
    make_group(&db, gid(1), 10, None).await;
    make_group(&db, gid(2), 20, None).await;
    make_group(&db, gid(3), 30, None).await;
    // The oldest group has the newest message.
    insert_message(&db, 1, gid(1), 900, ContentType::Text).await;

    let by_created = db
        .fetch_conversation_list(&GroupQueryArgs {
            order_by: Some(GroupQueryOrderBy::CreatedAt),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(
        ids(&by_created),
        vec![gid(3), gid(2), gid(1)],
        "newest-created first -- the opposite of find_groups"
    );

    let by_activity = db
        .fetch_conversation_list(&GroupQueryArgs {
            order_by: Some(GroupQueryOrderBy::LastActivity),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(ids(&by_activity), vec![gid(1), gid(3), gid(2)]);
}

#[tokio::test]
async fn applies_the_scalar_filters() {
    let db = fresh_db("cl_filters").await;
    make_group(&db, gid(1), 10, None).await;
    make_group(&db, gid(2), 20, None).await;
    make_group(&db, gid(3), 30, None).await;
    db.update_group_membership(&gid(3), GroupMembershipState::Pending)
        .await
        .unwrap();

    let window = db
        .fetch_conversation_list(&GroupQueryArgs {
            created_after_ns: Some(10),
            created_before_ns: Some(30),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(ids(&window), vec![gid(2)]);

    let pending = db
        .fetch_conversation_list(&GroupQueryArgs {
            allowed_states: Some(vec![GroupMembershipState::Pending]),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(ids(&pending), vec![gid(3)]);

    let limited = db
        .fetch_conversation_list(&GroupQueryArgs {
            limit: Some(2),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(ids(&limited), vec![gid(3), gid(2)]);
}

/// The activity window is over `COALESCE(sent_at_ns, created_at_ns)`, so a
/// conversation with no messages is judged by when it was created.
#[tokio::test]
async fn activity_window_falls_back_to_created_at() {
    let db = fresh_db("cl_activity").await;
    make_group(&db, gid(1), 100, None).await;
    make_group(&db, gid(2), 10, None).await;
    insert_message(&db, 1, gid(2), 500, ContentType::Text).await;

    let recent = db
        .fetch_conversation_list(&GroupQueryArgs {
            last_activity_after_ns: Some(50),
            ..Default::default()
        })
        .await
        .unwrap();
    let mut recent = ids(&recent);
    recent.sort();
    assert_eq!(recent, vec![gid(1), gid(2)]);

    let old = db
        .fetch_conversation_list(&GroupQueryArgs {
            last_activity_before_ns: Some(200),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(ids(&old), vec![gid(1)]);
}

// --- consent ----------------------------------------------------------------

#[tokio::test]
async fn default_consent_keeps_unknown_and_unrecorded() {
    let db = fresh_db("cl_consent_def").await;
    for n in 1..=4u8 {
        make_group(&db, gid(n), n as i64, None).await;
    }
    set_consent(&db, &gid(2), ConsentState::Allowed).await;
    set_consent(&db, &gid(3), ConsentState::Unknown).await;
    set_consent(&db, &gid(4), ConsentState::Denied).await;

    let list = db
        .fetch_conversation_list(&GroupQueryArgs::default())
        .await
        .unwrap();
    assert_eq!(ids(&list), vec![gid(3), gid(2), gid(1)]);
}

#[tokio::test]
async fn explicit_states_require_a_consent_record() {
    let db = fresh_db("cl_consent_inner").await;
    make_group(&db, gid(1), 1, None).await; // no consent record
    make_group(&db, gid(2), 2, None).await;
    set_consent(&db, &gid(2), ConsentState::Denied).await;

    let denied = db
        .fetch_conversation_list(&GroupQueryArgs {
            consent_states: Some(vec![ConsentState::Denied]),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(ids(&denied), vec![gid(2)]);
}

#[tokio::test]
async fn all_consent_states_skip_the_join() {
    let db = fresh_db("cl_consent_all").await;
    make_group(&db, gid(1), 1, None).await;
    make_group(&db, gid(2), 2, None).await;
    set_consent(&db, &gid(2), ConsentState::Denied).await;

    let list = db
        .fetch_conversation_list(&GroupQueryArgs {
            consent_states: Some(vec![
                ConsentState::Allowed,
                ConsentState::Denied,
                ConsentState::Unknown,
            ]),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(ids(&list), vec![gid(2), gid(1)]);
}

#[tokio::test]
async fn rejects_conflicting_time_filters() {
    let db = fresh_db("cl_conflict").await;
    assert!(
        db.fetch_conversation_list(&GroupQueryArgs {
            created_before_ns: Some(1),
            last_activity_before_ns: Some(1),
            ..Default::default()
        })
        .await
        .is_err()
    );
}
