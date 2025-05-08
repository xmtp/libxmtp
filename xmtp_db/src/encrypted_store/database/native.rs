mod sqlcipher_connection;

use crate::TransactionGuard;
/// Native SQLite connection using SqlCipher
use crate::{ConnectionError, ConnectionExt, NotFound};
use diesel::connection::TransactionManager;
use diesel::r2d2::R2D2Connection;
use diesel::sqlite::SqliteConnection;
use diesel::{
    Connection,
    connection::{AnsiTransactionManager, SimpleConnection},
    r2d2::{self, CustomizeConnection, PooledConnection},
};
use parking_lot::{Mutex, RwLock};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use thiserror::Error;
use xmtp_common::RetryableError;

pub type ConnectionManager = r2d2::ConnectionManager<SqliteConnection>;
pub type Pool = r2d2::Pool<ConnectionManager>;
pub type RawDbConnection = PooledConnection<ConnectionManager>;

pub use self::sqlcipher_connection::EncryptedConnection;
use crate::{EncryptionKey, StorageOption, XmtpDb};

use super::PersistentOrMem;

trait XmtpConnection:
    ValidatedConnection + CustomizeConnection<SqliteConnection, r2d2::Error> + dyn_clone::DynClone
{
}

impl<T> XmtpConnection for T where
    T: ValidatedConnection
        + CustomizeConnection<SqliteConnection, r2d2::Error>
        + dyn_clone::DynClone
{
}

dyn_clone::clone_trait_object!(XmtpConnection);

pub(crate) trait ValidatedConnection {
    fn validate(&self, _opts: &StorageOption) -> Result<(), PlatformStorageError> {
        Ok(())
    }
}

/// An Unencrypted Connection
/// Creates a Sqlite3 Database/Connection in WAL mode.
/// Sets `busy_timeout` on each connection.
/// _*NOTE:*_Unencrypted Connections are not validated and mostly meant for testing.
/// It is not recommended to use an unencrypted connection in production.
#[derive(Clone, Debug)]
pub struct UnencryptedConnection;
impl ValidatedConnection for UnencryptedConnection {}

