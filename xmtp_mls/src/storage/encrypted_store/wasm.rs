//! WebAssembly specific connection for a SQLite Database
//! Stores a single connection behind a mutex that's used for every libxmtp operation
use diesel::{connection::AnsiTransactionManager, prelude::*};
use parking_lot::Mutex;
pub use sqlite_web::connection::WasmSqliteConnection as SqliteConnection;
use std::sync::Arc;

use super::{db_connection::DbConnectionPrivate, StorageError, StorageOption, XmtpDb};

#[derive(Clone)]
pub struct WasmDb {
    conn: Arc<Mutex<SqliteConnection>>,
    opts: StorageOption,
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
    pub async fn new(opts: &StorageOption) -> Result<Self, StorageError> {
        use super::StorageOption::*;
        sqlite_web::init_sqlite().await;
        let conn = match opts {
            Ephemeral => SqliteConnection::establish(":memory:"),
            Persistent(ref db_path) => SqliteConnection::establish(db_path),
        }?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            opts: opts.clone(),
        })
    }
}

impl XmtpDb for WasmDb {
    type Connection = SqliteConnection;
    type TransactionManager = AnsiTransactionManager;

    fn conn(&self) -> Result<DbConnectionPrivate<Self::Connection>, StorageError> {
        Ok(DbConnectionPrivate::from_arc_mutex(self.conn.clone()))
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
