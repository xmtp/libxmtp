//! Async-track storage handle: sqlx over Postgres, servers only.
//!
//! # Why there is a lock at all
//!
//! Outside a transaction there is none. `&PgPool` is a sqlx `Executor`, so each
//! `Query*` method takes its own connection from the pool and the handle needs
//! no interior mutability whatsoever.
//!
//! Inside a transaction one is unavoidable, and it is not a choice of *which*
//! cell to use: sqlx transactions are driven through `&mut PgConnection`, while
//! every one of libxmtp's `Query*` methods takes `&self`. Something has to
//! bridge `&self` to `&mut conn`, and that bridge is interior mutability by
//! definition. It is a `tokio::sync::Mutex` rather than a blocking one because
//! the transaction body is caller-supplied and awaits arbitrary work while the
//! guard is held.
//!
//! # Why the pool/transaction split is a value and not a type
//!
//! [`PgDb`] is a single concrete type whose variant is chosen at runtime. A
//! type-level split (`PgDb` plus a separate `PgTx`) would need every `Query*`
//! trait implemented twice, since a blanket `impl<E: PgExecutor> QueryX for E`
//! cannot coexist with the `impl<T: QueryX> QueryX for &T` forwarding impl that
//! each trait already has -- coherence cannot rule out a future `&T: PgExecutor`.
//! One type keeps all ~156 method bodies single-copy: they call
//! [`PgDb::conn`] and write against `&mut PgConnection`, never observing which
//! variant produced it.

use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use sqlx::{PgConnection, PgPool, Postgres, Transaction, pool::PoolConnection};
use tokio::sync::{Mutex, MutexGuard};

use crate::ConnectionError;

/// The async-track database handle. Cheap to clone; clones share one backend.
#[derive(Clone, Debug)]
pub struct PgDb {
    exec: Arc<PgExec>,
}

#[derive(Debug)]
enum PgExec {
    /// Normal operation: no lock, a fresh pooled connection per query.
    Pool(PgPool),
    /// Inside [`PgDb::transaction`]: one pinned connection, serialized.
    Tx(Mutex<Transaction<'static, Postgres>>),
}

/// A connection borrowed for the duration of one query.
///
/// Deliberately not `Clone` and not held across statements by callers -- in the
/// `Tx` case it is a live mutex guard, so keeping one alive across an await that
/// re-enters [`PgDb::conn`] on the same handle would deadlock.
pub enum PgConn<'a> {
    Pooled(PoolConnection<Postgres>),
    Tx(MutexGuard<'a, Transaction<'static, Postgres>>),
}

impl Deref for PgConn<'_> {
    type Target = PgConnection;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Pooled(c) => c,
            Self::Tx(t) => t,
        }
    }
}

impl DerefMut for PgConn<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            Self::Pooled(c) => c,
            Self::Tx(t) => t,
        }
    }
}

impl PgDb {
    pub fn new(pool: PgPool) -> Self {
        Self {
            exec: Arc::new(PgExec::Pool(pool)),
        }
    }

    /// Borrow a connection for one query.
    ///
    /// Hold the returned guard only for the statement that needs it. In the
    /// transaction variant it is a mutex guard, so calling `conn()` again on the
    /// same handle while one is alive deadlocks rather than erroring.
    pub async fn conn(&self) -> Result<PgConn<'_>, ConnectionError> {
        match &*self.exec {
            PgExec::Pool(pool) => Ok(PgConn::Pooled(pool.acquire().await?)),
            PgExec::Tx(tx) => Ok(PgConn::Tx(tx.lock().await)),
        }
    }

    /// True when this handle is already inside a transaction.
    pub fn in_transaction(&self) -> bool {
        matches!(&*self.exec, PgExec::Tx(_))
    }

    /// Run `f` against a handle pinned to a single connection inside a Postgres
    /// transaction, committing on `Ok` and rolling back on `Err`.
    ///
    /// This is what keeps openmls' writes and libxmtp's own table writes atomic
    /// with respect to each other: both go through the handle `f` receives, so
    /// both land on the one pinned connection.
    ///
    /// Nesting is rejected rather than silently flattened -- a nested call would
    /// otherwise look transactional while committing with the outer transaction.
    pub async fn transaction<T, E, F>(&self, f: F) -> Result<T, E>
    where
        F: AsyncFnOnce(&PgDb) -> Result<T, E>,
        E: From<ConnectionError>,
    {
        let PgExec::Pool(pool) = &*self.exec else {
            return Err(ConnectionError::InvalidQuery("nested transaction".into()).into());
        };

        let tx = pool.begin().await.map_err(ConnectionError::from)?;
        let scoped = PgDb {
            exec: Arc::new(PgExec::Tx(Mutex::new(tx))),
        };

        let result = f(&scoped).await;

        // Reclaim the transaction to commit it. `Arc::into_inner` returns `None`
        // only if `f` stashed a clone of the handle somewhere outliving this
        // scope; in that case the `Transaction` is dropped and sqlx rolls it
        // back, which is the safe outcome.
        let Some(PgExec::Tx(tx)) = Arc::into_inner(scoped.exec) else {
            return Err(ConnectionError::TransactionHandleEscaped.into());
        };
        let tx = tx.into_inner();

        match result {
            Ok(value) => {
                tx.commit().await.map_err(ConnectionError::from)?;
                Ok(value)
            }
            Err(e) => {
                // An explicit rollback surfaces its own failure; dropping would
                // roll back too but swallow any error doing so.
                tx.rollback().await.map_err(ConnectionError::from)?;
                Err(e)
            }
        }
    }
}

