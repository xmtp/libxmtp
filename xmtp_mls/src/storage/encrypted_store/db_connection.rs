use crate::storage::{xmtp_openmls_provider::XmtpOpenMlsProvider, StorageError};
use diesel::connection::TransactionManager;
use parking_lot::Mutex;
use std::{
    fmt,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use super::XmtpDb;

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
    // Connection with read-only privileges
    read: Option<Arc<Mutex<C>>>,
    // Connection with write privileges
    write: Arc<Mutex<C>>,
    // Is any connection (possibly this one) currently in a transaction?
    global_transaction_lock: Arc<Mutex<()>>,
    // Is this particular connection in a transaction?
    in_transaction: Arc<AtomicBool>,
}

/// Owned DBConnection Methods
impl<C> DbConnectionPrivate<C> {
    /// Create a new [`DbConnectionPrivate`] from an existing Arc<Mutex<C>>
    pub(super) fn from_arc_mutex(
        write: Arc<Mutex<C>>,
        read: Option<Arc<Mutex<C>>>,
        transaction_lock: Arc<Mutex<()>>,
    ) -> Self {
        Self {
            read,
            write,
            global_transaction_lock: transaction_lock,
            in_transaction: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl<C> DbConnectionPrivate<C>
where
    C: diesel::Connection,
{
    pub(crate) fn start_transaction<Db: XmtpDb<Connection = C>>(
        &self,
    ) -> Result<TransactionGuard<'_>, StorageError> {
        let guard = self.global_transaction_lock.lock();
        let mut write = self.write.lock();
        <Db as XmtpDb>::TransactionManager::begin_transaction(&mut *write)?;
        self.in_transaction.store(true, Ordering::SeqCst);

        Ok(TransactionGuard {
            _mutex_guard: guard,
            in_transaction: self.in_transaction.clone(),
        })
    }

    /// Do a scoped query with a mutable [`diesel::Connection`]
    /// reference
    pub(crate) fn raw_query_read<T, E, F>(&self, fun: F) -> Result<T, E>
    where
        F: FnOnce(&mut C) -> Result<T, E>,
    {
        let mut lock = if self.in_transaction.load(Ordering::SeqCst) {
            self.write.lock()
        } else if let Some(read) = &self.read {
            read.lock()
        } else {
            self.write.lock()
        };

        fun(&mut lock)
    }

    /// Do a scoped query with a mutable [`diesel::Connection`]
    /// reference
    pub(crate) fn raw_query_write<T, E, F>(&self, fun: F) -> Result<T, E>
    where
        F: FnOnce(&mut C) -> Result<T, E>,
    {
        let _guard;
        // If this connection is not in a transaction
        if !self.in_transaction.load(Ordering::SeqCst) {
            // Make sure another connection isn't
            _guard = self.global_transaction_lock.lock();
        }
        fun(&mut self.write.lock())
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

pub struct TransactionGuard<'a> {
    in_transaction: Arc<AtomicBool>,
    _mutex_guard: parking_lot::MutexGuard<'a, ()>,
}
impl Drop for TransactionGuard<'_> {
    fn drop(&mut self) {
        self.in_transaction.store(false, Ordering::SeqCst);
    }
}
