mod sqlcipher_connection;

/// Native SQLite connection using SqlCipher
use crate::{ConnectionExt, NotFound};
use crate::{StorageError, TransactionGuard};
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

use self::sqlcipher_connection::EncryptedConnection;
use crate::{EncryptionKey, StorageOption, XmtpDb};

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
    fn validate(&self, _opts: &StorageOption) -> Result<(), NativeStorageError> {
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
pub enum NativeStorageError {
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

impl RetryableError for NativeStorageError {
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

#[derive(Clone)]
/// Database used in `native` (everywhere but web)
pub struct NativeDb {
    customizer: Option<Box<dyn XmtpConnection>>,

    opts: StorageOption,
    conn: Arc<PersistentOrMem>,
}

pub enum PersistentOrMem {
    Persistent(NativeDbConnection),
    Mem(EphemeralDbConnection),
}

impl NativeDb {
    /// This function is private so that an unencrypted database cannot be created by accident
    pub(crate) fn new(
        opts: &StorageOption,
        enc_key: Option<EncryptionKey>,
    ) -> Result<Self, NativeStorageError> {
        let customizer = if let Some(key) = enc_key {
            let enc_connection = EncryptedConnection::new(key, opts)?;
            Some(Box::new(enc_connection) as Box<dyn XmtpConnection>)
        } else if matches!(opts, StorageOption::Persistent(_)) {
            Some(Box::new(UnencryptedConnection) as Box<dyn XmtpConnection>)
        } else {
            None
        };

        let conn = match opts {
            StorageOption::Ephemeral => PersistentOrMem::Mem(EphemeralDbConnection::new()?),
            StorageOption::Persistent(path) => {
                PersistentOrMem::Persistent(NativeDbConnection::new(path, customizer.clone())?)
            }
        };

        Ok(Self {
            conn: conn.into(),
            customizer,
            opts: opts.clone(),
        })
    }
}

impl ConnectionExt for PersistentOrMem {
    type Connection = SqliteConnection;

    fn start_transaction(&self) -> Result<TransactionGuard<'_>, StorageError> {
        match self {
            Self::Persistent(p) => p.start_transaction(),
            Self::Mem(m) => m.start_transaction(),
        }
    }

    fn raw_query_read<T, F, E>(&self, fun: F) -> Result<T, E>
    where
        F: FnOnce(&mut Self::Connection) -> Result<T, diesel::result::Error>,
        E: From<diesel::result::Error>,
        Self: Sized,
    {
        match self {
            Self::Persistent(p) => p.raw_query_read(fun),
            Self::Mem(m) => m.raw_query_read(fun),
        }
    }

    fn raw_query_write<T, F, E>(&self, fun: F) -> Result<T, E>
    where
        F: FnOnce(&mut Self::Connection) -> Result<T, diesel::result::Error>,
        E: From<diesel::result::Error>,
        Self: Sized,
    {
        match self {
            Self::Persistent(p) => p.raw_query_write(fun),
            Self::Mem(m) => m.raw_query_write(fun),
        }
    }
}

impl XmtpDb for NativeDb {
    type Connection = PersistentOrMem;
    type Error = NativeStorageError;

    /// Returns the Wrapped [`super::db_connection::DbConnection`] Connection implementation for this Database
    fn conn(&self) -> Result<&Self::Connection, Self::Error> {
        Ok(&self.conn)
    }

    fn validate(&self, opts: &StorageOption) -> Result<(), Self::Error> {
        if let Some(c) = &self.customizer {
            c.validate(opts)
        } else {
            Ok(())
        }
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
    global_lock: Arc<Mutex<()>>,
}

impl EphemeralDbConnection {
    fn new() -> Result<Self, NativeStorageError> {
        Ok(Self {
            conn: Arc::new(Mutex::new(SqliteConnection::establish(":memory:")?)),
            in_transaction: Arc::new(AtomicBool::new(false)),
            global_lock: Arc::new(Mutex::new(())),
        })
    }

    fn disconnect(&self) -> Result<(), NativeStorageError> {
        Ok(())
    }

    fn reconnect(&self) -> Result<(), NativeStorageError> {
        let mut w = self.conn.lock();
        let conn = SqliteConnection::establish(":memory:")?;
        *w = conn;
        Ok(())
    }
}

impl ConnectionExt for EphemeralDbConnection {
    type Connection = SqliteConnection;

    fn start_transaction(&self) -> Result<TransactionGuard<'_>, StorageError> {
        let guard = self.global_lock.lock();
        let mut c = self.conn.lock();
        AnsiTransactionManager::begin_transaction(&mut *c)?;
        self.in_transaction.store(true, Ordering::SeqCst);

        Ok(TransactionGuard {
            _mutex_guard: guard,
            in_transaction: self.in_transaction.clone(),
        })
    }

    fn raw_query_read<T, F, E>(&self, fun: F) -> Result<T, E>
    where
        F: FnOnce(&mut Self::Connection) -> Result<T, diesel::result::Error>,
        Self: Sized,
        E: From<diesel::result::Error>,
    {
        let mut conn = self.conn.lock();
        return fun(&mut *conn).map_err(E::from);
    }

    fn raw_query_write<T, F, E>(&self, fun: F) -> Result<T, E>
    where
        F: FnOnce(&mut Self::Connection) -> Result<T, diesel::result::Error>,
        E: From<diesel::result::Error>,
        Self: Sized,
    {
        let mut conn = self.conn.lock();
        return fun(&mut *conn).map_err(E::from);
    }
}

pub struct NativeDbConnection {
    pub(super) read: Arc<RwLock<Option<Pool>>>,
    pub(super) write: Arc<Mutex<SqliteConnection>>,
    transaction_lock: Arc<Mutex<()>>,
    global_lock: Arc<Mutex<()>>,
    in_transaction: Arc<AtomicBool>,
    path: String,
    customizer: Option<Box<dyn XmtpConnection>>,
}

impl NativeDbConnection {
    fn new(
        path: &str,
        customizer: Option<Box<dyn XmtpConnection>>,
    ) -> Result<Self, NativeStorageError> {
        let builder = Pool::builder();
        let builder = if let Some(ref c) = customizer {
            builder.connection_customizer(c.clone())
        } else {
            builder
        };
        let read = builder
            .max_size(crate::configuration::MAX_DB_POOL_SIZE)
            .build(ConnectionManager::new(path))?;

        let mut write = SqliteConnection::establish(path)?;
        write.batch_execute("PRAGMA query_only = OFF;")?;
        let write = Arc::new(Mutex::new(write));

        Ok(Self {
            read: Arc::new(RwLock::new(Some(read))),
            write,
            transaction_lock: Arc::new(Mutex::new(())),
            global_lock: Arc::new(Mutex::new(())),
            in_transaction: Arc::new(AtomicBool::new(false)),
            path: path.to_string(),
            customizer,
        })
    }

    fn disconnect(&self) -> Result<(), NativeStorageError> {
        tracing::warn!("released sqlite database connection");
        let mut pool_guard = self.read.write();
        pool_guard.take();
        Ok(())
    }

    fn reconnect(&self) -> Result<(), NativeStorageError> {
        tracing::info!("reconnecting sqlite database connection");
        let mut builder = Pool::builder();

        if let Some(ref c) = self.customizer {
            builder = builder.connection_customizer(c.clone());
        }

        let mut pool = self.read.write();
        *pool = Some(
            builder
                .max_size(crate::configuration::MAX_DB_POOL_SIZE)
                .build(ConnectionManager::new(self.path.clone()))?,
        );

        let mut write = self.write.lock();
        if write.is_broken() {
            let mut new = SqliteConnection::establish(&self.path)?;
            new.batch_execute("PRAGMA query_only = OFF;")?;
            *write = new;
        }
        Ok(())
    }
}

impl ConnectionExt for NativeDbConnection {
    type Connection = SqliteConnection;

    fn start_transaction(&self) -> Result<crate::TransactionGuard<'_>, crate::StorageError> {
        let guard = self.global_lock.lock();
        let mut write = self.write.lock();
        AnsiTransactionManager::begin_transaction(&mut *write)?;
        self.in_transaction.store(true, Ordering::SeqCst);

        Ok(TransactionGuard {
            _mutex_guard: guard,
            in_transaction: self.in_transaction.clone(),
        })
    }

    fn raw_query_read<T, F, E>(&self, fun: F) -> Result<T, E>
    where
        F: FnOnce(&mut Self::Connection) -> Result<T, diesel::result::Error>,
        E: From<diesel::result::Error>,
        Self: Sized,
    {
        if self.in_transaction.load(Ordering::SeqCst) {
            let mut lock = self.write.lock();
            return fun(&mut *lock);
        }

        // TODO: Question: insipx: why were reads in a lock before? If we're writing something it should
        // use the write lock, why would we need to protect reads?
        if let Some(pool) = &*self.read.read() {
            tracing::trace!(
                "pulling connection from pool, idle={}, total={}",
                pool.state().idle_connections,
                pool.state().connections
            );
            let mut conn = pool.get().map_err(NativeStorageError::from)?;
            return fun(&mut *conn).map_err(E::from);
        } else {
            return Err(NativeStorageError::PoolNeedsConnection);
        }
    }

    fn raw_query_write<T, F, E>(&self, fun: F) -> Result<T, E>
    where
        F: FnOnce(&mut Self::Connection) -> Result<T, diesel::result::Error>,
        E: From<diesel::result::Error>,
        Self: Sized,
    {
        let _guard;
        if !self.in_transaction.load(Ordering::SeqCst) {
            _guard = self.global_lock.lock();
        }
        fun(&mut self.write.lock()).map_err(E::from)
    }
}