impl CustomizeConnection<SqliteConnection, r2d2::Error> for UnencryptedConnection {
    fn on_acquire(&self, conn: &mut SqliteConnection) -> Result<(), r2d2::Error> {
        conn.batch_execute("PRAGMA query_only = ON; PRAGMA busy_timeout = 5000;")
            .map_err(r2d2::Error::QueryError)?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct NopConnection;
impl ValidatedConnection for NopConnection {}
impl CustomizeConnection<SqliteConnection, r2d2::Error> for NopConnection {
    fn on_acquire(&self, _conn: &mut SqliteConnection) -> Result<(), r2d2::Error> {
        Ok(())
    }
}

impl StorageOption {
    // create a completely new standalone connection
    pub(super) fn conn(&self) -> Result<SqliteConnection, diesel::ConnectionError> {
        use StorageOption::*;
        match self {
            Persistent(path) => SqliteConnection::establish(path),
            Ephemeral => SqliteConnection::establish(":memory:"),
        }
    }

    pub(super) fn path(&self) -> Option<&String> {
        use StorageOption::*;
        match self {
            Persistent(path) => Some(path),
            _ => None,
        }
    }
}

#[derive(Debug, Error)]
pub enum PlatformStorageError {
    #[error("Pool error: {0}")]
    Pool(#[from] diesel::r2d2::PoolError),
    #[error("Error with connection to Sqlite {0}")]
    DbConnection(#[from] diesel::r2d2::Error),
    #[error("Pool needs to  reconnect before use")]
    PoolNeedsConnection,
    #[error("The SQLCipher Sqlite extension is not present, but an encryption key is given")]
    SqlCipherNotLoaded,
    #[error("PRAGMA key or salt has incorrect value")]
    SqlCipherKeyIncorrect,
    #[error(transparent)]
    DieselResult(#[from] diesel::result::Error),
    #[error(transparent)]
    NotFound(#[from] NotFound),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    FromHex(#[from] hex::FromHexError),
    #[error(transparent)]
    DieselConnect(#[from] diesel::ConnectionError),
    #[error(transparent)]
    Boxed(#[from] Box<dyn std::error::Error + Send + Sync>),
}

impl RetryableError for PlatformStorageError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::Pool(_) => true,
            Self::SqlCipherNotLoaded => true,
            Self::PoolNeedsConnection => true,
            Self::SqlCipherKeyIncorrect => false,

            _ => false,
        }
    }
}

#[derive(Clone, Debug)]
/// Database used in `native` (everywhere but web)
pub struct NativeDb {
    customizer: Box<dyn XmtpConnection>,
    conn: Arc<super::DefaultConnectionInner>,
}

impl NativeDb {
    /// This function is private so that an unencrypted database cannot be created by accident
    pub(crate) fn new(
        opts: &StorageOption,
        enc_key: Option<EncryptionKey>,
    ) -> Result<Self, PlatformStorageError> {
        let customizer = if let Some(key) = enc_key {
            let enc_connection = EncryptedConnection::new(key, opts)?;
            Box::new(enc_connection) as Box<dyn XmtpConnection>
        } else if matches!(opts, StorageOption::Persistent(_)) {
            Box::new(UnencryptedConnection) as Box<dyn XmtpConnection>
        } else {
            Box::new(NopConnection) as Box<dyn XmtpConnection>
        };
        customizer.validate(opts)?;

        let conn = match opts {
            StorageOption::Ephemeral => PersistentOrMem::Mem(EphemeralDbConnection::new()?),
            StorageOption::Persistent(path) => {
                PersistentOrMem::Persistent(NativeDbConnection::new(path, customizer.clone())?)
            }
        };

        Ok(Self {
            conn: conn.into(),
            customizer,
        })
    }
}

impl XmtpDb for NativeDb {
    type Connection = crate::DefaultConnection;
    type Error = PlatformStorageError;

    /// Returns the Wrapped [`super::db_connection::DbConnection`] Connection implementation for this Database
    fn conn(&self) -> Self::Connection {
        self.conn.clone()
    }

    fn validate(&self, opts: &StorageOption) -> Result<(), Self::Error> {
        self.customizer.validate(opts)
    }

    fn disconnect(&self) -> Result<(), Self::Error> {
        use PersistentOrMem::*;
        match self.conn.as_ref() {
            Persistent(p) => p.disconnect()?,
            Mem(m) => m.disconnect()?,
        };
        Ok(())
    }

    fn reconnect(&self) -> Result<(), Self::Error> {
        use PersistentOrMem::*;
        match self.conn.as_ref() {
            Persistent(p) => p.reconnect(),
            Mem(m) => m.reconnect(),
        }
    }
}

pub struct EphemeralDbConnection {
    conn: Arc<Mutex<SqliteConnection>>,
    in_transaction: Arc<AtomicBool>,
    global_transaction_lock: Arc<Mutex<()>>,
}

impl std::fmt::Debug for EphemeralDbConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "EphemeralConnection {{ in_transaction: {} }}",
            self.in_transaction.load(Ordering::Relaxed)
        )
    }
}

impl EphemeralDbConnection {
    fn new() -> Result<Self, PlatformStorageError> {
        Ok(Self {
            conn: Arc::new(Mutex::new(SqliteConnection::establish(":memory:")?)),
            in_transaction: Arc::new(AtomicBool::new(false)),
            global_transaction_lock: Arc::new(Mutex::new(())),
        })
    }

    fn disconnect(&self) -> Result<(), PlatformStorageError> {
        Ok(())
    }

    fn reconnect(&self) -> Result<(), PlatformStorageError> {
        let mut w = self.conn.lock();
        let conn = SqliteConnection::establish(":memory:")?;
        *w = conn;
        Ok(())
    }
}

impl ConnectionExt for EphemeralDbConnection {
    type Connection = SqliteConnection;
    type Error = ConnectionError;

    fn start_transaction(&self) -> Result<TransactionGuard<'_>, Self::Error> {
        let guard = self.global_transaction_lock.lock();
        let mut c = self.conn.lock();
        AnsiTransactionManager::begin_transaction(&mut *c)?;
        self.in_transaction.store(true, Ordering::SeqCst);

        Ok(TransactionGuard {
            _mutex_guard: guard,
            in_transaction: self.in_transaction.clone(),
        })
    }

    fn raw_query_read<T, F>(&self, fun: F) -> Result<T, Self::Error>
    where
        F: FnOnce(&mut Self::Connection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        let mut conn = self.conn.lock();
        fun(&mut conn).map_err(ConnectionError::from)
    }

    fn raw_query_write<T, F>(&self, fun: F) -> Result<T, Self::Error>
    where
        F: FnOnce(&mut Self::Connection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        let mut conn = self.conn.lock();
        fun(&mut conn).map_err(ConnectionError::from)
    }
}

pub struct NativeDbConnection {
    pub(super) read: Arc<RwLock<Option<Pool>>>,
    pub(super) write: Arc<Mutex<SqliteConnection>>,
    global_transaction_lock: Arc<Mutex<()>>,
    in_transaction: Arc<AtomicBool>,
    path: String,
    customizer: Box<dyn XmtpConnection>,
}

impl std::fmt::Debug for NativeDbConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "NativeDbConnection {{ path: {}, in_transaction: {} }}",
            &self.path,
            self.in_transaction.load(Ordering::Relaxed)
        )
    }
}

