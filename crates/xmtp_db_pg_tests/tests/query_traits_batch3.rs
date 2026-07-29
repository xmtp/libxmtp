//! sqlx `Query*` impls: tasks, readd status, DM stitching, device-sync messages.

use std::collections::HashSet;
use xmtp_db::group::{ConversationType, GroupMembershipState, QueryDms};
use xmtp_db::pg::PgDb;
use xmtp_db::processed_device_sync_messages::QueryDeviceSyncMessages;
use xmtp_db::readd_status::QueryReaddStatus;
use xmtp_db::tasks::{NewTask, QueryTasks, TaskDataHash};
use xmtp_db_pg_tests::fresh_db;
use xmtp_proto::types::GroupId;
use xmtp_proto::xmtp::mls::database::{
    KpRotation, ProcessPendingSelfRemove, Task as TaskProto, task::Task as TaskKind,
};

// --- tasks ------------------------------------------------------------------

fn task_with(kind: TaskKind, expires_at_ns: i64, max_attempts: i32) -> NewTask {
    NewTask::builder()
        .originating_message_sequence_id(0)
        .originating_message_originator_id(0)
        .expires_at_ns(expires_at_ns)
        .max_attempts(max_attempts)
        .next_attempt_at_ns(0)
        .build(TaskProto { task: Some(kind) })
        .unwrap()
}

fn kp_rotation() -> NewTask {
    task_with(
        TaskKind::KpRotation(KpRotation {}),
        xmtp_db::tasks::NEVER_EXPIRES,
        i32::MAX,
    )
}

fn self_remove(group_id: &GroupId, expires_at_ns: i64, max_attempts: i32) -> NewTask {
    task_with(
        TaskKind::ProcessPendingSelfRemove(ProcessPendingSelfRemove {
            group_id: group_id.to_vec(),
        }),
        expires_at_ns,
        max_attempts,
    )
}

#[tokio::test]
async fn task_create_returns_the_inserted_row() {
    let db = fresh_db("tk_create").await;
    let created = db.create_task(kp_rotation()).await.unwrap();
    assert!(created.id > 0, "id is assigned by the sequence");
    assert_eq!(created.attempts, 0);
    assert_eq!(db.get_tasks().await.unwrap().len(), 1);
}

/// `data_hash` is UNIQUE, so a payload-identical enqueue coalesces instead of
/// piling up — that is what makes repeated nudges safe.
#[tokio::test]
async fn task_create_or_ignore_dedups_by_payload() {
    let db = fresh_db("tk_ignore").await;
    db.create_or_ignore_task(kp_rotation()).await.unwrap();
    db.create_or_ignore_task(kp_rotation()).await.unwrap();
    assert_eq!(db.get_tasks().await.unwrap().len(), 1);
}

#[tokio::test]
async fn next_task_is_the_soonest_deadline() {
    let db = fresh_db("tk_next").await;
    assert!(db.get_next_task().await.unwrap().is_none());

    let mut later = kp_rotation();
    later.next_attempt_at_ns = 5_000;
    let later = db.create_task(later).await.unwrap();

    let mut sooner = self_remove(&GroupId::ONE, i64::MAX, 5);
    sooner.next_attempt_at_ns = 100;
    let sooner = db.create_task(sooner).await.unwrap();

    let next = db.get_next_task().await.unwrap().unwrap();
    assert_eq!(next.id, sooner.id, "ordered by next_attempt_at_ns");
    assert_ne!(next.id, later.id);
}

/// `LEAST` is the Postgres scalar equivalent of SQLite's two-arg `MIN`; the
/// deadline only ever moves earlier.
#[tokio::test]
async fn pull_in_deadline_only_lowers() {
    let db = fresh_db("tk_pullin").await;
    let mut t = kp_rotation();
    t.next_attempt_at_ns = 1_000;
    let created = db.create_task(t).await.unwrap();
    let hash = TaskDataHash::try_from(created.data_hash.as_slice()).unwrap();

    assert!(db.pull_in_task_deadline(&hash, 500).await.unwrap());
    assert_eq!(
        db.get_next_task()
            .await
            .unwrap()
            .unwrap()
            .next_attempt_at_ns,
        500
    );

    assert!(
        db.pull_in_task_deadline(&hash, 900).await.unwrap(),
        "a row still matched, even though the value did not move"
    );
    assert_eq!(
        db.get_next_task()
            .await
            .unwrap()
            .unwrap()
            .next_attempt_at_ns,
        500,
        "a later deadline must never raise the stored one"
    );

    let missing = TaskDataHash::try_from([0xAAu8; 32].as_slice()).unwrap();
    assert!(
        !db.pull_in_task_deadline(&missing, 1).await.unwrap(),
        "no target is a no-op reported as false"
    );
}

