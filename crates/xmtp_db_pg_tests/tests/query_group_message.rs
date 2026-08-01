//! The sqlx `QueryGroupMessage` impl, method by method.
//!
//! Two things carry most of the risk and get most of the tests: the shared
//! `$n IS NULL OR ...` filter block that every listing splices in (a bind that
//! drifts out of order would silently filter on the wrong column), and
//! `messages_newer_than`, whose one-statement `NOT EXISTS` form replaces the
//! sync path's hand-built tree of `OR`s.

use std::collections::HashMap;
use xmtp_db::group::{ConversationType, GroupMembershipState, QueryGroup, StoredGroup};
use xmtp_db::group_message::{
    ContentType, DeliveryStatus, GroupMessageKind, MsgQueryArgs, QueryGroupMessage, RelationQuery,
    SortBy, SortDirection, StoredGroupMessage,
};
use xmtp_db::pg::PgDb;
use xmtp_db_pg_tests::fresh_db;
use xmtp_proto::types::{Cursor, GlobalCursor, GroupId};

fn gid(n: u8) -> GroupId {
    GroupId::from([n; 16])
}

async fn make_group(db: &PgDb, id: GroupId, dm_id: Option<&str>) {
    let mut builder = StoredGroup::builder();
    builder
        .id(id)
        .created_at_ns(0)
        .membership_state(GroupMembershipState::Allowed)
        .added_by_inbox_id("inbox");
    if let Some(dm_id) = dm_id {
        builder.dm_id(Some(dm_id.to_string()));
    }
    db.insert_or_replace_group(builder.build().unwrap())
        .await
        .unwrap();
}

/// A published Application/Text message. `delivery_status` is always set
/// explicitly: the column defaults to 0, which is not a `DeliveryStatus`
/// variant, so a row taking the default cannot be decoded on either track.
fn msg(id: u8, group_id: GroupId, sent_at_ns: i64) -> StoredGroupMessage {
    StoredGroupMessage {
        id: vec![id],
        group_id,
        decrypted_message_bytes: vec![id],
        sent_at_ns,
        kind: GroupMessageKind::Application,
        sender_installation_id: vec![1],
        sender_inbox_id: "sender".to_string(),
        delivery_status: DeliveryStatus::Published,
        content_type: ContentType::Text,
        version_major: 1,
        version_minor: 0,
        authority_id: "xmtp.org".to_string(),
        reference_id: None,
        originator_id: 1,
        sequence_id: id as i64,
        inserted_at_ns: sent_at_ns,
        expire_at_ns: None,
        should_push: true,
        idempotency_key: String::new(),
    }
}

async fn insert(db: &PgDb, message: &StoredGroupMessage) {
    let mut c = db.conn().await.unwrap();
    sqlx::query(
        "INSERT INTO group_messages \
         (id, group_id, decrypted_message_bytes, sent_at_ns, kind, sender_installation_id, \
          sender_inbox_id, delivery_status, content_type, version_major, version_minor, \
          authority_id, reference_id, originator_id, sequence_id, inserted_at_ns, expire_at_ns, \
          should_push, idempotency_key) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19)",
    )
    .bind(&message.id)
    .bind(message.group_id)
    .bind(&message.decrypted_message_bytes)
    .bind(message.sent_at_ns)
    .bind(message.kind)
    .bind(&message.sender_installation_id)
    .bind(&message.sender_inbox_id)
    .bind(message.delivery_status)
    .bind(message.content_type)
    .bind(message.version_major)
    .bind(message.version_minor)
    .bind(&message.authority_id)
    .bind(&message.reference_id)
    .bind(message.originator_id)
    .bind(message.sequence_id)
    .bind(message.inserted_at_ns)
    .bind(message.expire_at_ns)
    .bind(message.should_push)
    .bind(&message.idempotency_key)
    .execute(&mut *c)
    .await
    .unwrap();
}

fn ids(messages: &[StoredGroupMessage]) -> Vec<u8> {
    messages.iter().map(|m| m.id[0]).collect()
}