impl NativeDbConnection {
    fn new(path: &str, customizer: Box<dyn XmtpConnection>) -> Result<Self, PlatformStorageError> {
        let read = Pool::builder()
            .connection_customizer(customizer.clone())
            .max_size(crate::configuration::MAX_DB_POOL_SIZE)
            .build(ConnectionManager::new(path))?;

        let mut write = SqliteConnection::establish(path)?;
        customizer.on_acquire(&mut write)?;
        write.batch_execute("PRAGMA query_only = OFF;")?;
        write.batch_execute("PRAGMA journal_mode = WAL;")?;
        let write = Arc::new(Mutex::new(write));

        Ok(Self {
            read: Arc::new(RwLock::new(Some(read))),
            write,
            global_transaction_lock: Arc::new(Mutex::new(())),
            in_transaction: Arc::new(AtomicBool::new(false)),
            path: path.to_string(),
            customizer,
        })
    }

    fn disconnect(&self) -> Result<(), PlatformStorageError> {
        tracing::warn!("released sqlite database connection");
        let mut pool_guard = self.read.write();
        pool_guard.take();
        Ok(())
    }

    fn reconnect(&self) -> Result<(), PlatformStorageError> {
        tracing::info!("reconnecting sqlite database connection");
        let builder = Pool::builder().connection_customizer(self.customizer.clone());

        let mut pool = self.read.write();
        *pool = Some(
            builder
                .max_size(crate::configuration::MAX_DB_POOL_SIZE)
                .build(ConnectionManager::new(self.path.clone()))?,
        );

        let mut write = self.write.lock();
        if write.is_broken() {
            let mut new = SqliteConnection::establish(&self.path)?;
            self.customizer.on_acquire(&mut new)?;
            new.batch_execute("PRAGMA query_only = OFF;")?;
            *write = new;
        }
        Ok(())
    }
}

impl ConnectionExt for NativeDbConnection {
    type Connection = SqliteConnection;
    type Error = ConnectionError;

    fn start_transaction(&self) -> Result<crate::TransactionGuard<'_>, Self::Error> {
        let guard = self.global_transaction_lock.lock();
        let mut write = self.write.lock();
        AnsiTransactionManager::begin_transaction(&mut *write)?;
        self.in_transaction.store(true, Ordering::SeqCst);

