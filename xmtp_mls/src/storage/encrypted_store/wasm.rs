//! WebAssembly specific connection for a SQLite Database
//! Stores a single connection behind a mutex that's used for every libxmtp operation
use diesel::prelude::SqliteConnection;
use diesel::{connection::AnsiTransactionManager, prelude::*};
use parking_lot::Mutex;
use std::sync::Arc;

use super::{db_connection::DbConnectionPrivate, StorageError, StorageOption, XmtpDb};

#[derive(Clone)]
pub struct WasmDb {
    conn: Arc<Mutex<SqliteConnection>>,
    opts: StorageOption,
    transaction_lock: Arc<Mutex<()>>,
}

impl std::fmt::Debug for WasmDb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmDb")
            .field("conn", &"WasmSqliteConnection")
            .field("opts", &self.opts)
            .finish()
    }
}

impl WasmDb {
    pub fn new(opts: &StorageOption) -> Result<Self, StorageError> {
        use super::StorageOption::*;
        let name = xmtp_common::rand_string::<12>();
        let name = format!("file:/xmtp-test-{}.db?vfs=memdb", name);
        let conn = match opts {
            Ephemeral => SqliteConnection::establish(name.as_str()),
            Persistent(ref db_path) => SqliteConnection::establish(db_path),
        }?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            opts: opts.clone(),
            transaction_lock: Arc::new(Mutex::new(())),
        })
    }
}

impl XmtpDb for WasmDb {
    type Connection = SqliteConnection;
    type TransactionManager = AnsiTransactionManager;

    fn conn(&self) -> Result<DbConnectionPrivate<Self::Connection>, StorageError> {
        Ok(DbConnectionPrivate::from_arc_mutex(
            self.conn.clone(),
            None,
            self.transaction_lock.clone(),
        ))
    }

    fn validate(&self, _opts: &StorageOption) -> Result<(), StorageError> {
        Ok(())
    }

    fn release_connection(&self) -> Result<(), StorageError> {
        Ok(())
    }

    fn reconnect(&self) -> Result<(), StorageError> {
        Ok(())
    }
}
