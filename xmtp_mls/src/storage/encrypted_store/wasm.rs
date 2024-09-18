use std::sync::Arc;

use diesel::{
    connection::{AnsiTransactionManager, TransactionManager},
    prelude::*,
    result::Error,
};
pub use diesel_wasm_sqlite::{
    connection::WasmSqliteConnection as SqliteConnection
};
use parking_lot::Mutex;

use super::db_connection::DbConnection;

use super::{EncryptionKey, StorageError, StorageOption, XmtpDb, db_connection::DbConnectionPrivate};
use crate::xmtp_openmls_provider::XmtpOpenMlsProvider;

pub type RawDbConnection = Arc<Mutex<SqliteConnection>>;

#[derive(Clone)]
pub struct WasmDb {
    conn: Arc<Mutex<SqliteConnection>>,
    enc_key: Option<EncryptionKey>,
    opts: StorageOption,
}

impl std::fmt::Debug for WasmDb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmDb").field("conn", &"WasmSqliteConnection").field("enc_key", &self.enc_key).field("opts", &self.opts).finish()
    }
}

impl WasmDb {
    pub fn new(opts: &StorageOption, enc_key: Option<EncryptionKey>) -> Result<Self, StorageError> {
        use super::StorageOption::*;
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

    fn conn(&self) -> Result<DbConnectionPrivate<Self::Connection>, StorageError> {
        Ok(DbConnectionPrivate::from_arc_mutex(self.conn.clone()))
    }

    fn validate(&self, _opts: &StorageOption) -> Result<(), StorageError> {
        Ok(())
    }

    fn transaction<T, F, E>(&self, fun: F) -> Result<T, E>
    where
        F: FnOnce(&crate::xmtp_openmls_provider::XmtpOpenMlsProvider) -> Result<T, E>,
        E: From<diesel::result::Error> + From<StorageError>,
    {
        log::debug!("Transaction beginning");
        {
            let mut connection = self.conn.lock();
            AnsiTransactionManager::begin_transaction(&mut *connection)?;
        }

        let db_connection = self.conn()?;
        let provider = XmtpOpenMlsProvider::new(db_connection);
        let conn = provider.conn_ref();

        match fun(&provider) {
            Ok(value) => {
                conn.raw_query(|conn| AnsiTransactionManager::commit_transaction(&mut *conn))?;
                log::debug!("Transaction being committed");
                Ok(value)
            }
            Err(err) => {
                log::debug!("Transaction being rolled back");
                match conn
                    .raw_query(|conn| AnsiTransactionManager::rollback_transaction(&mut *conn))
                {
                    Ok(()) => Err(err),
                    Err(Error::BrokenTransactionManager) => Err(err),
                    Err(rollback) => Err(rollback.into()),
                }
            }
        }
    }

    async fn transaction_async<T, F, E, Fut>(&self, fun: F) -> Result<T, E>
    where
        F: FnOnce(crate::xmtp_openmls_provider::XmtpOpenMlsProvider) -> Fut,
        Fut: futures::Future<Output = Result<T, E>>,
        E: From<diesel::result::Error> + From<StorageError>,
    {
        log::debug!("Transaction async beginning");
        let local_connection = self.conn.clone();
        {
            let mut local_connection = local_connection.lock();
            AnsiTransactionManager::begin_transaction(&mut *local_connection)?;
        }
        let db_connection = self.conn()?;
        let provider = XmtpOpenMlsProvider::new(db_connection);

        // the other connection is dropped in the closure
        // ensuring we have only one strong reference
        let result = fun(provider).await;
        if Arc::strong_count(&local_connection) > 1 {
            log::warn!("More than 1 strong connection references still exist during transaction");
        }

        if Arc::weak_count(&local_connection) > 1 {
            log::warn!("More than 1 weak connection references still exist during transaction");
        }

        // after the closure finishes, `local_provider` should have the only reference ('strong')
        // to `XmtpOpenMlsProvider` inner `DbConnection`..
        let local_connection = DbConnection::from_arc_mutex(local_connection);
        match result {
            Ok(value) => {
                local_connection
                    .raw_query(|conn| AnsiTransactionManager::commit_transaction(&mut *conn))?;
                log::debug!("Transaction async being committed");
                Ok(value)
            }
            Err(err) => {
                log::debug!("Transaction async being rolled back");
                match local_connection
                    .raw_query(|conn| AnsiTransactionManager::rollback_transaction(&mut *conn))
                {
                    Ok(()) => Err(err),
                    Err(Error::BrokenTransactionManager) => Err(err),
                    Err(rollback) => Err(rollback.into()),
                }
            }
        }
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
