use parking_lot::Mutex;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::storage::xmtp_openmls_provider::XmtpOpenMlsProvider;

#[cfg(not(target_arch = "wasm32"))]
pub type DbConnection = DbConnectionPrivate<super::RawDbConnection>;

#[cfg(target_arch = "wasm32")]
pub type DbConnection = DbConnectionPrivate<sqlite_web::connection::WasmSqliteConnection>;

/// A wrapper for RawDbConnection that houses all XMTP DB operations.
/// Uses a [`Mutex]` internally for interior mutability, so that the connection
/// and transaction state can be shared between the OpenMLS Provider and
/// native XMTP operations
// ~~~~ _NOTE_ ~~~~~
// Do not derive clone here.
// callers should be able to accomplish everything with one conn/reference.
#[doc(hidden)]
pub struct DbConnectionPrivate<C> {
    read: Arc<Mutex<C>>,
    write: Option<Arc<Mutex<C>>>,
    pub(super) in_transaction: Arc<AtomicBool>,
}

/// Owned DBConnection Methods
impl<C> DbConnectionPrivate<C> {
    /// Create a new [`DbConnectionPrivate`] from an existing Arc<Mutex<C>>
    pub(super) fn from_arc_mutex(read: Arc<Mutex<C>>, write: Option<Arc<Mutex<C>>>) -> Self {
        Self {
            read,
            write,
            in_transaction: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl<C> DbConnectionPrivate<C>
where
    C: diesel::Connection,
{
    fn in_transaction(&self) -> bool {
        self.in_transaction.load(Ordering::SeqCst)
    }

    pub(crate) fn start_transaction(&self) -> TransactionGuard {
        self.in_transaction.store(true, Ordering::SeqCst);
        TransactionGuard {
            in_transaction: self.in_transaction.clone(),
        }
    }

    /// Do a scoped query with a mutable [`diesel::Connection`]
    /// reference
    pub(crate) fn raw_query<T, E, F>(&self, write: bool, fun: F) -> Result<T, E>
    where
        F: FnOnce(&mut C) -> Result<T, E>,
    {
        if write {
            if let Some(write_conn) = &self.write {
                let mut lock = write_conn.lock();
                return fun(&mut lock);
            }
        }

        let mut lock = self.read.lock();
        fun(&mut lock)
    }

    /// Internal-only API to get the underlying `diesel::Connection` reference
    /// without a scope
    /// Must be used with care. holding this reference while calling `raw_query`
    /// will cause a deadlock.
    pub(super) fn read_mut_ref(&self) -> parking_lot::MutexGuard<'_, C> {
        if self.in_transaction() {
            if let Some(write) = &self.write {
                return write.lock();
            }
        }
        self.read.lock()
    }

    /// Internal-only API to get the underlying `diesel::Connection` reference
    /// without a scope
    pub(super) fn read_ref(&self) -> Arc<Mutex<C>> {
        if self.in_transaction() {
            if let Some(write) = &self.write {
                return write.clone();
            };
        }
        self.read.clone()
    }

    /// Internal-only API to get the underlying `diesel::Connection` reference
    /// without a scope
    /// Must be used with care. holding this reference while calling `raw_query`
    /// will cause a deadlock.
    pub(super) fn write_mut_ref(&self) -> parking_lot::MutexGuard<'_, C> {
        let Some(write) = &self.write else {
            return self.read_mut_ref();
        };
        write.lock()
    }

    /// Internal-only API to get the underlying `diesel::Connection` reference
    /// without a scope
    pub(super) fn write_ref(&self) -> Option<Arc<Mutex<C>>> {
        self.write.clone()
    }
}

// Forces a move for conn
// This is an important distinction from deriving `Clone` on `DbConnection`.
// This way, conn will be moved into XmtpOpenMlsProvider. This forces codepaths to
// use a connection from the provider, rather than pulling a new one from the pool, resulting
// in two connections in the same scope.
impl From<DbConnection> for XmtpOpenMlsProvider {
    fn from(conn: DbConnection) -> XmtpOpenMlsProvider {
        XmtpOpenMlsProvider::new(conn)
    }
}

impl<C> fmt::Debug for DbConnectionPrivate<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DbConnection")
            .field("wrapped_conn", &"DbConnection")
            .finish()
    }
}

pub struct TransactionGuard {
    in_transaction: Arc<AtomicBool>,
}
impl Drop for TransactionGuard {
    fn drop(&mut self) {
        self.in_transaction.store(false, Ordering::SeqCst);
    }
}
