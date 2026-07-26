//! `PgDb` executor semantics against a real Postgres.

use sqlx::Row;
use xmtp_common::RetryableError;
use xmtp_db::pg::PgDb;
use xmtp_db::{ConnectionError, is_retryable_sqlx};
use xmtp_db_pg_tests::bare_db;

async fn scratch_table(db: &PgDb, table: &str) {
    let mut c = db.conn().await.unwrap();
    sqlx::query(&format!(
        "CREATE TABLE {table} (k int PRIMARY KEY, v int NOT NULL)"
    ))
    .execute(&mut *c)
    .await
    .unwrap();
}

async fn count(db: &PgDb, table: &str) -> i64 {
    let mut c = db.conn().await.unwrap();
    sqlx::query(&format!("SELECT count(*) FROM {table}"))
        .fetch_one(&mut *c)
        .await
        .unwrap()
        .get::<i64, _>(0)
}

#[tokio::test]
async fn pool_query_roundtrips() {
    let db = bare_db("roundtrip").await;
    scratch_table(&db, "t").await;
    let mut c = db.conn().await.unwrap();
    sqlx::query("INSERT INTO t (k, v) VALUES (1, 42)")
        .execute(&mut *c)
        .await
        .unwrap();
    drop(c);
    assert_eq!(count(&db, "t").await, 1);
}

#[tokio::test]
async fn transaction_commits() {
    let db = bare_db("commit").await;
    scratch_table(&db, "t").await;
    db.transaction(async |tx: &PgDb| -> Result<(), ConnectionError> {
        let mut c = tx.conn().await?;
        sqlx::query("INSERT INTO t (k, v) VALUES (1, 1)")
            .execute(&mut *c)
            .await?;
        Ok(())
    })
    .await
    .unwrap();
    assert_eq!(count(&db, "t").await, 1);
}

#[tokio::test]
async fn transaction_rolls_back_on_error() {
    let db = bare_db("rollback").await;
    scratch_table(&db, "t").await;
    let res = db
        .transaction(async |tx: &PgDb| -> Result<(), ConnectionError> {
            let mut c = tx.conn().await?;
            sqlx::query("INSERT INTO t (k, v) VALUES (1, 1)")
                .execute(&mut *c)
                .await?;
            Err(ConnectionError::InvalidQuery("deliberate".into()))
        })
        .await;
    assert!(res.is_err());
    assert_eq!(count(&db, "t").await, 0, "rollback must discard the insert");
}

/// The cross-store atomicity guarantee: uncommitted writes made through the
/// transaction handle are invisible to anyone else until commit. This is what
/// keeps openmls writes and libxmtp table writes all-or-nothing together.
#[tokio::test]
async fn transaction_writes_are_invisible_until_commit() {
    let db = bare_db("isolation").await;
    scratch_table(&db, "t").await;
    db.transaction(async |tx: &PgDb| -> Result<(), ConnectionError> {
        let mut c = tx.conn().await?;
        sqlx::query("INSERT INTO t (k, v) VALUES (1, 1)")
            .execute(&mut *c)
            .await?;
        // `db` is the pool handle, i.e. a different connection.
        assert_eq!(count(&db, "t").await, 0, "must not be visible pre-commit");
        Ok(())
    })
    .await
    .unwrap();
    assert_eq!(count(&db, "t").await, 1, "must be visible post-commit");
}

#[tokio::test]
async fn nested_transaction_is_rejected() {
    let db = bare_db("nested").await;
    let res = db
        .transaction(async |tx: &PgDb| -> Result<(), ConnectionError> {
            assert!(tx.in_transaction());
            tx.transaction(async |_: &PgDb| -> Result<(), ConnectionError> { Ok(()) })
                .await
        })
        .await;
    assert!(res.is_err(), "nesting must not silently flatten");
}

/// The pool path holds no lock, so independent queries genuinely overlap rather
/// than serializing behind one connection.
#[tokio::test]
async fn concurrent_pool_queries_do_not_serialize() {
    let db = bare_db("concurrency").await;
    let started = std::time::Instant::now();
    let tasks: Vec<_> = (0..8)
        .map(|_| {
            let db = db.clone();
            tokio::spawn(async move {
                let mut c = db.conn().await.unwrap();
                sqlx::query("SELECT pg_sleep(0.25)")
                    .execute(&mut *c)
                    .await
                    .unwrap();
            })
        })
        .collect();
    for t in tasks {
        t.await.unwrap();
    }
    let elapsed = started.elapsed();
    assert!(
        elapsed < std::time::Duration::from_millis(1200),
        "8x250ms sleeps took {elapsed:?}; they serialized instead of overlapping"
    );
}

/// Postgres reports contention as SQLSTATE 40001 where SQLite reports
/// SQLITE_BUSY. If this is not classified retryable, the retry layer that works
/// on SQLite silently stops working on Postgres.
#[tokio::test]
async fn serialization_failure_is_retryable() {
    let db = bare_db("serialization").await;
    scratch_table(&db, "t").await;
    {
        let mut c = db.conn().await.unwrap();
        sqlx::query("INSERT INTO t (k, v) VALUES (1, 0), (2, 0)")
            .execute(&mut *c)
            .await
            .unwrap();
    }

    // Two SERIALIZABLE transactions with read-write dependencies in both
    // directions: Postgres must abort one with 40001.
    let mut a = db.conn().await.unwrap();
    let mut b = db.conn().await.unwrap();
    for c in [&mut a, &mut b] {
        sqlx::query("BEGIN ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut **c)
            .await
            .unwrap();
    }
    sqlx::query("SELECT sum(v) FROM t")
        .fetch_one(&mut *a)
        .await
        .unwrap();
    sqlx::query("SELECT sum(v) FROM t")
        .fetch_one(&mut *b)
        .await
        .unwrap();
    sqlx::query("UPDATE t SET v = 1 WHERE k = 1")
        .execute(&mut *a)
        .await
        .unwrap();
    sqlx::query("UPDATE t SET v = 1 WHERE k = 2")
        .execute(&mut *b)
        .await
        .unwrap();
    sqlx::query("COMMIT").execute(&mut *a).await.unwrap();

    let err = sqlx::query("COMMIT")
        .execute(&mut *b)
        .await
        .expect_err("second commit must fail with a serialization failure");
    // Assert the SQLSTATE explicitly: without this the test would also pass on an
    // unrelated error that happens to classify as retryable, proving nothing
    // about 40001 in particular.
    let sqlstate = err
        .as_database_error()
        .and_then(|e| e.code())
        .map(|c| c.into_owned());
    assert_eq!(
        sqlstate.as_deref(),
        Some("40001"),
        "expected serialization_failure, got: {err:?}"
    );
    assert!(
        is_retryable_sqlx(&err),
        "40001 must be retryable, got: {err:?}"
    );
    assert!(
        ConnectionError::from(err).is_retryable(),
        "classification must survive the ConnectionError conversion"
    );
}