/// One group holding messages 1..=3 at sent_at 10/20/30.
async fn three_messages(name: &str) -> PgDb {
    let db = fresh_db(name).await;
    make_group(&db, gid(1), None).await;
    for (id, sent_at) in [(1u8, 10i64), (2, 20), (3, 30)] {
        insert(&db, &msg(id, gid(1), sent_at)).await;
    }
    db
}

// --- get_group_messages -----------------------------------------------------

#[tokio::test]
async fn get_group_messages_returns_the_group_in_sent_order() {
    let db = three_messages("m_basic").await;
    let found = db
        .get_group_messages(&gid(1), &MsgQueryArgs::default())
        .await
        .unwrap();
    assert_eq!(ids(&found), vec![1, 2, 3]);
}

/// Every optional filter in one test, because the risk being covered is a bind
/// landing on the wrong `$n` rather than any single predicate being wrong.
#[tokio::test]
async fn get_group_messages_applies_every_optional_filter() {
    let db = fresh_db("m_filters").await;
    make_group(&db, gid(1), None).await;

    let mut plain = msg(1, gid(1), 10);
    plain.inserted_at_ns = 100;
    insert(&db, &plain).await;

    let mut membership = msg(2, gid(1), 20);
    membership.kind = GroupMessageKind::MembershipChange;
    membership.content_type = ContentType::GroupMembershipChange;
    membership.inserted_at_ns = 200;
    insert(&db, &membership).await;

    let mut unpublished = msg(3, gid(1), 30);
    unpublished.delivery_status = DeliveryStatus::Unpublished;
    unpublished.sender_inbox_id = "other".to_string();
    unpublished.inserted_at_ns = 300;
    insert(&db, &unpublished).await;

    let query =
        async |args: MsgQueryArgs| ids(&db.get_group_messages(&gid(1), &args).await.unwrap());

    assert_eq!(
        query(MsgQueryArgs {
            sent_after_ns: Some(10),
            sent_before_ns: Some(30),
            ..Default::default()
        })
        .await,
        vec![2]
    );
    assert_eq!(
        query(MsgQueryArgs {
            kind: Some(GroupMessageKind::MembershipChange),
            ..Default::default()
        })
        .await,
        vec![2]
    );
    assert_eq!(
        query(MsgQueryArgs {
            delivery_status: Some(DeliveryStatus::Unpublished),
            ..Default::default()
        })
        .await,
        vec![3]
    );
    assert_eq!(
        query(MsgQueryArgs {
            content_types: Some(vec![ContentType::GroupMembershipChange]),
            ..Default::default()
        })
        .await,
        vec![2]
    );
    assert_eq!(
        query(MsgQueryArgs {
            exclude_content_types: Some(vec![ContentType::Text]),
            ..Default::default()
        })
        .await,
        vec![2]
    );
    assert_eq!(
        query(MsgQueryArgs {
            exclude_sender_inbox_ids: Some(vec!["sender".to_string()]),
            ..Default::default()
        })
        .await,
        vec![3]
    );
    assert_eq!(
        query(MsgQueryArgs {
            inserted_after_ns: Some(100),
            inserted_before_ns: Some(300),
            ..Default::default()
        })
        .await,
        vec![2]
    );
    assert_eq!(
        query(MsgQueryArgs {
            limit: Some(2),
            ..Default::default()
        })
        .await,
        vec![1, 2]
    );
}

#[tokio::test]
async fn get_group_messages_hides_already_expired_messages() {
    let db = fresh_db("m_expired").await;
    make_group(&db, gid(1), None).await;

    let mut expired = msg(1, gid(1), 10);
    expired.expire_at_ns = Some(1);
    insert(&db, &expired).await;

    let mut later = msg(2, gid(1), 20);
    later.expire_at_ns = Some(i64::MAX);
    insert(&db, &later).await;

    insert(&db, &msg(3, gid(1), 30)).await;

    let found = db
        .get_group_messages(&gid(1), &MsgQueryArgs::default())
        .await
        .unwrap();
    assert_eq!(ids(&found), vec![2, 3]);
}

