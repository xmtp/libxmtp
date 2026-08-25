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
//! cannot coexist with the `impl<T: QueryX + xmtp_common::MaybeSync> QueryX for &T` forwarding impl that
//! each trait already has -- coherence cannot rule out a future `&T: PgExecutor`.
//! One type keeps all ~156 method bodies single-copy: they call
//! [`PgDb::conn`] and write against `&mut PgConnection`, never observing which
//! variant produced it.

use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

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

/// Delegates to the inherent [`PgDb::conn`]; see [`crate::PgConnectionProvider`].
/// This is what makes the generic `Store`/`Fetch` impls reachable through a
/// `&impl DbQuery` on the async track.
impl crate::PgConnectionProvider for PgDb {
    fn pg_conn(
        &self,
    ) -> impl std::future::Future<Output = Result<PgConn<'_>, ConnectionError>> + xmtp_common::MaybeSend
    {
        self.conn()
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

    /// Run `f` atomically, joining the caller's transaction if there is one.
    ///
    /// [`Self::transaction`] deliberately rejects nesting, which is right for a
    /// caller that means "start a transaction" but wrong for a query method that
    /// only means "these writes must land together". Such a method can be called
    /// either standalone or from inside a larger transaction, and must work both
    /// ways: here it opens its own, there it inherits the caller's and lets the
    /// outer scope decide the commit.
    pub async fn atomic<T, E, F>(&self, f: F) -> Result<T, E>
    where
        F: AsyncFnOnce(&PgDb) -> Result<T, E>,
        E: From<ConnectionError>,
    {
        if self.in_transaction() {
            f(self).await
        } else {
            self.transaction(f).await
        }
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

    /// Run `f` as a Postgres SAVEPOINT nested inside the caller's transaction,
    /// releasing it on `Ok` and rolling back to it on `Err`.
    ///
    /// This is the async analog of the sync/diesel track's *nested*
    /// `transaction` call, which issues a SAVEPOINT under an already-open
    /// transaction. MLS welcome and commit processing legitimately nests an
    /// atomic sub-unit inside [`Self::transaction`] (see `xmtp_welcome`'s
    /// `transaction(..).savepoint(..)`), and that sub-unit must be able to roll
    /// back on its own without aborting the enclosing transaction — precisely
    /// what a SAVEPOINT provides and what a nested `transaction` (rejected by
    /// design) cannot.
    ///
    /// Called with no open transaction there is nothing to nest inside, so it
    /// degrades to a plain [`Self::transaction`]. `f` runs against the same
    /// pinned connection, so its writes join the savepoint.
    pub async fn savepoint<T, E, F>(&self, f: F) -> Result<T, E>
    where
        F: AsyncFnOnce(&PgDb) -> Result<T, E>,
        E: From<ConnectionError>,
    {
        if !self.in_transaction() {
            return self.transaction(f).await;
        }

        // A unique name per savepoint so strictly-nested savepoints never alias.
        // Names are cleaned up (RELEASE) on both paths, but reuse would still be
        // ambiguous if two live at once, which double-nesting can do.
        let name = format!(
            "xmtp_sp_{}",
            SAVEPOINT_SEQ.fetch_add(1, Ordering::Relaxed)
        );

        // Open the savepoint, then drop the connection guard before running `f`
        // so `f`'s own queries can re-acquire the pinned connection.
        {
            let mut conn = self.conn().await?;
            sqlx::query(&format!("SAVEPOINT {name}"))
                .execute(&mut *conn)
                .await
                .map_err(ConnectionError::from)?;
        }

        let result = f(self).await;

        let mut conn = self.conn().await?;
        match result {
            Ok(value) => {
                sqlx::query(&format!("RELEASE SAVEPOINT {name}"))
                    .execute(&mut *conn)
                    .await
                    .map_err(ConnectionError::from)?;
                Ok(value)
            }
            Err(e) => {
                // Roll the sub-unit back, then release the (now-empty) savepoint
                // so its name does not linger for the rest of the transaction.
                sqlx::query(&format!("ROLLBACK TO SAVEPOINT {name}"))
                    .execute(&mut *conn)
                    .await
                    .map_err(ConnectionError::from)?;
                sqlx::query(&format!("RELEASE SAVEPOINT {name}"))
                    .execute(&mut *conn)
                    .await
                    .map_err(ConnectionError::from)?;
                Err(e)
            }
        }
    }
}

/// Monotonic source of unique SAVEPOINT identifiers for [`PgDb::savepoint`].
static SAVEPOINT_SEQ: AtomicU64 = AtomicU64::new(0);

// Tests for this module live in the `xmtp_db_pg_tests` crate, not here: the sqlx
// `Query*` impls are gated `not(feature = "sync")`, and every test target inside
// this crate has `sync` on via the self dev-dependency, so they would be compiled
// out. That crate depends on xmtp_db from outside and exercises the real
// `--no-default-features --features async` build.

/// A struct that maps onto a Postgres table or view, as emitted by
/// `#[derive(xmtp_macro::PgModel)]`.
///
/// The async track has no `diesel::table!` equivalent, so this is where a
/// model's column list lives. Implementations are generated from the struct's
/// fields, never written by hand -- writing one by hand would reintroduce
/// exactly the drift the derive exists to prevent.
pub trait PgModel: Sized + for<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow> {
    /// The table (or view) these columns belong to.
    const TABLE: &'static str;
    /// Every column, in field order.
    const COLUMNS: &'static [&'static str];

    /// `COLUMNS` as a select list: `"id, created_at_ns, ..."`.
    fn select_columns() -> String {
        Self::COLUMNS.join(", ")
    }

    /// `COLUMNS` qualified with a table alias, for queries that join:
    /// `"m.id, m.created_at_ns, ..."`.
    fn select_columns_for(alias: &str) -> String {
        Self::COLUMNS
            .iter()
            .map(|c| format!("{alias}.{c}"))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// `PgDb` is the async track's `DbQuery`, and callers take `&impl DbQuery`, so
/// both forms have to hold. Asserting it here means the async clippy job fails
/// the moment a `Query*` impl goes missing or drifts out of the supertrait list
/// -- otherwise the first sign is an unrelated downstream crate failing to
/// satisfy a bound, far from the cause.
///
/// Gated `not(sync)` to match the sqlx impls: with both features on, `sync`
/// wins and they are compiled out, so the assertion would be false there.
#[cfg(not(feature = "sync"))]
const _: fn() = || {
    fn assert_db_query<T: crate::DbQuery>() {}
    assert_db_query::<PgDb>();
    assert_db_query::<&PgDb>();
};

/// The async-track [`XmtpDb`] store, backing an `xmtp_mls::Client` with a
/// Postgres [`PgDb`] instead of the sync/diesel `EncryptedMessageStore`.
///
/// `EncryptedMessageStore` is the SQLite/diesel store (its `XmtpDb::new` runs
/// diesel migrations and it carries SQLite-only bits); the async track supplies
/// its own store type over `PgDb`, exactly as the `MlsContext` alias comment in
/// xmtp_mls notes. Schema setup is the caller's job (run the Postgres migrations
/// against the pool before constructing this), so there is no `init` here — the
/// `XmtpDb::init` hook is sync-only anyway.
///
/// `conn()` and `db()` both hand back the same cheap-to-clone [`PgDb`] handle:
/// on the async track a "connection" is just a pooled handle, and every query
/// goes through the `DbQuery` (`= PgDb`) surface, never a raw diesel connection.
#[cfg(all(feature = "async", not(feature = "sync"), not(target_arch = "wasm32")))]
#[derive(Clone, Debug)]
pub struct PgMlsDb {
    db: PgDb,
    opts: crate::StorageOption,
}

#[cfg(all(feature = "async", not(feature = "sync"), not(target_arch = "wasm32")))]
impl PgMlsDb {
    /// Wrap a `PgDb` as an `XmtpDb` store. Defaults to `Ephemeral` opts — a
    /// server-side Postgres store has no local file path, and `Ephemeral` keeps
    /// callers off the diesel/SQLite path-based logic keyed on `Persistent`.
    pub fn new(db: PgDb) -> Self {
        Self {
            db,
            opts: crate::StorageOption::Ephemeral,
        }
    }

    /// Wrap a `PgDb` with an explicit [`StorageOption`].
    pub fn with_opts(db: PgDb, opts: crate::StorageOption) -> Self {
        Self { db, opts }
    }

    /// The underlying `PgDb` handle (e.g. to build a `PgKeyStore` or a cursor
    /// store over the same pool).
    pub fn pg(&self) -> &PgDb {
        &self.db
    }
}

#[cfg(all(feature = "async", not(feature = "sync"), not(target_arch = "wasm32")))]
impl crate::XmtpDb for PgMlsDb {
    type Connection = PgDb;
    type DbQuery = PgDb;

    fn conn(&self) -> Self::Connection {
        self.db.clone()
    }

    fn db(&self) -> Self::DbQuery {
        self.db.clone()
    }

    fn opts(&self) -> &crate::StorageOption {
        &self.opts
    }

    fn reconnect(&self) -> Result<(), crate::ConnectionError> {
        Ok(())
    }

    fn disconnect(&self) -> Result<(), crate::ConnectionError> {
        Ok(())
    }
}