#[tokio::test]
async fn task_update_and_delete() {
    let db = fresh_db("tk_update").await;
    let created = db.create_task(kp_rotation()).await.unwrap();

    let updated = db.update_task(created.id, 3, 111, 222).await.unwrap();
    assert_eq!(updated.attempts, 3);
    assert_eq!(updated.last_attempted_at_ns, 111);
    assert_eq!(updated.next_attempt_at_ns, 222);
    assert_eq!(updated.id, created.id);

    assert!(
        db.update_task(999_999, 1, 1, 1).await.is_err(),
        "updating a missing task errors, matching the sync track"
    );

    assert!(db.delete_task(created.id).await.unwrap());
    assert!(
        !db.delete_task(created.id).await.unwrap(),
        "deleting twice reports false"
    );
    assert!(db.get_tasks().await.unwrap().is_empty());
}

/// A *live* self-remove task must survive: deleting it would reset the
/// TaskRunner's backoff and could race the worker onto a deleted id.
#[tokio::test]
async fn self_remove_upsert_keeps_live_rows_and_clears_dead_ones() {
    let db = fresh_db("tk_selfremove").await;
    let group = GroupId::ONE;

    // A live task (far-future expiry, attempts below max).
    db.create_task(self_remove(&group, i64::MAX, 5))
        .await
        .unwrap();
    let live = db.get_tasks().await.unwrap();
    assert_eq!(live.len(), 1);
    let live_id = live[0].id;

    db.upsert_pending_self_remove_task(&group, self_remove(&group, i64::MAX, 5))
        .await
        .unwrap();
    let after = db.get_tasks().await.unwrap();
    assert_eq!(after.len(), 1, "the duplicate is deduped by data_hash");
    assert_eq!(after[0].id, live_id, "the live row is left untouched");

    // Exhaust its attempts: it is now dead and must be replaced.
    db.update_task(live_id, 5, 0, 0).await.unwrap();
    db.upsert_pending_self_remove_task(&group, self_remove(&group, i64::MAX, 5))
        .await
        .unwrap();
    let after = db.get_tasks().await.unwrap();
    assert_eq!(after.len(), 1);
    assert_ne!(
        after[0].id, live_id,
        "the dead row is cleared so a fresh retry can take its hash"
    );
    assert_eq!(after[0].attempts, 0, "the replacement starts clean");
}

/// Dead rows for *other* groups, and non-self-remove tasks, are not touched.
#[tokio::test]
async fn self_remove_upsert_is_scoped_to_its_group_and_kind() {
    let db = fresh_db("tk_scope").await;

    // Dead, but a different kind.
    let mut dead_rotation = kp_rotation();
    dead_rotation.expires_at_ns = 1;
    db.create_task(dead_rotation).await.unwrap();
    // Dead self-remove, but a different group.
    db.create_task(self_remove(&GroupId::TWO, 1, 5))
        .await
        .unwrap();

    db.upsert_pending_self_remove_task(&GroupId::ONE, self_remove(&GroupId::ONE, i64::MAX, 5))
        .await
        .unwrap();

    assert_eq!(
        db.get_tasks().await.unwrap().len(),
        3,
        "neither the other kind nor the other group's row may be cleared"
    );
}

// --- readd_status -----------------------------------------------------------

const INST_A: &[u8] = b"install-a";
const INST_B: &[u8] = b"install-b";

#[tokio::test]
async fn readd_sequence_ids_only_advance() {
    let db = fresh_db("rd_monotonic").await;
    let group = GroupId::ONE;

    db.update_requested_at_sequence_id(&group, INST_A, 10)
        .await
        .unwrap();
    db.update_requested_at_sequence_id(&group, INST_A, 5)
        .await
        .unwrap();
    let status = db.get_readd_status(&group, INST_A).await.unwrap().unwrap();
    assert_eq!(
        status.requested_at_sequence_id,
        Some(10),
        "a lower request id must not overwrite a higher one"
    );
    assert_eq!(
        status.responded_at_sequence_id, None,
        "the upsert must not clobber the other column"
    );

    db.update_responded_at_sequence_id(&group, INST_A, 20)
        .await
        .unwrap();
    let status = db.get_readd_status(&group, INST_A).await.unwrap().unwrap();
    assert_eq!(status.requested_at_sequence_id, Some(10));
    assert_eq!(status.responded_at_sequence_id, Some(20));
}

