use std::sync::Arc;

use diesel::{connection::AnsiTransactionManager, prelude::*};
pub use diesel_wasm_sqlite::connection::WasmSqliteConnection as SqliteConnection;
use parking_lot::Mutex;

use super::{
    db_connection::DbConnectionPrivate, EncryptionKey, StorageError, StorageOption, XmtpDb,
};

#[derive(Clone)]
pub struct WasmDb {
    conn: Arc<Mutex<SqliteConnection>>,
    enc_key: Option<EncryptionKey>,
    opts: StorageOption,
}

impl std::fmt::Debug for WasmDb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmDb")
            .field("conn", &"WasmSqliteConnection")
            .field("enc_key", &self.enc_key)
            .field("opts", &self.opts)
            .finish()
    }
}

impl WasmDb {
    pub async fn new(
        opts: &StorageOption,
        enc_key: Option<EncryptionKey>,
    ) -> Result<Self, StorageError> {
        use super::StorageOption::*;
        diesel_wasm_sqlite::init_sqlite().await;
        let conn = match opts {
            Ephemeral => SqliteConnection::establish(":memory:"),
            Persistent(ref db_path) => SqliteConnection::establish(db_path),
        };
        Ok(Self {
            conn: Arc::new(Mutex::new(conn?)),
            opts: opts.clone(),
            enc_key,
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

    #[allow(unreachable_code)]
    fn release_connection(&self) -> Result<(), StorageError> {
        unimplemented!();
    }

    #[allow(unreachable_code)]
    fn reconnect(&self) -> Result<(), StorageError> {
        unimplemented!();
    }
}
