use diesel::prelude::*;
use diesel_migrations::MigrationHarness;

use crate::sql_key_store::SqlKeyStore;
use crate::{ConnectionExt, MIGRATIONS};
use parking_lot::Mutex;
use std::sync::Arc;

pub type MlsMemoryStorage = SqlKeyStore<MemoryStorage>;

#[derive(Clone)]
pub struct MemoryStorage {
    inner: Arc<Mutex<diesel::SqliteConnection>>,
}

impl Default for MemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryStorage {
    pub fn new() -> Self {
        let mut conn = diesel::SqliteConnection::establish(":memory:").unwrap();
        conn.run_pending_migrations(MIGRATIONS).unwrap();
        Self {
            inner: Arc::new(Mutex::new(conn)),
        }
    }

    /// Print the key-value pairs in MLS memory as hex
    pub fn key_value_pairs(&self) -> String {
        todo!()
    }
}

impl ConnectionExt for MemoryStorage {
    type Connection = diesel::SqliteConnection;

    // mls memory storage does not do transactions
    fn start_transaction(&self) -> Result<crate::TransactionGuard, crate::ConnectionError> {
        panic!("memory storage cannot start txs")
    }

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

    fn is_in_transaction(&self) -> bool {
        false
    }

    fn disconnect(&self) -> Result<(), crate::ConnectionError> {
        unimplemented!()
    }

    fn reconnect(&self) -> Result<(), crate::ConnectionError> {
        unimplemented!()
    }
}
