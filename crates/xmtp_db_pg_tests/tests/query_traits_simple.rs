//! sqlx `Query*` impls whose SQL is straightforward, exercised against the real
//! committed schema.
//!
//! The Postgres backend has no `diesel::table!` definitions, so a column rename or a
//! type mismatch cannot fail the build — these tests are the only thing standing
//! between a typo and a runtime error in production. Every method gets at least
//! one call that actually reaches Postgres.

use xmtp_db::association_state::QueryAssociationStateCache;
use xmtp_db::d14n_migration_cutover::QueryMigrationCutover;
use xmtp_db::identity::QueryIdentity;
use xmtp_db::key_store_entry::QueryKeyStoreEntry;
use xmtp_db::pending_remove::QueryPendingRemove;
use xmtp_db::pg::PgDb;
use xmtp_db_pg_tests::fresh_db;
use xmtp_proto::types::GroupId;

// --- pending_remove ---------------------------------------------------------

async fn add_pending_remove(db: &PgDb, group_id: &GroupId, inbox_id: &str) {
    let mut c = db.conn().await.unwrap();
    sqlx::query("INSERT INTO pending_remove (group_id, inbox_id, message_id) VALUES ($1, $2, $3)")
        .bind(group_id)
        .bind(inbox_id)
        .bind(vec![1u8, 2, 3])
        .execute(&mut *c)
        .await
        .unwrap();
}

#[tokio::test]
async fn pending_remove_scopes_by_group() {
    let db = fresh_db("pr_scope").await;
    add_pending_remove(&db, &GroupId::ONE, "a").await;
    add_pending_remove(&db, &GroupId::ONE, "b").await;
    add_pending_remove(&db, &GroupId::TWO, "c").await;

    let mut users = db.get_pending_remove_users(&GroupId::ONE).await.unwrap();
    users.sort();
    assert_eq!(users, vec!["a".to_string(), "b".to_string()]);

    assert!(
        db.get_user_pending_remove_status(&GroupId::ONE, "a")
            .await
            .unwrap()
    );
    assert!(
        !db.get_user_pending_remove_status(&GroupId::TWO, "a")
            .await
            .unwrap(),
        "status must be scoped to the group, not just the inbox id"
    );
}

#[tokio::test]
async fn pending_remove_delete_returns_rows_affected() {
    let db = fresh_db("pr_delete").await;
    add_pending_remove(&db, &GroupId::ONE, "1").await;
    add_pending_remove(&db, &GroupId::ONE, "2").await;
    add_pending_remove(&db, &GroupId::ONE, "3").await;

    let deleted = db
        .delete_pending_remove_users(&GroupId::ONE, vec!["1".into(), "2".into()])
        .await
        .unwrap();
    assert_eq!(deleted, 2);
    assert_eq!(
        db.get_pending_remove_users(&GroupId::ONE)
            .await
            .unwrap()
            .len(),
        1
    );

    // Right inbox id, wrong group: matches nothing.
    let deleted = db
        .delete_pending_remove_users(&GroupId::TWO, vec!["3".into()])
        .await
        .unwrap();
    assert_eq!(deleted, 0);
}

/// An empty id list must delete nothing. `= ANY('{}')` matches no rows, but a
/// hand-built `IN ()` would be a syntax error — worth pinning.
#[tokio::test]
async fn pending_remove_delete_with_no_ids_is_a_noop() {
    let db = fresh_db("pr_empty").await;
    add_pending_remove(&db, &GroupId::ONE, "1").await;

    let deleted = db
        .delete_pending_remove_users(&GroupId::ONE, vec![])
        .await
        .unwrap();
    assert_eq!(deleted, 0);
    assert_eq!(
        db.get_pending_remove_users(&GroupId::ONE)
            .await
            .unwrap()
            .len(),
        1
    );
}

// --- association_state ------------------------------------------------------

fn proto(inbox_id: &str) -> xmtp_proto::xmtp::identity::associations::AssociationState {
    xmtp_proto::xmtp::identity::associations::AssociationState {
        inbox_id: inbox_id.to_string(),
        ..Default::default()
    }
}

#[tokio::test]
async fn association_state_write_then_read() {
    let db = fresh_db("as_rw").await;
    db.write_to_cache("inbox_a".into(), 1, proto("inbox_a"))
        .await
        .unwrap();

    let got = db.read_from_cache("inbox_a", 1).await.unwrap();
    assert_eq!(got.map(|s| s.inbox_id), Some("inbox_a".to_string()));

    assert!(
        db.read_from_cache("inbox_a", 2).await.unwrap().is_none(),
        "a different sequence_id is a different cache entry"
    );
}

