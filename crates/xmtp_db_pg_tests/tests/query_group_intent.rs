//! The sqlx `QueryGroupIntent` impl, method by method.
//!
//! Most of these methods are a guarded state transition whose *miss* carries the
//! meaning: which `NotFound` comes back distinguishes "no such intent" from "the
//! intent was in the wrong state". The tests below pin each transition's success
//! and its miss, since a wrong guard would only ever show up as a stuck intent.

use xmtp_db::group::{GroupMembershipState, QueryGroup, StoredGroup};
use xmtp_db::group_intent::{
    IntentKind, IntentState, NewGroupIntent, QueryGroupIntent, StoredGroupIntent,
};
use xmtp_db::group_message::{
    ContentType, DeliveryStatus, GroupMessageKind, QueryGroupMessage, StoredGroupMessage,
};
use xmtp_db::pg::PgDb;
use xmtp_db::refresh_state::{EntityKind, QueryRefreshState};
use xmtp_db::{NotFound, StorageError};
use xmtp_db_pg_tests::fresh_db;
use xmtp_proto::types::{Cursor, GroupId};

fn gid(n: u8) -> GroupId {
    GroupId::from([n; 16])
}

async fn make_group(db: &PgDb, id: GroupId) {
    db.insert_or_replace_group(
        StoredGroup::builder()
            .id(id)
            .created_at_ns(0)
            .membership_state(GroupMembershipState::Allowed)
            .added_by_inbox_id("inbox")
            .build()
            .unwrap(),
    )
    .await
    .unwrap();
}

fn new_intent(kind: IntentKind, group_id: GroupId) -> NewGroupIntent {
    NewGroupIntent::new(kind, group_id, vec![1, 2, 3], true)
}

/// A database with one group and one freshly queued intent.
async fn with_intent(name: &str) -> (PgDb, StoredGroupIntent) {
    let db = fresh_db(name).await;
    make_group(&db, gid(1)).await;
    let intent = db
        .insert_group_intent(new_intent(IntentKind::SendMessage, gid(1)))
        .await
        .unwrap();
    (db, intent)
}

async fn reload(db: &PgDb, id: i32) -> StoredGroupIntent {
    db.find_group_intents(gid(1), None, None)
        .await
        .unwrap()
        .into_iter()
        .find(|intent| intent.id == id)
        .expect("intent still exists")
}

// --- insert and query -------------------------------------------------------

#[tokio::test]
async fn insert_returns_the_row_the_sequence_assigned() {
    let (_db, intent) = with_intent("i_insert").await;
    assert!(intent.id > 0, "id comes from the SERIAL");
    assert_eq!(intent.state, IntentState::ToPublish);
    assert_eq!(intent.kind, IntentKind::SendMessage);
    assert_eq!(intent.data, vec![1, 2, 3]);
    assert_eq!(intent.publish_attempts, 0, "column default");
    assert!(intent.payload_hash.is_none());
}

#[tokio::test]
async fn find_group_intents_filters_by_state_and_kind() {
    let db = fresh_db("i_find").await;
    make_group(&db, gid(1)).await;
    make_group(&db, gid(2)).await;

    let send = db
        .insert_group_intent(new_intent(IntentKind::SendMessage, gid(1)))
        .await
        .unwrap();
    let key_update = db
        .insert_group_intent(new_intent(IntentKind::KeyUpdate, gid(1)))
        .await
        .unwrap();
    db.insert_group_intent(new_intent(IntentKind::SendMessage, gid(2)))
        .await
        .unwrap();
    db.set_group_intent_error(key_update.id).await.unwrap();

    let ids = async |states: Option<Vec<IntentState>>, kinds: Option<Vec<IntentKind>>| {
        db.find_group_intents(gid(1), states, kinds)
            .await
            .unwrap()
            .into_iter()
            .map(|intent| intent.id)
            .collect::<Vec<_>>()
    };

    assert_eq!(
        ids(None, None).await,
        vec![send.id, key_update.id],
        "scoped to the group, ordered by id"
    );
    assert_eq!(
        ids(Some(vec![IntentState::Error]), None).await,
        vec![key_update.id]
    );
    assert_eq!(
        ids(None, Some(vec![IntentKind::SendMessage])).await,
        vec![send.id]
    );
    assert!(
        ids(
            Some(vec![IntentState::ToPublish]),
            Some(vec![IntentKind::KeyUpdate])
        )
        .await
        .is_empty(),
        "both filters apply"
    );
    assert!(
        ids(Some(vec![]), None).await.is_empty(),
        "an empty allow-list matches nothing"
    );
}