#[tokio::test]
async fn get_group_messages_sorts_by_sent_or_inserted_in_either_direction() {
    let db = fresh_db("m_sort").await;
    make_group(&db, gid(1), None).await;
    // Insertion order deliberately disagrees with send order.
    for (id, sent_at, inserted_at) in [(1u8, 30i64, 10i64), (2, 20, 20), (3, 10, 30)] {
        let mut m = msg(id, gid(1), sent_at);
        m.inserted_at_ns = inserted_at;
        insert(&db, &m).await;
    }

    let sorted = async |sort_by: SortBy, direction: SortDirection| {
        ids(&db
            .get_group_messages(
                &gid(1),
                &MsgQueryArgs {
                    sort_by: Some(sort_by),
                    direction: Some(direction),
                    ..Default::default()
                },
            )
            .await
            .unwrap())
    };

    assert_eq!(
        sorted(SortBy::SentAt, SortDirection::Ascending).await,
        vec![3, 2, 1]
    );
    assert_eq!(
        sorted(SortBy::SentAt, SortDirection::Descending).await,
        vec![1, 2, 3]
    );
    assert_eq!(
        sorted(SortBy::InsertedAt, SortDirection::Ascending).await,
        vec![1, 2, 3]
    );
    assert_eq!(
        sorted(SortBy::InsertedAt, SortDirection::Descending).await,
        vec![3, 2, 1]
    );
}