        Ok(TransactionGuard {
            _mutex_guard: guard,
            in_transaction: self.in_transaction.clone(),
        })
    }

    #[tracing::instrument(level = "debug", skip_all)]
    fn raw_query_read<T, F>(&self, fun: F) -> Result<T, Self::Error>
    where
        F: FnOnce(&mut Self::Connection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        if self.in_transaction.load(Ordering::SeqCst) {
            tracing::debug!("read in transaction");
            let mut conn = self.write.lock();
            return fun(&mut conn).map_err(ConnectionError::from);
        } else if let Some(pool) = &*self.read.read() {
            tracing::trace!(
                "pulling connection from pool, idle={}, total={}",
                pool.state().idle_connections,
                pool.state().connections
            );
            let mut conn = pool.get().map_err(PlatformStorageError::from)?;

            return fun(&mut conn).map_err(ConnectionError::from);
        } else {
            return Err(ConnectionError::from(
                PlatformStorageError::PoolNeedsConnection,
            ));
        };
    }

    #[tracing::instrument(level = "debug", skip_all)]
    fn raw_query_write<T, F>(&self, fun: F) -> Result<T, Self::Error>
    where
        F: FnOnce(&mut Self::Connection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        let _guard;
        if !self.in_transaction.load(Ordering::SeqCst) {
            _guard = self.global_transaction_lock.lock();
        }
        let mut locked = self.write.lock();
        fun(&mut locked).map_err(ConnectionError::from)
    }
}

#[cfg(test)]
mod tests {
    use crate::EncryptedMessageStore;

    use super::*;
    use crate::{Fetch, Store, identity::StoredIdentity};
    use xmtp_common::{rand_vec, tmp_path};

    #[tokio::test]
    async fn releases_db_lock() {
        let db_path = tmp_path();
        {
            let store = EncryptedMessageStore::new(
                StorageOption::Persistent(db_path.clone()),
                EncryptedMessageStore::generate_enc_key(),
            )
            .await
            .unwrap();
            let conn = &store.conn().unwrap();

            let inbox_id = "inbox_id";
            StoredIdentity::new(inbox_id.to_string(), rand_vec::<24>(), rand_vec::<24>())
                .store(conn)
                .unwrap();

            let fetched_identity: StoredIdentity = conn.fetch(&()).unwrap().unwrap();

            assert_eq!(fetched_identity.inbox_id, inbox_id);

            store.release_connection().unwrap();
            if let PersistentOrMem::Persistent(p) = &*store.db.conn() {
                assert!(p.read.read().is_none())
            } else {
                panic!("conn expected")
            }
            store.reconnect().unwrap();
            let fetched_identity2: StoredIdentity = conn.fetch(&()).unwrap().unwrap();

            assert_eq!(fetched_identity2.inbox_id, inbox_id);
        }

        EncryptedMessageStore::remove_db_files(db_path)
    }

    #[tokio::test]
    async fn mismatched_encryption_key() {
        use crate::database::PlatformStorageError;
        let mut enc_key = [1u8; 32];

        let db_path = tmp_path();
        {
            // Setup a persistent store
            let store =
                EncryptedMessageStore::new(StorageOption::Persistent(db_path.clone()), enc_key)
                    .await
                    .unwrap();

            StoredIdentity::new(
                "dummy_address".to_string(),
                rand_vec::<24>(),
                rand_vec::<24>(),
            )
            .store(&store.conn().unwrap())
            .unwrap();
        } // Drop it
        enc_key[3] = 145; // Alter the enc_key
        let err = EncryptedMessageStore::new(StorageOption::Persistent(db_path.clone()), enc_key)
            .await
            .unwrap_err();
        // Ensure it fails
        assert!(
            matches!(
                err,
                crate::StorageError::Platform(PlatformStorageError::SqlCipherKeyIncorrect)
            ),
            "Expected SqlCipherKeyIncorrect error, got {}",
            err
        );
        EncryptedMessageStore::remove_db_files(db_path)
    }
}