#[tokio::test]
async fn find_group_intent_by_payload_hash() {
    let (db, intent) = with_intent("i_by_hash").await;
    assert!(
        db.find_group_intent_by_payload_hash(&[9])
            .await
            .unwrap()
            .is_none(),
        "unpublished intents have no payload hash"
    );

    db.set_group_intent_published(intent.id, &[9], None, None, 3)
        .await
        .unwrap();
    let found = db
        .find_group_intent_by_payload_hash(&[9])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.id, intent.id);
}

// --- state machine ----------------------------------------------------------

#[tokio::test]
async fn publish_writes_the_payload_and_is_idempotent() {
    let (db, intent) = with_intent("i_publish").await;

    db.set_group_intent_published(intent.id, &[7, 7], Some(vec![1]), Some(vec![2]), 42)
        .await
        .unwrap();

    let stored = reload(&db, intent.id).await;
    assert_eq!(stored.state, IntentState::Published);
    assert_eq!(stored.payload_hash, Some(vec![7, 7]));
    assert_eq!(stored.post_commit_data, Some(vec![1]));
    assert_eq!(stored.staged_commit, Some(vec![2]));
    assert_eq!(stored.published_in_epoch, Some(42));

    // The transition only applies from ToPublish, but re-publishing an intent
    // that is already past it is a no-op rather than an error.
    db.set_group_intent_published(intent.id, &[8, 8], None, None, 99)
        .await
        .unwrap();
    let stored = reload(&db, intent.id).await;
    assert_eq!(
        stored.payload_hash,
        Some(vec![7, 7]),
        "the second publish changed nothing"
    );
    assert_eq!(stored.published_in_epoch, Some(42));
}

#[tokio::test]
async fn publishing_a_missing_intent_is_not_found() {
    let (db, _) = with_intent("i_publish_missing").await;
    let err = db
        .set_group_intent_published(9999, &[1], None, None, 1)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        StorageError::NotFound(NotFound::IntentForToPublish(9999))
    ));
}

#[tokio::test]
async fn commit_requires_the_intent_to_be_published() {
    let (db, intent) = with_intent("i_commit").await;

    // ToPublish -> Committed is not a legal transition.
    let err = db
        .set_group_intent_committed(intent.id, Cursor::new(5, 2u32))
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        StorageError::NotFound(NotFound::IntentForCommitted(_))
    ));

    db.set_group_intent_published(intent.id, &[1], None, None, 1)
        .await
        .unwrap();
    db.set_group_intent_committed(intent.id, Cursor::new(5, 2u32))
        .await
        .unwrap();

    let stored = reload(&db, intent.id).await;
    assert_eq!(stored.state, IntentState::Committed);
    assert_eq!(stored.sequence_id, Some(5));
    assert_eq!(stored.originator_id, Some(2));
}

/// Republishing clears everything the publish wrote, so the retry cannot be
/// matched against the payload hash of the attempt that failed.
#[tokio::test]
async fn to_publish_rewinds_and_clears_the_published_fields() {
    let (db, intent) = with_intent("i_republish").await;

    let err = db.set_group_intent_to_publish(intent.id).await.unwrap_err();
    assert!(
        matches!(err, StorageError::NotFound(NotFound::IntentForPublish(_))),
        "only a Published intent can rewind"
    );

    db.set_group_intent_published(intent.id, &[1], Some(vec![2]), Some(vec![3]), 7)
        .await
        .unwrap();
    db.set_group_intent_to_publish(intent.id).await.unwrap();

    let stored = reload(&db, intent.id).await;
    assert_eq!(stored.state, IntentState::ToPublish);
    assert_eq!(stored.payload_hash, None);
    assert_eq!(stored.post_commit_data, None);
    assert_eq!(stored.staged_commit, None);
    assert_eq!(stored.published_in_epoch, None);
}

/// `Processed` and `Error` are the two unguarded transitions: any state may
/// reach them, so a miss can only mean the intent is gone.
#[tokio::test]
async fn processed_and_error_apply_from_any_state() {
    let (db, intent) = with_intent("i_terminal").await;

    db.set_group_intent_error(intent.id).await.unwrap();
    assert_eq!(reload(&db, intent.id).await.state, IntentState::Error);

    db.set_group_intent_processed(intent.id).await.unwrap();
    assert_eq!(reload(&db, intent.id).await.state, IntentState::Processed);

    for result in [
        db.set_group_intent_error(9999).await,
        db.set_group_intent_processed(9999).await,
    ] {
        assert!(matches!(
            result.unwrap_err(),
            StorageError::NotFound(NotFound::IntentById(9999))
        ));
    }
}

#[tokio::test]
async fn publish_attempts_increment_in_place() {
    let (db, intent) = with_intent("i_attempts").await;
    db.increment_intent_publish_attempt_count(intent.id)
        .await
        .unwrap();
    db.increment_intent_publish_attempt_count(intent.id)
        .await
        .unwrap();
    assert_eq!(reload(&db, intent.id).await.publish_attempts, 2);

    // A missing intent is not an error here, matching the sync path.
    db.increment_intent_publish_attempt_count(9999)
        .await
        .unwrap();
}