/// Messages sent at the same instant fall back to `rowid`, the insertion-order
/// column `migrations_pg` materializes because Postgres has no implicit rowid.
#[tokio::test]
async fn get_group_messages_breaks_ties_on_insertion_order() {
    let db = fresh_db("m_tiebreak").await;
    make_group(&db, gid(1), None).await;
    for id in [3u8, 1, 2] {
        insert(&db, &msg(id, gid(1), 100)).await;
    }

    let ascending = db
        .get_group_messages(&gid(1), &MsgQueryArgs::default())
        .await
        .unwrap();
    assert_eq!(ids(&ascending), vec![3, 1, 2], "insertion order");

    let descending = db
        .get_group_messages(
            &gid(1),
            &MsgQueryArgs {
                direction: Some(SortDirection::Descending),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(ids(&descending), vec![2, 1, 3]);
}

/// A DM's listing spans every group stitched to it by `dm_id`; a non-DM group's
/// listing sees only its own messages.
#[tokio::test]
async fn get_group_messages_spans_stitched_dms() {
    let db = fresh_db("m_stitch").await;
    make_group(&db, gid(1), Some("dm:a:b")).await;
    make_group(&db, gid(2), Some("dm:a:b")).await;
    make_group(&db, gid(3), Some("dm:c:d")).await;
    make_group(&db, gid(4), None).await;
    insert(&db, &msg(1, gid(1), 10)).await;
    insert(&db, &msg(2, gid(2), 20)).await;
    insert(&db, &msg(3, gid(3), 30)).await;
    insert(&db, &msg(4, gid(4), 40)).await;

    let stitched = db
        .get_group_messages(&gid(1), &MsgQueryArgs::default())
        .await
        .unwrap();
    assert_eq!(ids(&stitched), vec![1, 2]);

    let plain = db
        .get_group_messages(&gid(4), &MsgQueryArgs::default())
        .await
        .unwrap();
    assert_eq!(ids(&plain), vec![4]);
}

// --- count_group_messages ---------------------------------------------------

#[tokio::test]
async fn count_group_messages_uses_the_same_filters() {
    let db = three_messages("m_count").await;
    assert_eq!(
        db.count_group_messages(&gid(1), &MsgQueryArgs::default())
            .await
            .unwrap(),
        3
    );
    assert_eq!(
        db.count_group_messages(
            &gid(1),
            &MsgQueryArgs {
                sent_after_ns: Some(15),
                ..Default::default()
            }
        )
        .await
        .unwrap(),
        2
    );
}

/// DMs accumulate duplicate GroupUpdated messages that the listing path dedupes
/// after the fact. A count cannot, so they are excluded unless asked for.
#[tokio::test]
async fn count_group_messages_excludes_group_updated_in_dms() {
    let db = fresh_db("m_count_dm").await;
    make_group(&db, gid(1), Some("dm:a:b")).await;
    make_group(&db, gid(2), None).await;
    for group in [gid(1), gid(2)] {
        let mut updated = msg(if group == gid(1) { 1 } else { 3 }, group, 10);
        updated.content_type = ContentType::GroupUpdated;
        insert(&db, &updated).await;
        insert(&db, &msg(if group == gid(1) { 2 } else { 4 }, group, 20)).await;
    }

    assert_eq!(
        db.count_group_messages(&gid(1), &MsgQueryArgs::default())
            .await
            .unwrap(),
        1,
        "the DM's GroupUpdated is dropped"
    );
    assert_eq!(
        db.count_group_messages(
            &gid(1),
            &MsgQueryArgs {
                content_types: Some(vec![ContentType::GroupUpdated, ContentType::Text]),
                ..Default::default()
            }
        )
        .await
        .unwrap(),
        2,
        "unless explicitly requested"
    );
    assert_eq!(
        db.count_group_messages(&gid(2), &MsgQueryArgs::default())
            .await
            .unwrap(),
        2,
        "a regular group keeps its GroupUpdated"
    );
}

// --- other listings ---------------------------------------------------------

#[tokio::test]
async fn missing_messages_returns_application_messages_not_in_the_set() {
    let db = fresh_db("m_missing").await;
    make_group(&db, gid(1), None).await;
    for id in 1..=3u8 {
        insert(&db, &msg(id, gid(1), id as i64 * 10)).await;
    }
    let mut membership = msg(4, gid(1), 40);
    membership.kind = GroupMessageKind::MembershipChange;
    insert(&db, &membership).await;

    let missing = db.missing_messages(&gid(1), &[2]).await.unwrap();
    assert_eq!(
        ids(&missing),
        vec![1, 3],
        "sequence 2 is known; the membership change is not an Application message"
    );

    // An empty set means nothing is known, so everything is missing.
    let all = db.missing_messages(&gid(1), &[]).await.unwrap();
    assert_eq!(ids(&all), vec![1, 2, 3]);
}

#[tokio::test]
async fn group_messages_paged_walks_every_group_by_id() {
    let db = fresh_db("m_paged").await;
    make_group(&db, gid(1), None).await;
    make_group(&db, gid(2), None).await;
    insert(&db, &msg(1, gid(1), 10)).await;
    insert(&db, &msg(2, gid(2), 20)).await;
    insert(&db, &msg(3, gid(1), 30)).await;

    let page = db
        .group_messages_paged(
            &MsgQueryArgs {
                limit: Some(2),
                ..Default::default()
            },
            1,
        )
        .await
        .unwrap();
    assert_eq!(ids(&page), vec![2, 3]);

    // Virtual conversations are skipped entirely.
    let sync_group = StoredGroup::builder()
        .id(gid(9))
        .created_at_ns(0)
        .membership_state(GroupMembershipState::Allowed)
        .added_by_inbox_id("inbox")
        .conversation_type(ConversationType::Sync)
        .build()
        .unwrap();
    db.insert_or_replace_group(sync_group).await.unwrap();
    insert(&db, &msg(4, gid(9), 40)).await;

    let all = db
        .group_messages_paged(&MsgQueryArgs::default(), 0)
        .await
        .unwrap();
    assert_eq!(ids(&all), vec![1, 2, 3]);
}

#[tokio::test]
async fn group_messages_paged_can_exclude_disappearing_messages() {
    let db = fresh_db("m_paged_disappear").await;
    make_group(&db, gid(1), None).await;
    insert(&db, &msg(1, gid(1), 10)).await;
    let mut disappearing = msg(2, gid(1), 20);
    disappearing.expire_at_ns = Some(i64::MAX);
    insert(&db, &disappearing).await;

    let default = db
        .group_messages_paged(&MsgQueryArgs::default(), 0)
        .await
        .unwrap();
    assert_eq!(ids(&default), vec![1, 2], "unexpired ones are kept");

    let excluded = db
        .group_messages_paged(
            &MsgQueryArgs {
                exclude_disappearing: true,
                ..Default::default()
            },
            0,
        )
        .await
        .unwrap();
    assert_eq!(ids(&excluded), vec![1]);
}

// --- relations --------------------------------------------------------------

/// Builds a group with two messages (1, 2) and three reactions to message 1.
async fn with_reactions(name: &str) -> PgDb {
    let db = fresh_db(name).await;
    make_group(&db, gid(1), None).await;
    insert(&db, &msg(1, gid(1), 10)).await;
    insert(&db, &msg(2, gid(1), 20)).await;
    for (id, sent_at, target) in [(3u8, 30i64, 1u8), (4, 40, 1), (5, 50, 2)] {
        let mut reaction = msg(id, gid(1), sent_at);
        reaction.content_type = ContentType::Reaction;
        reaction.reference_id = Some(vec![target]);
        insert(&db, &reaction).await;
    }
    db
}

#[tokio::test]
async fn get_group_messages_with_reactions_excludes_reactions_from_the_main_list() {
    let db = with_reactions("m_reactions").await;
    let results = db
        .get_group_messages_with_reactions(&gid(1), &MsgQueryArgs::default())
        .await
        .unwrap();

    assert_eq!(
        results.iter().map(|r| r.message.id[0]).collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(ids(&results[0].reactions), vec![3, 4]);
    assert_eq!(ids(&results[1].reactions), vec![5]);
}

#[tokio::test]
async fn inbound_relations_group_by_referenced_message() {
    let db = with_reactions("m_inbound").await;
    let inbound = db
        .get_inbound_relations(&gid(1), &[&[1u8][..], &[2u8][..]], RelationQuery::default())
        .await
        .unwrap();
    assert_eq!(ids(&inbound[&vec![1u8]]), vec![3, 4]);
    assert_eq!(ids(&inbound[&vec![2u8]]), vec![5]);

    // Same query, restricted to a content type nothing matches.
    let none = db
        .get_inbound_relations(
            &gid(1),
            &[&[1u8][..]],
            RelationQuery {
                content_types: Some(vec![ContentType::Reply]),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(none.is_empty());
}

#[tokio::test]
async fn inbound_relation_counts_match_the_relations() {
    let db = with_reactions("m_counts").await;
    let counts = db
        .get_inbound_relation_counts(&gid(1), &[&[1u8][..], &[2u8][..]], RelationQuery::default())
        .await
        .unwrap();
    assert_eq!(counts[&vec![1u8]], 2);
    assert_eq!(counts[&vec![2u8]], 1);
}

#[tokio::test]
async fn outbound_relations_key_by_referenced_id() {
    let db = with_reactions("m_outbound").await;
    let outbound = db
        .get_outbound_relations(&gid(1), &[&[1u8][..], &[9u8][..]])
        .await
        .unwrap();
    assert_eq!(outbound.len(), 1);
    assert_eq!(outbound[&vec![1u8]].sent_at_ns, 10);
}

#[tokio::test]
async fn latest_message_times_by_sender() {
    let db = fresh_db("m_latest").await;
    make_group(&db, gid(1), None).await;
    for (id, sent_at, sender) in [(1u8, 10i64, "a"), (2, 30, "a"), (3, 20, "b")] {
        let mut m = msg(id, gid(1), sent_at);
        m.sender_inbox_id = sender.to_string();
        insert(&db, &m).await;
    }
    let mut reaction = msg(4, gid(1), 99);
    reaction.content_type = ContentType::Reaction;
    reaction.sender_inbox_id = "a".to_string();
    insert(&db, &reaction).await;

    let latest = db
        .get_latest_message_times_by_sender(gid(1), &[ContentType::Text])
        .await
        .unwrap();
    assert_eq!(latest["a"], 30, "the reaction is not an allowed type");
    assert_eq!(latest["b"], 20);
}

// --- single-message lookups and updates -------------------------------------

#[tokio::test]
async fn single_message_lookups() {
    let db = three_messages("m_lookups").await;

    assert!(db.get_group_message([1u8]).await.unwrap().is_some());
    assert!(db.get_group_message([9u8]).await.unwrap().is_none());
    assert!(
        db.write_conn_get_group_message([1u8])
            .await
            .unwrap()
            .is_some()
    );

    let by_timestamp = db
        .get_group_message_by_timestamp(gid(1), 20)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(by_timestamp.id, vec![2]);

    let by_cursor = db
        .get_group_message_by_cursor(gid(1), Cursor::new(3, 1u32))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(by_cursor.id, vec![3]);
    assert!(
        db.get_group_message_by_cursor(gid(1), Cursor::new(3, 9u32))
            .await
            .unwrap()
            .is_none(),
        "the originator is part of the cursor"
    );
}

#[tokio::test]
async fn delivery_status_transitions() {
    let db = fresh_db("m_delivery").await;
    make_group(&db, gid(1), None).await;
    let mut pending = msg(1, gid(1), 10);
    pending.delivery_status = DeliveryStatus::Unpublished;
    insert(&db, &pending).await;

    let updated = db
        .set_delivery_status_to_published(&[1u8], 99, Cursor::new(7, 2u32), Some(500))
        .await
        .unwrap();
    assert_eq!(updated, 1);

    let stored = db.get_group_message([1u8]).await.unwrap().unwrap();
    assert_eq!(stored.delivery_status, DeliveryStatus::Published);
    assert_eq!(stored.sent_at_ns, 99);
    assert_eq!(stored.cursor(), Cursor::new(7, 2u32));
    assert_eq!(stored.expire_at_ns, Some(500));

    assert_eq!(
        db.set_delivery_status_to_failed(&[1u8]).await.unwrap(),
        1,
        "rows affected"
    );
    let stored = db.get_group_message([1u8]).await.unwrap().unwrap();
    assert_eq!(stored.delivery_status, DeliveryStatus::Failed);

    assert_eq!(
        db.set_delivery_status_to_failed(&[9u8]).await.unwrap(),
        0,
        "an unknown message updates nothing"
    );
}

// --- deletion ---------------------------------------------------------------

#[tokio::test]
async fn expired_messages_are_deleted_and_returned() {
    let db = fresh_db("m_delete_expired").await;
    make_group(&db, gid(1), None).await;

    let mut expired = msg(1, gid(1), 10);
    expired.expire_at_ns = Some(1);
    insert(&db, &expired).await;

    let mut future = msg(2, gid(1), 20);
    future.expire_at_ns = Some(i64::MAX);
    insert(&db, &future).await;

    // Same deadline, but unpublished: not a disappearing message yet.
    let mut unpublished = msg(3, gid(1), 30);
    unpublished.expire_at_ns = Some(1);
    unpublished.delivery_status = DeliveryStatus::Unpublished;
    insert(&db, &unpublished).await;

    insert(&db, &msg(4, gid(1), 40)).await;

    let deleted = db.delete_expired_messages().await.unwrap();
    assert_eq!(ids(&deleted), vec![1]);
    assert!(db.get_group_message([1u8]).await.unwrap().is_none());
    assert!(db.get_group_message([3u8]).await.unwrap().is_some());
}

#[tokio::test]
async fn min_expire_at_ns_is_the_soonest_pending_expiry() {
    let db = fresh_db("m_min_expire").await;
    make_group(&db, gid(1), None).await;
    assert_eq!(db.min_expire_at_ns().await.unwrap(), None);

    for (id, expire_at) in [(1u8, 900i64), (2, 300), (3, 600)] {
        let mut m = msg(id, gid(1), 10);
        m.expire_at_ns = Some(expire_at);
        insert(&db, &m).await;
    }
    // Unpublished messages do not count toward the worker's next wake-up.
    let mut unpublished = msg(4, gid(1), 10);
    unpublished.expire_at_ns = Some(1);
    unpublished.delivery_status = DeliveryStatus::Unpublished;
    insert(&db, &unpublished).await;

    assert_eq!(db.min_expire_at_ns().await.unwrap(), Some(300));
}

#[tokio::test]
async fn delete_message_by_id_reports_rows_affected() {
    let db = three_messages("m_delete_one").await;
    assert_eq!(db.delete_message_by_id([2u8]).await.unwrap(), 1);
    assert_eq!(db.delete_message_by_id([2u8]).await.unwrap(), 0);
    assert_eq!(
        ids(&db
            .get_group_messages(&gid(1), &MsgQueryArgs::default())
            .await
            .unwrap()),
        vec![1, 3]
    );
}

#[tokio::test]
async fn clear_messages_filters_by_group_and_age() {
    let db = fresh_db("m_clear").await;
    make_group(&db, gid(1), None).await;
    make_group(&db, gid(2), None).await;
    insert(&db, &msg(1, gid(1), 10)).await;
    insert(&db, &msg(2, gid(2), 20)).await;
    insert(&db, &msg(3, gid(1), i64::MAX)).await;

    // Retention alone: everything older than the cutoff, in every group.
    assert_eq!(db.clear_messages(None, Some(1)).await.unwrap(), 2);
    assert_eq!(
        ids(&db
            .get_group_messages(&gid(1), &MsgQueryArgs::default())
            .await
            .unwrap()),
        vec![3]
    );

    // Group alone: everything in it, at any age.
    assert_eq!(db.clear_messages(Some(&[gid(1)]), None).await.unwrap(), 1);
    assert_eq!(db.clear_messages(None, None).await.unwrap(), 0);
}

// --- messages_newer_than ----------------------------------------------------

fn cursor_map(entries: &[(u32, u64)]) -> GlobalCursor {
    entries.iter().copied().collect()
}

fn sorted(mut cursors: Vec<(GroupId, Cursor)>) -> Vec<(GroupId, Cursor)> {
    cursors.sort_by_key(|(group_id, cursor)| {
        (group_id.to_vec(), cursor.originator_id, cursor.sequence_id)
    });
    cursors
}

/// The `NOT EXISTS` rewrite has to reproduce three behaviors at once: a group
/// with no cursor yields everything, a known originator yields only what is
/// ahead of its sequence id, and an originator the cursor has never seen yields
/// everything.
#[tokio::test]
async fn messages_newer_than_covers_known_and_unknown_originators() {
    let db = fresh_db("m_newer").await;
    make_group(&db, gid(1), None).await;
    make_group(&db, gid(2), None).await;

    for (id, originator, sequence) in [(1u8, 1i64, 5i64), (2, 1, 15), (3, 2, 1)] {
        let mut m = msg(id, gid(1), id as i64);
        m.originator_id = originator;
        m.sequence_id = sequence;
        insert(&db, &m).await;
    }
    let mut other_group = msg(4, gid(2), 40);
    other_group.originator_id = 1;
    other_group.sequence_id = 1;
    insert(&db, &other_group).await;

    let mut cursors = HashMap::new();
    cursors.insert(gid(1).to_vec(), cursor_map(&[(1, 10)]));
    cursors.insert(gid(2).to_vec(), GlobalCursor::default());

    let mut newer = sorted(db.messages_newer_than(&cursors).await.unwrap());
    let mut expected = sorted(vec![
        // originator 1 at sequence 15 is ahead of the cursor's 10
        (gid(1), Cursor::new(15, 1u32)),
        // originator 2 is unknown to the cursor, so it is entirely new
        (gid(1), Cursor::new(1, 2u32)),
        // an empty cursor means the whole group is new
        (gid(2), Cursor::new(1, 1u32)),
    ]);
    assert_eq!(newer, expected);

    // Message 1 (originator 1, sequence 5) is behind the cursor and must not
    // appear under any ordering.
    newer.retain(|(_, cursor)| cursor == &Cursor::new(5, 1u32));
    expected.clear();
    assert_eq!(newer, expected);
}

/// Groups absent from the map contribute nothing, and an empty map returns
/// nothing at all.
#[tokio::test]
async fn messages_newer_than_ignores_groups_not_asked_about() {
    let db = fresh_db("m_newer_scope").await;
    make_group(&db, gid(1), None).await;
    make_group(&db, gid(2), None).await;
    insert(&db, &msg(1, gid(1), 10)).await;
    insert(&db, &msg(2, gid(2), 20)).await;

    assert!(
        db.messages_newer_than(&HashMap::new())
            .await
            .unwrap()
            .is_empty()
    );

    let mut cursors = HashMap::new();
    cursors.insert(gid(2).to_vec(), GlobalCursor::default());
    let newer = db.messages_newer_than(&cursors).await.unwrap();
    assert_eq!(newer, vec![(gid(2), Cursor::new(2, 1u32))]);
}
