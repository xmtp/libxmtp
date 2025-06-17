use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use diesel::{
    connection::{AnsiTransactionManager, TransactionManager},
    prelude::SqliteConnection,
};
use parking_lot::Mutex;

use crate::{ConnectionError, ConnectionExt, TransactionGuard};

#[derive(Clone)]
pub struct MockConnection {
    inner: Arc<Mutex<SqliteConnection>>,
    in_transaction: Arc<AtomicBool>,
    transaction_lock: Arc<Mutex<()>>,
}

impl std::fmt::Debug for MockConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MockConnection")
    }
}

impl AsRef<MockConnection> for MockConnection {
    fn as_ref(&self) -> &MockConnection {
        self
    }
}

// TODO: We should use diesels test transaction
impl ConnectionExt for MockConnection {
    type Connection = SqliteConnection;

    fn start_transaction(&self) -> Result<crate::TransactionGuard<'_>, crate::ConnectionError> {
        let guard = self.transaction_lock.lock();
        let mut c = self.inner.lock();
        AnsiTransactionManager::begin_transaction(&mut *c)?;
        self.in_transaction.store(true, Ordering::SeqCst);

        Ok(TransactionGuard {
            _mutex_guard: guard,
            in_transaction: self.in_transaction.clone(),
        })
    }

    fn raw_query_read<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut Self::Connection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        let mut conn = self.inner.lock();
        fun(&mut conn).map_err(ConnectionError::from)
    }

    fn raw_query_write<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut Self::Connection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        let mut conn = self.inner.lock();
        fun(&mut conn).map_err(ConnectionError::from)
    }

    fn is_in_transaction(&self) -> bool {
        self.in_transaction.load(Ordering::SeqCst)
    }

    fn disconnect(&self) -> Result<(), ConnectionError> {
        Ok(())
    }

    fn reconnect(&self) -> Result<(), ConnectionError> {
        Ok(())
    }
}
/*
pub struct MockDb;

mock! {
    pub MockDb { }

    impl XmtpDb for MockDb {
    type Connection = MockConnection;

    fn init(&self, opts: &StorageOption) -> Result<(), ConnectionError>;

    /// The Options this databae was created with
    fn opts(&self) -> &crate::StorageOption;

    /// Validate a connection is as expected
    fn validate(&self, _opts: &StorageOption) -> Result<(), ConnectionError>;

    /// Returns the Connection implementation for this Database
    fn conn(&self) -> MockConnection;

    /// Returns a higher-level wrapeped DbConnection from which high-level queries may be
    /// accessed.
    fn db(&self) -> DbConnection<MockConnection>;

    /// Reconnect to the database
    fn reconnect(&self) -> Result<(), ConnectionError>;

    /// Release connection to the database, closing it
    fn disconnect(&self) -> Result<(), ConnectionError>;
    }
}
*/