/// These run against a real Postgres, not a mock: the whole point of the async
/// track is that a query is now a network round-trip, and the failure modes
/// worth testing (contention, isolation, rollback, concurrency) do not exist
/// in-process. Set `XMTP_ASYNCDB_PG_URL` to a scratch database.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::is_retryable_sqlx;
    use sqlx::Row;
    use sqlx::postgres::PgPoolOptions;
    use xmtp_common::RetryableError;

    async fn db() -> PgDb {
        let url = std::env::var("XMTP_ASYNCDB_PG_URL")
            .expect("XMTP_ASYNCDB_PG_URL must point at a scratch Postgres");
        let pool = PgPoolOptions::new()
            .max_connections(8)
            .connect(&url)
            .await
            .expect("connect");
        PgDb::new(pool)
    }

    /// Each test owns a uniquely-named table so they can run concurrently.
    async fn scratch_table(db: &PgDb, name: &str) -> String {
        let table = format!("t_{name}");
        let mut c = db.conn().await.unwrap();
        sqlx::query(&format!("DROP TABLE IF EXISTS {table}"))
            .execute(&mut *c)
            .await
            .unwrap();
        sqlx::query(&format!(
            "CREATE TABLE {table} (k int PRIMARY KEY, v int NOT NULL)"
        ))
        .execute(&mut *c)
        .await
        .unwrap();
        table
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
        let db = db().await;
        let table = scratch_table(&db, "roundtrip").await;
        let mut c = db.conn().await.unwrap();
        sqlx::query(&format!("INSERT INTO {table} (k, v) VALUES (1, 42)"))
            .execute(&mut *c)
            .await
            .unwrap();
        drop(c);
        assert_eq!(count(&db, &table).await, 1);
    }

    #[tokio::test]
    async fn transaction_commits() {
        let db = db().await;
        let table = scratch_table(&db, "commit").await;
        db.transaction(async |tx: &PgDb| -> Result<(), ConnectionError> {
            let mut c = tx.conn().await?;
            sqlx::query(&format!("INSERT INTO {table} (k, v) VALUES (1, 1)"))
                .execute(&mut *c)
                .await?;
            Ok(())
        })
        .await
        .unwrap();
        assert_eq!(count(&db, &table).await, 1);
    }

    #[tokio::test]
    async fn transaction_rolls_back_on_error() {
        let db = db().await;
        let table = scratch_table(&db, "rollback").await;
        let res = db
            .transaction(async |tx: &PgDb| -> Result<(), ConnectionError> {
                let mut c = tx.conn().await?;
                sqlx::query(&format!("INSERT INTO {table} (k, v) VALUES (1, 1)"))
                    .execute(&mut *c)
                    .await?;
                Err(ConnectionError::InvalidQuery("deliberate".into()))
            })
            .await;
        assert!(res.is_err());
        assert_eq!(
            count(&db, &table).await,
            0,
            "rollback must discard the insert"
        );
    }

    /// The cross-store atomicity guarantee: uncommitted writes made through the
    /// transaction handle are invisible to anyone else until commit. This is what
    /// keeps openmls writes and libxmtp table writes all-or-nothing together.
    #[tokio::test]
    async fn transaction_writes_are_invisible_until_commit() {
        let db = db().await;
        let table = scratch_table(&db, "isolation").await;
        db.transaction(async |tx: &PgDb| -> Result<(), ConnectionError> {
            let mut c = tx.conn().await?;
            sqlx::query(&format!("INSERT INTO {table} (k, v) VALUES (1, 1)"))
                .execute(&mut *c)
                .await?;
            // `db` is the pool handle, i.e. a different connection.
            assert_eq!(
                count(&db, &table).await,
                0,
                "must not be visible pre-commit"
            );
            Ok(())
        })
        .await
        .unwrap();
        assert_eq!(count(&db, &table).await, 1, "must be visible post-commit");
    }

    #[tokio::test]
    async fn nested_transaction_is_rejected() {
        let db = db().await;
        let res = db
            .transaction(async |tx: &PgDb| -> Result<(), ConnectionError> {
                assert!(tx.in_transaction());
                tx.transaction(async |_: &PgDb| -> Result<(), ConnectionError> { Ok(()) })
                    .await
            })
            .await;
        assert!(res.is_err(), "nesting must not silently flatten");
    }

    /// The pool path holds no lock, so independent queries genuinely overlap
    /// rather than serializing behind one connection.
    #[tokio::test]
    async fn concurrent_pool_queries_do_not_serialize() {
        let db = db().await;
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
    /// SQLITE_BUSY. If this is not classified retryable, the retry layer that
    /// works on SQLite silently stops working on Postgres.
    #[tokio::test]
    async fn serialization_failure_is_retryable() {
        let db = db().await;
        let table = scratch_table(&db, "serialization").await;
        {
            let mut c = db.conn().await.unwrap();
            sqlx::query(&format!("INSERT INTO {table} (k, v) VALUES (1, 0), (2, 0)"))
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
        sqlx::query(&format!("SELECT sum(v) FROM {table}"))
            .fetch_one(&mut *a)
            .await
            .unwrap();
        sqlx::query(&format!("SELECT sum(v) FROM {table}"))
            .fetch_one(&mut *b)
            .await
            .unwrap();
        sqlx::query(&format!("UPDATE {table} SET v = 1 WHERE k = 1"))
            .execute(&mut *a)
            .await
            .unwrap();
        sqlx::query(&format!("UPDATE {table} SET v = 1 WHERE k = 2"))
            .execute(&mut *b)
            .await
            .unwrap();
        sqlx::query("COMMIT").execute(&mut *a).await.unwrap();

        let err = sqlx::query("COMMIT")
            .execute(&mut *b)
            .await
            .expect_err("second commit must fail with a serialization failure");
        // Assert the SQLSTATE explicitly: without this the test would also pass
        // on an unrelated error that happens to classify as retryable, proving
        // nothing about 40001 in particular.
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
}
