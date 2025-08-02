use diesel::prelude::*;
use diesel_migrations::MigrationHarness;

use crate::schema::openmls_key_value::dsl;
use crate::sql_key_store::SqlKeyStore;
use crate::{ConnectionExt, MIGRATIONS};
use parking_lot::Mutex;
use std::fmt::Write;
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
        let mut c = self.inner.lock();
        let key_values = dsl::openmls_key_value
            .select((dsl::key_bytes, dsl::value_bytes))
            .load::<(Vec<u8>, Vec<u8>)>(&mut *c)
            .unwrap();
        let mut s = String::new();
        s.push('\n');
        for (key, value) in key_values.iter() {
            write!(s, "{}:{}", hex::encode(key), hex::encode(value)).unwrap();
            s.push('\n');
        }
        s
    }

    /// Print the key-value pairs in MLS memory as hex
    pub fn key_value_pairs_utf8(&self) -> String {
        let mut c = self.inner.lock();
        let key_values = dsl::openmls_key_value
            .select((dsl::key_bytes, dsl::value_bytes))
            .load::<(Vec<u8>, Vec<u8>)>(&mut *c)
            .unwrap();
        let mut s = String::new();
        s.push('\n');
        for (key, value) in key_values.iter() {
            write!(
                s,
                "{}:{}",
                String::from_utf8_lossy(key),
                String::from_utf8_lossy(value)
            )
            .unwrap();
            s.push('\n');
        }
        s
    }
}

impl ConnectionExt for MemoryStorage {
    fn raw_query_read<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        let mut c = self.inner.lock();
        Ok(fun(&mut c)?)
    }

    fn raw_query_write<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<T, diesel::result::Error>,
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