/// Both writes land together: an intent marked failed while its message still
/// looks publishable would be retried forever.
#[tokio::test]
async fn error_and_fail_msg_updates_the_intent_and_its_message() {
    let (db, intent) = with_intent("i_fail_msg").await;

    let message = StoredGroupMessage {
        id: vec![1],
        group_id: gid(1),
        decrypted_message_bytes: vec![],
        sent_at_ns: 10,
        kind: GroupMessageKind::Application,
        sender_installation_id: vec![1],
        sender_inbox_id: "sender".to_string(),
        delivery_status: DeliveryStatus::Unpublished,
        content_type: ContentType::Text,
        version_major: 1,
        version_minor: 0,
        authority_id: "xmtp.org".to_string(),
        reference_id: None,
        originator_id: 1,
        sequence_id: 1,
        inserted_at_ns: 10,
        expire_at_ns: None,
        should_push: true,
        idempotency_key: String::new(),
    };
    let mut c = db.conn().await.unwrap();
    sqlx::query(
        "INSERT INTO group_messages \
         (id, group_id, decrypted_message_bytes, sent_at_ns, kind, sender_installation_id, \
          sender_inbox_id, delivery_status, content_type, version_major, version_minor, \
          authority_id, originator_id, sequence_id, inserted_at_ns, should_push, idempotency_key) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)",
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
    .bind(message.originator_id)
    .bind(message.sequence_id)
    .bind(message.inserted_at_ns)
    .bind(message.should_push)
    .bind(&message.idempotency_key)
    .execute(&mut *c)
    .await
    .unwrap();
    drop(c);

    db.set_group_intent_error_and_fail_msg(&intent, Some(vec![1]))
        .await
        .unwrap();

    assert_eq!(reload(&db, intent.id).await.state, IntentState::Error);
    let stored = db.get_group_message([1u8]).await.unwrap().unwrap();
    assert_eq!(stored.delivery_status, DeliveryStatus::Failed);
}

// --- find_dependant_commits -------------------------------------------------

#[tokio::test]
async fn dependant_commits_join_the_commit_message_refresh_state() {
    let db = fresh_db("i_deps").await;
    make_group(&db, gid(1)).await;
    make_group(&db, gid(2)).await;

    let first = db
        .insert_group_intent(new_intent(IntentKind::SendMessage, gid(1)))
        .await
        .unwrap();
    let second = db
        .insert_group_intent(new_intent(IntentKind::SendMessage, gid(2)))
        .await
        .unwrap();
    db.set_group_intent_published(first.id, &[0xaa], None, None, 1)
        .await
        .unwrap();
    db.set_group_intent_published(second.id, &[0xbb], None, None, 1)
        .await
        .unwrap();

    db.update_cursor(gid(1), EntityKind::CommitMessage, Cursor::new(11, 3u32))
        .await
        .unwrap();
    // A different entity kind for the same group must not be picked up.
    db.update_cursor(
        gid(1),
        EntityKind::ApplicationMessage,
        Cursor::new(99, 3u32),
    )
    .await
    .unwrap();

    let deps = db
        .find_dependant_commits(&[&[0xaau8][..], &[0xbbu8][..], &[0xccu8][..]])
        .await
        .unwrap();

    assert_eq!(deps.len(), 1, "only the group with a commit cursor");
    let dependency = &deps[&vec![0xaau8].into()];
    assert_eq!(dependency.cursor, Cursor::new(11, 3u32));
    assert_eq!(dependency.group_id, gid(1));
}

#[tokio::test]
async fn dependant_commits_reject_a_hash_that_spans_two_cursors() {
    let db = fresh_db("i_deps_dup").await;
    make_group(&db, gid(1)).await;
    let intent = db
        .insert_group_intent(new_intent(IntentKind::SendMessage, gid(1)))
        .await
        .unwrap();
    db.set_group_intent_published(intent.id, &[0xaa], None, None, 1)
        .await
        .unwrap();

    // `refresh_state` is keyed by (entity, kind, originator), so one group can
    // carry a commit-message cursor per originator.
    db.update_cursor(gid(1), EntityKind::CommitMessage, Cursor::new(11, 3u32))
        .await
        .unwrap();
    db.update_cursor(gid(1), EntityKind::CommitMessage, Cursor::new(12, 4u32))
        .await
        .unwrap();

    assert!(
        db.find_dependant_commits(&[&[0xaau8][..]]).await.is_err(),
        "an ambiguous dependency is an error, not an arbitrary pick"
    );
}

#[tokio::test]
async fn dependant_commits_of_nothing_is_empty() {
    let db = fresh_db("i_deps_empty").await;
    let empty: &[&[u8]] = &[];
    assert!(db.find_dependant_commits(empty).await.unwrap().is_empty());
}
