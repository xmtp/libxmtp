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
}

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