/// The SQLite backend uses `INSERT OR IGNORE`; the first write must win.
#[tokio::test]
async fn association_state_write_does_not_overwrite() {
    let db = fresh_db("as_ignore").await;
    db.write_to_cache("first".into(), 1, proto("first"))
        .await
        .unwrap();
    db.write_to_cache("first".into(), 1, proto("second"))
        .await
        .unwrap();

    let got = db.read_from_cache("first", 1).await.unwrap().unwrap();
    assert_eq!(
        got.inbox_id, "first",
        "ON CONFLICT DO NOTHING must leave the original row intact"
    );
}

#[tokio::test]
async fn association_state_batch_read_matches_pairs_not_columns() {
    let db = fresh_db("as_batch").await;
    db.write_to_cache("a".into(), 1, proto("a1")).await.unwrap();
    db.write_to_cache("b".into(), 2, proto("b2")).await.unwrap();

    let both = db
        .batch_read_from_cache(vec![("a".into(), 1), ("b".into(), 2)])
        .await
        .unwrap();
    assert_eq!(both.len(), 2);

    // ("a", 2) exists as neither a row nor a pair, even though both "a" and 2
    // appear in the table. A query that filtered the two columns independently
    // would wrongly return rows here.
    let none = db
        .batch_read_from_cache(vec![("a".into(), 2)])
        .await
        .unwrap();
    assert!(none.is_empty(), "must match on the pair, not column-wise");

    let empty = db.batch_read_from_cache(vec![]).await.unwrap();
    assert!(empty.is_empty(), "no identifiers must not load the table");
}

// --- identity ---------------------------------------------------------------

async fn insert_identity(db: &PgDb, rotation_ns: Option<i64>) {
    let mut c = db.conn().await.unwrap();
    sqlx::query(
        "INSERT INTO identity (inbox_id, installation_keys, credential_bytes, rowid, \
         next_key_package_rotation_ns) VALUES ('inbox', '\\x00', '\\x00', 1, $1)",
    )
    .bind(rotation_ns)
    .execute(&mut *c)
    .await
    .unwrap();
}

#[tokio::test]
async fn identity_rotation_is_a_noop_before_registration() {
    let db = fresh_db("id_prereg").await;
    db.queue_key_package_rotation().await.unwrap();
    assert_eq!(db.next_key_package_rotation_ns().await.unwrap(), None);
    assert!(
        !db.is_identity_needs_rotation().await.unwrap(),
        "no identity row means nothing to rotate"
    );
}

/// A NULL column on an existing row means "unscheduled": queueing initializes it
/// rather than skipping the row, and a later queue never raises the deadline.
#[tokio::test]
async fn identity_queue_initializes_null_then_only_lowers() {
    let db = fresh_db("id_queue").await;
    insert_identity(&db, None).await;

    assert!(
        db.is_identity_needs_rotation().await.unwrap(),
        "NULL column means rotation is due now"
    );

    db.queue_key_package_rotation().await.unwrap();
    let first = db
        .next_key_package_rotation_ns()
        .await
        .unwrap()
        .expect("NULL must be initialized");

    db.queue_key_package_rotation().await.unwrap();
    assert_eq!(
        db.next_key_package_rotation_ns().await.unwrap().unwrap(),
        first,
        "queueing must lower the deadline, never raise it"
    );
}

#[tokio::test]
async fn identity_reset_only_touches_due_rows() {
    let db = fresh_db("id_reset").await;
    let far_future = xmtp_common::time::now_ns() + 1_000_000_000_000;
    insert_identity(&db, Some(far_future)).await;

    db.reset_key_package_rotation_queue(60_000_000_000)
        .await
        .unwrap();
    assert_eq!(
        db.next_key_package_rotation_ns().await.unwrap(),
        Some(far_future),
        "a deadline still in the future must not be reset"
    );
}

/// A stand-in rotation seed (the real payload lives in xmtp_mls).
fn rotation_seed() -> xmtp_db::tasks::NewTask {
    use xmtp_proto::xmtp::mls::database::{KpRotation, Task as TaskProto, task::Task};
    xmtp_db::tasks::NewTask::builder()
        .originating_message_sequence_id(0)
        .originating_message_originator_id(0)
        .expires_at_ns(xmtp_db::tasks::NEVER_EXPIRES)
        .max_attempts(i32::MAX)
        .next_attempt_at_ns(0)
        .build(TaskProto {
            task: Some(Task::KpRotation(KpRotation {})),
        })
        .unwrap()
}

