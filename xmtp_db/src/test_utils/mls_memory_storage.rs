use crate::sql_key_store::SqlKeyStore;
use crate::{ConnectionExt, InstrumentedSqliteConnection, MIGRATIONS};
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use diesel_migrations::MigrationHarness;
use parking_lot::Mutex;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

pub type MlsMemoryStorage = SqlKeyStore<MemoryStorage>;

#[derive(Clone)]
pub struct MemoryStorage {
    inner: Arc<Mutex<SqliteConnection>>,
    tx_counter: Arc<AtomicUsize>,
    in_transaction: Arc<AtomicBool>,
}

impl Default for MemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryStorage {
    pub fn new() -> Self {
        let mut conn = SqliteConnection::establish(":memory:").unwrap();
        conn.run_pending_migrations(MIGRATIONS).unwrap();
        conn.set_instrumentation(InstrumentedSqliteConnection);
        Self {
            inner: Arc::new(Mutex::new(conn)),
            tx_counter: Arc::new(AtomicUsize::new(0)),
            in_transaction: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Print the key-value pairs in MLS memory as hex
    pub fn key_value_pairs(&self) -> String {
        todo!()
    }
}

impl ConnectionExt for MemoryStorage {
    type Connection = SqliteConnection;

    fn raw_query_read<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut Self::Connection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        let mut c = self.inner.lock();
        Ok(fun(&mut c)?)
    }

    fn raw_query_write<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut Self::Connection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        let mut c = self.inner.lock();
        Ok(fun(&mut c)?)
    }

    fn disconnect(&self) -> Result<(), crate::ConnectionError> {
        unimplemented!()
    }

    fn reconnect(&self) -> Result<(), crate::ConnectionError> {
        unimplemented!()
    }
}