#[tokio::test]
async fn awaiting_readd_compares_request_against_response() {
    let db = fresh_db("rd_awaiting").await;
    let group = GroupId::ONE;

    assert!(
        !db.is_awaiting_readd(&group, INST_A).await.unwrap(),
        "no row means nothing is awaited"
    );

    db.update_requested_at_sequence_id(&group, INST_A, 10)
        .await
        .unwrap();
    assert!(db.is_awaiting_readd(&group, INST_A).await.unwrap());

    db.update_responded_at_sequence_id(&group, INST_A, 20)
        .await
        .unwrap();
    assert!(
        !db.is_awaiting_readd(&group, INST_A).await.unwrap(),
        "a response newer than the request settles it"
    );

    db.update_requested_at_sequence_id(&group, INST_A, 30)
        .await
        .unwrap();
    assert!(
        db.is_awaiting_readd(&group, INST_A).await.unwrap(),
        "a newer request re-opens it"
    );
}

#[tokio::test]
async fn readds_awaiting_response_excludes_self_and_answered() {
    let db = fresh_db("rd_pending").await;
    let group = GroupId::ONE;

    db.update_requested_at_sequence_id(&group, INST_A, 10)
        .await
        .unwrap();
    db.update_requested_at_sequence_id(&group, INST_B, 10)
        .await
        .unwrap();
    db.update_responded_at_sequence_id(&group, INST_B, 99)
        .await
        .unwrap();

    let pending = db
        .get_readds_awaiting_response(&group, INST_A)
        .await
        .unwrap();
    assert!(
        pending.is_empty(),
        "self is excluded and B has already responded"
    );

    let pending = db
        .get_readds_awaiting_response(&group, INST_B)
        .await
        .unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].installation_id, INST_A.to_vec());
}

#[tokio::test]
async fn readd_deletion_is_scoped() {
    let db = fresh_db("rd_delete").await;
    let group = GroupId::ONE;
    db.update_requested_at_sequence_id(&group, INST_A, 1)
        .await
        .unwrap();
    db.update_requested_at_sequence_id(&group, INST_B, 1)
        .await
        .unwrap();
    db.update_requested_at_sequence_id(&GroupId::TWO, INST_A, 1)
        .await
        .unwrap();

    db.delete_other_readd_statuses(&group, INST_A)
        .await
        .unwrap();
    assert!(db.get_readd_status(&group, INST_A).await.unwrap().is_some());
    assert!(db.get_readd_status(&group, INST_B).await.unwrap().is_none());
    assert!(
        db.get_readd_status(&GroupId::TWO, INST_A)
            .await
            .unwrap()
            .is_some(),
        "another group's rows are untouched"
    );

    db.delete_readd_statuses(&group, HashSet::from([INST_A.to_vec()]))
        .await
        .unwrap();
    assert!(db.get_readd_status(&group, INST_A).await.unwrap().is_none());

    // An empty set deletes nothing rather than everything.
    db.delete_readd_statuses(&GroupId::TWO, HashSet::new())
        .await
        .unwrap();
    assert!(
        db.get_readd_status(&GroupId::TWO, INST_A)
            .await
            .unwrap()
            .is_some()
    );
}

// --- DM stitching -----------------------------------------------------------

async fn insert_group(
    db: &PgDb,
    id: &GroupId,
    dm_id: Option<&str>,
    last_message_ns: Option<i64>,
    state: GroupMembershipState,
) {
    let mut c = db.conn().await.unwrap();
    sqlx::query(
        "INSERT INTO groups (id, created_at_ns, membership_state, installations_last_checked, \
         added_by_inbox_id, dm_id, last_message_ns, conversation_type) \
         VALUES ($1, 0, $2, 0, '', $3, $4, $5)",
    )
    .bind(id)
    .bind(state)
    .bind(dm_id)
    .bind(last_message_ns)
    .bind(if dm_id.is_some() {
        ConversationType::Dm
    } else {
        ConversationType::Group
    })
    .execute(&mut *c)
    .await
    .unwrap();
}