async fn task_count(db: &PgDb) -> i64 {
    use sqlx::Row;
    let mut c = db.conn().await.unwrap();
    sqlx::query("SELECT COUNT(*) FROM tasks")
        .fetch_one(&mut *c)
        .await
        .unwrap()
        .try_get(0)
        .unwrap()
}

/// Pre-registration must write *nothing* — not even the seed. Enqueueing a
/// pull-in with no identity row would leave a nudge whose target never arrives.
#[tokio::test]
async fn nudge_writes_nothing_before_registration() {
    let db = fresh_db("id_nudge_prereg").await;
    let hash = xmtp_db::tasks::TaskDataHash::try_from([0x11u8; 32].as_slice()).unwrap();

    db.queue_key_rotation_with_nudge(&hash, rotation_seed())
        .await
        .unwrap();

    assert_eq!(
        task_count(&db).await,
        0,
        "no pull-in without an identity row"
    );
}

/// With an identity row the nudge self-heals a missing seed: it inserts both the
/// pull-in's target and the pull-in itself, atomically.
#[tokio::test]
async fn nudge_selfheals_missing_seed() {
    let db = fresh_db("id_nudge_seed").await;
    insert_identity(&db, None).await;

    let seed = rotation_seed();
    let hash = xmtp_db::tasks::TaskDataHash::try_from(seed.data_hash.as_slice()).unwrap();
    db.queue_key_rotation_with_nudge(&hash, seed).await.unwrap();

    assert_eq!(task_count(&db).await, 2, "seed + pull-in");

    // Repeat calls must coalesce rather than pile up: the deadline column is
    // stable between rotations, so the pull-in payload is byte-identical and
    // collides on `data_hash`.
    let seed = rotation_seed();
    let hash = xmtp_db::tasks::TaskDataHash::try_from(seed.data_hash.as_slice()).unwrap();
    db.queue_key_rotation_with_nudge(&hash, seed).await.unwrap();
    assert_eq!(task_count(&db).await, 2, "a repeat nudge must coalesce");
}

// --- d14n_migration_cutover -------------------------------------------------

#[tokio::test]
async fn cutover_defaults_come_from_the_seeded_row() {
    let db = fresh_db("cut_default").await;
    let cutover = db.get_migration_cutover().await.unwrap();
    assert_eq!(cutover.cutover_ns, i64::MAX);
    assert_eq!(cutover.last_checked_ns, 0);
    assert!(!cutover.has_migrated);
}

#[tokio::test]
async fn cutover_setters_are_independent() {
    let db = fresh_db("cut_set").await;
    let ts = 1_700_000_000_000_000_000i64;

    db.set_cutover_ns(ts).await.unwrap();
    let c = db.get_migration_cutover().await.unwrap();
    assert_eq!(c.cutover_ns, ts);
    assert_eq!(
        c.last_checked_ns, 0,
        "setting one column must not clear another"
    );
    assert!(!c.has_migrated);

    db.set_last_checked_ns(ts).await.unwrap();
    assert_eq!(db.get_last_checked_ns().await.unwrap(), ts);
    assert_eq!(db.get_migration_cutover().await.unwrap().cutover_ns, ts);

    db.set_has_migrated(true).await.unwrap();
    assert!(db.get_migration_cutover().await.unwrap().has_migrated);
}

// --- key_store_entry --------------------------------------------------------

#[tokio::test]
async fn key_store_entry_upserts() {
    use sqlx::Row;
    let db = fresh_db("kse_upsert").await;

    db.insert_or_update_key_store_entry(vec![1, 2, 3], vec![b'a'])
        .await
        .unwrap();
    db.insert_or_update_key_store_entry(vec![1, 2, 3], vec![b'b'])
        .await
        .unwrap();

    let mut c = db.conn().await.unwrap();
    let row = sqlx::query("SELECT value_bytes FROM openmls_key_store WHERE key_bytes = $1")
        .bind(vec![1u8, 2, 3])
        .fetch_one(&mut *c)
        .await
        .unwrap();
    let value: Vec<u8> = row.try_get(0).unwrap();
    assert_eq!(
        value,
        vec![b'b'],
        "a second write to the same key must replace the value, matching REPLACE INTO"
    );
}
