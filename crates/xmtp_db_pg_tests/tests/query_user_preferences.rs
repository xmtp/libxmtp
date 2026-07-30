//! The sqlx `QueryUserPreferences` impl.
//!
//! `user_preferences` was the one table of the 23 with no `Query*` trait at all
//! — it was reached directly through `ConnectionExt::raw_query`, which is
//! sync-track-only — so the trait had to be written before it could be ported.
//! It is a singleton row pinned to `id = 0` by a CHECK constraint on both
//! backends.

use xmtp_db::pg::PgDb;
use xmtp_db::user_preferences::{HmacKey, QueryUserPreferences};
use xmtp_db::StorageError::InvalidHmacLength;
use xmtp_db_pg_tests::fresh_db;

async fn row_count(db: &PgDb) -> i64 {
    let mut c = db.conn().await.unwrap();
    sqlx::query_scalar("SELECT COUNT(*) FROM user_preferences")
        .fetch_one(&mut *c)
        .await
        .unwrap()
}

#[tokio::test]
async fn load_returns_defaults_before_anything_is_stored() {
    let db = fresh_db("up_defaults").await;
    let prefs = db.load_user_preferences().await.unwrap();
    assert_eq!(prefs.id, 0);
    assert!(prefs.hmac_key.is_none());
    assert!(prefs.hmac_key_cycled_at_ns.is_none());
    assert!(!prefs.dm_group_updates_migrated);
    assert_eq!(row_count(&db).await, 0, "loading does not create the row");
}

#[tokio::test]
async fn storing_an_hmac_key_creates_then_updates_the_one_row() {
    let db = fresh_db("up_hmac").await;
    let first = HmacKey::random_key();
    db.store_hmac_key(&first, None).await.unwrap();

    let prefs = db.load_user_preferences().await.unwrap();
    assert_eq!(prefs.hmac_key.as_ref(), Some(&first));
    assert!(prefs.hmac_key_cycled_at_ns.unwrap() > 0, "defaults to now");
    assert_eq!(row_count(&db).await, 1);

    let second = HmacKey::random_key();
    db.store_hmac_key(&second, None).await.unwrap();
    assert_eq!(db.load_user_preferences().await.unwrap().hmac_key, Some(second));
    assert_eq!(row_count(&db).await, 1, "still a singleton");
}

/// A device-sync update carrying an older `cycled_at_ns` is a message that
/// arrived out of order, and must not roll the key back.
#[tokio::test]
async fn an_older_cycled_at_is_ignored() {
    let db = fresh_db("up_monotonic").await;
    let current = HmacKey::random_key();
    db.store_hmac_key(&current, Some(500)).await.unwrap();

    let stale = HmacKey::random_key();
    db.store_hmac_key(&stale, Some(499)).await.unwrap();
    let prefs = db.load_user_preferences().await.unwrap();
    assert_eq!(prefs.hmac_key.as_ref(), Some(&current));
    assert_eq!(prefs.hmac_key_cycled_at_ns, Some(500));

    // Equal timestamps still write, matching the sync path's `old > new` guard.
    let same_instant = HmacKey::random_key();
    db.store_hmac_key(&same_instant, Some(500)).await.unwrap();
    assert_eq!(
        db.load_user_preferences().await.unwrap().hmac_key,
        Some(same_instant)
    );

    let newer = HmacKey::random_key();
    db.store_hmac_key(&newer, Some(900)).await.unwrap();
    let prefs = db.load_user_preferences().await.unwrap();
    assert_eq!(prefs.hmac_key, Some(newer));
    assert_eq!(prefs.hmac_key_cycled_at_ns, Some(900));
}

/// `None` means "now" and is a local rotation, not a replayed sync message, so
/// it overwrites even a stored timestamp in the future.
#[tokio::test]
async fn a_local_rotation_always_wins() {
    let db = fresh_db("up_local").await;
    db.store_hmac_key(&HmacKey::random_key(), Some(i64::MAX))
        .await
        .unwrap();

    let local = HmacKey::random_key();
    db.store_hmac_key(&local, None).await.unwrap();
    assert_eq!(db.load_user_preferences().await.unwrap().hmac_key, Some(local));
}

#[tokio::test]
async fn a_wrong_length_key_is_rejected_before_it_reaches_the_database() {
    let db = fresh_db("up_len").await;
    assert!(matches!(
        db.store_hmac_key(&[1, 2, 3], None).await.unwrap_err(),
        InvalidHmacLength
    ));
    assert_eq!(row_count(&db).await, 0);
}

/// Upsert rather than the sync path's bare UPDATE: the row may not exist yet,
/// and a flag that fails to stick makes the one-time migration re-run forever.
#[tokio::test]
async fn the_migration_flag_sticks_even_with_no_existing_row() {
    let db = fresh_db("up_migrated").await;
    db.set_dm_group_updates_migrated().await.unwrap();
    assert!(db.load_user_preferences().await.unwrap().dm_group_updates_migrated);
    assert_eq!(row_count(&db).await, 1);

    // Idempotent, and it does not disturb the key alongside it.
    let key = HmacKey::random_key();
    db.store_hmac_key(&key, None).await.unwrap();
    db.set_dm_group_updates_migrated().await.unwrap();
    let prefs = db.load_user_preferences().await.unwrap();
    assert!(prefs.dm_group_updates_migrated);
    assert_eq!(prefs.hmac_key, Some(key));
}

/// The CHECK constraint is what makes "load the first row" unambiguous.
#[tokio::test]
async fn the_table_refuses_a_second_row() {
    let db = fresh_db("up_singleton").await;
    db.store_hmac_key(&HmacKey::random_key(), None).await.unwrap();

    let mut c = db.conn().await.unwrap();
    let err = sqlx::query("INSERT INTO user_preferences (id) VALUES (1)")
        .execute(&mut *c)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("check constraint"),
        "CHECK (id = 0) rejects it, not the primary key: {err}"
    );
}