#[tokio::test]
async fn fetch_stitched_returns_the_most_recent_dm() {
    let db = fresh_db("dm_stitch").await;
    let old = GroupId::ONE;
    let new = GroupId::TWO;
    insert_group(
        &db,
        &old,
        Some("dm:a:b"),
        Some(100),
        GroupMembershipState::Allowed,
    )
    .await;
    insert_group(
        &db,
        &new,
        Some("dm:a:b"),
        Some(200),
        GroupMembershipState::Allowed,
    )
    .await;

    // Asking for either one yields the most recently active of the pair.
    for asked in [&old, &new] {
        let got = db.fetch_stitched(asked).await.unwrap().unwrap();
        assert_eq!(got.id, new, "stitching resolves to the latest DM");
    }
}

/// A never-used DM has a NULL `last_message_ns`. Postgres sorts NULLs *first* on
/// DESC, so without `NULLS LAST` it would beat a DM that actually has messages.
#[tokio::test]
async fn fetch_stitched_prefers_a_dm_with_messages_over_a_null_one() {
    let db = fresh_db("dm_nulls").await;
    let used = GroupId::ONE;
    let unused = GroupId::TWO;
    insert_group(
        &db,
        &used,
        Some("dm:a:b"),
        Some(100),
        GroupMembershipState::Allowed,
    )
    .await;
    insert_group(
        &db,
        &unused,
        Some("dm:a:b"),
        None,
        GroupMembershipState::Allowed,
    )
    .await;

    let got = db.fetch_stitched(&used).await.unwrap().unwrap();
    assert_eq!(got.id, used, "NULL last_message_ns must sort last");
}

#[tokio::test]
async fn fetch_stitched_on_a_non_dm_returns_itself() {
    let db = fresh_db("dm_nondm").await;
    let plain = GroupId::ONE;
    insert_group(&db, &plain, None, Some(1), GroupMembershipState::Allowed).await;
    // Another non-DM group must not be confused for a stitch partner.
    insert_group(
        &db,
        &GroupId::TWO,
        None,
        Some(999),
        GroupMembershipState::Allowed,
    )
    .await;

    let got = db.fetch_stitched(&plain).await.unwrap().unwrap();
    assert_eq!(got.id, plain);

    assert!(
        db.fetch_stitched(&GroupId::THREE).await.unwrap().is_none(),
        "an unknown group is Ok(None)"
    );
}

#[tokio::test]
async fn other_dms_and_active_dm_lookup() {
    let db = fresh_db("dm_other").await;
    let a = GroupId::ONE;
    let b = GroupId::TWO;
    insert_group(
        &db,
        &a,
        Some("dm:a:b"),
        Some(100),
        GroupMembershipState::Allowed,
    )
    .await;
    insert_group(
        &db,
        &b,
        Some("dm:a:b"),
        Some(200),
        GroupMembershipState::Allowed,
    )
    .await;

    let others = db.other_dms(&a).await.unwrap();
    assert_eq!(others.len(), 1);
    assert_eq!(others[0].id, b, "excludes the group asked about");

    let plain = GroupId::THREE;
    insert_group(&db, &plain, None, Some(1), GroupMembershipState::Allowed).await;
    assert!(
        db.other_dms(&plain).await.unwrap().is_empty(),
        "a non-DM has no stitch partners"
    );

    let active = db.find_active_dm_group("dm:a:b").await.unwrap().unwrap();
    assert_eq!(active.id, b, "most recently active wins");
    assert!(
        db.find_active_dm_group("dm:nobody")
            .await
            .unwrap()
            .is_none()
    );
}

/// `Restored` groups are excluded from the active-DM lookup.
#[tokio::test]
async fn find_active_dm_skips_restored_groups() {
    let db = fresh_db("dm_restored").await;
    insert_group(
        &db,
        &GroupId::ONE,
        Some("dm:a:b"),
        Some(500),
        GroupMembershipState::Restored,
    )
    .await;
    insert_group(
        &db,
        &GroupId::TWO,
        Some("dm:a:b"),
        Some(100),
        GroupMembershipState::Allowed,
    )
    .await;

    let active = db.find_active_dm_group("dm:a:b").await.unwrap().unwrap();
    assert_eq!(
        active.id,
        GroupId::TWO,
        "the newer Restored group must be skipped"
    );
}

// --- device sync messages ---------------------------------------------------

/// `delivery_status` is set explicitly on purpose. The column is
/// `INTEGER NOT NULL DEFAULT 0` in *both* schemas, but `DeliveryStatus` has no
/// variant 0 (Unpublished = 1), so a row that takes the default cannot be
/// decoded. libxmtp's models always write the column, so the default is
/// unreachable in practice — but a raw INSERT that omits it produces a row no
/// reader can load. Pre-existing on both tracks, not a Postgres artifact.
async fn insert_sync_message(db: &PgDb, group_id: &GroupId, id: &[u8], sent_at_ns: i64) {
    let mut c = db.conn().await.unwrap();
    sqlx::query(
        "INSERT INTO group_messages (id, group_id, decrypted_message_bytes, sent_at_ns, \
         sender_installation_id, sender_inbox_id, authority_id, originator_id, sequence_id, \
         idempotency_key, delivery_status) VALUES ($1, $2, '\\x00', $3, '\\x00', '', '', 0, 0, '', $4)",
    )
    .bind(id)
    .bind(group_id)
    .bind(sent_at_ns)
    .bind(xmtp_db::group_message::DeliveryStatus::Published)
    .execute(&mut *c)
    .await
    .unwrap();
}

#[tokio::test]
async fn unprocessed_sync_messages_excludes_processed_and_non_sync_groups() {
    let db = fresh_db("ds_unprocessed").await;
    let sync_group = GroupId::ONE;
    let normal_group = GroupId::TWO;

    let mut c = db.conn().await.unwrap();
    sqlx::query(
        "INSERT INTO groups (id, created_at_ns, membership_state, installations_last_checked, \
         added_by_inbox_id, conversation_type) VALUES ($1, 0, 1, 0, '', $2)",
    )
    .bind(&sync_group)
    .bind(ConversationType::Sync)
    .execute(&mut *c)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO groups (id, created_at_ns, membership_state, installations_last_checked, \
         added_by_inbox_id, conversation_type) VALUES ($1, 0, 1, 0, '', $2)",
    )
    .bind(&normal_group)
    .bind(ConversationType::Group)
    .execute(&mut *c)
    .await
    .unwrap();
    drop(c);

    insert_sync_message(&db, &sync_group, b"m1", 10).await;
    insert_sync_message(&db, &sync_group, b"m2", 20).await;
    insert_sync_message(&db, &normal_group, b"m3", 30).await;

    let unprocessed = db.unprocessed_sync_group_messages().await.unwrap();
    assert_eq!(
        unprocessed.len(),
        2,
        "only sync-group messages, and none are processed yet"
    );

    db.mark_device_sync_msg_as_processed(b"m1").await.unwrap();
    let unprocessed = db.unprocessed_sync_group_messages().await.unwrap();
    assert_eq!(unprocessed.len(), 1);
    assert_eq!(unprocessed[0].id, b"m2".to_vec());

    let paged = db.sync_group_messages_paged(0, 1).await.unwrap();
    assert_eq!(paged.len(), 1);
    assert_eq!(paged[0].id, b"m2".to_vec(), "ordered by sent_at_ns DESC");
}

#[tokio::test]
async fn incrementing_attempts_fails_the_message_at_the_limit() {
    let db = fresh_db("ds_attempts").await;

    assert_eq!(
        db.increment_device_sync_msg_attempt(b"m1", 3)
            .await
            .unwrap(),
        1,
        "first attempt inserts with a count of 1"
    );
    assert_eq!(
        db.increment_device_sync_msg_attempt(b"m1", 3)
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        db.increment_device_sync_msg_attempt(b"m1", 3)
            .await
            .unwrap(),
        3
    );

    use sqlx::Row;
    let mut c = db.conn().await.unwrap();
    let state: i32 =
        sqlx::query("SELECT state FROM processed_device_sync_messages WHERE message_id = $1")
            .bind(b"m1".to_vec())
            .fetch_one(&mut *c)
            .await
            .unwrap()
            .try_get(0)
            .unwrap();
    assert_eq!(state, 2, "reaching max_attempts flips the state to Failed");
}
