/// Native SQLite connection using SqlCipher
use crate::storage::encrypted_store::DbConnectionPrivate;
use crate::storage::StorageError;
use diesel::sqlite::SqliteConnection;
use diesel::{
    connection::{AnsiTransactionManager, SimpleConnection},
    r2d2::{self, CustomizeConnection, PoolTransactionManager, PooledConnection},
    Connection,
};
use parking_lot::{Mutex, RwLock};
use std::sync::Arc;

pub type ConnectionManager = r2d2::ConnectionManager<SqliteConnection>;
pub type Pool = r2d2::Pool<ConnectionManager>;
pub type PoolBuilder = r2d2::Builder<ConnectionManager>;
pub type RawDbConnection = PooledConnection<ConnectionManager>;

use super::{sqlcipher_connection::EncryptedConnection, EncryptionKey, StorageOption, XmtpDb};

pub(crate) trait ValidatedConnection {
    fn validate(&self, _opts: &StorageOption) -> Result<(), StorageError> {
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

#[derive(Clone, Debug)]
pub enum ConnectionType {
    Unencrypted(UnencryptedConnection),
    Encrypted(EncryptedConnection),
}

impl ConnectionType {
    fn reinit(self, opts: &StorageOption) -> Result<Self, StorageError> {
        Ok(match self {
            Self::Encrypted(c) => Self::Encrypted(c.reinit(opts)?),
            Self::Unencrypted(_) => Self::Unencrypted(UnencryptedConnection),
        })
    }
}

impl ValidatedConnection for ConnectionType {
    fn validate(&self, opts: &StorageOption) -> Result<(), StorageError> {
        match self {
            Self::Unencrypted(u) => u.validate(opts),
            Self::Encrypted(e) => e.validate(opts),
        }
    }
}

impl CustomizeConnection<SqliteConnection, r2d2::Error> for ConnectionType {
    fn on_acquire(&self, conn: &mut SqliteConnection) -> Result<(), r2d2::Error> {
        match self {
            Self::Unencrypted(u) => u.on_acquire(conn),
            Self::Encrypted(e) => e.on_acquire(conn),
        }
    }

    fn on_release(&self, conn: SqliteConnection) {
        match self {
            Self::Unencrypted(u) => u.on_release(conn),
            Self::Encrypted(e) => e.on_release(conn),
        }
    }
}

#[derive(Clone)]
/// Database used in `native` (everywhere but web)
pub struct NativeDb {
    pub(super) pool: Arc<RwLock<Option<Pool>>>,
    pub(super) write_conn: Arc<Mutex<Option<RawDbConnection>>>,
    // ref to global transaction lock
    transaction_lock: Arc<Mutex<()>>,
    customizer: Arc<Mutex<Option<ConnectionType>>>,
    opts: StorageOption,
}

impl NativeDb {
    /// This function is private so that an unencrypted database cannot be created by accident
    pub(super) fn new(
        opts: &StorageOption,
        enc_key: Option<EncryptionKey>,
    ) -> Result<Self, StorageError> {
        let mut builder = Pool::builder();
        let customizer = Self::new_customizer(opts, enc_key)?;
        if let Some(ref c) = customizer {
            builder = builder.connection_customizer(Box::new(c.clone()));
        }
        let (pool, write_conn) = Self::new_pool(opts, builder)?;

        Ok(Self {
            pool: Arc::new(Some(pool).into()),
            write_conn: Some(Arc::new(Mutex::new(write_conn))),
            transaction_lock: Arc::new(Mutex::new(())),
            customizer: Arc::new(Mutex::new(customizer)),
            opts: opts.clone(),
        })
    }

    fn new_customizer(
        opts: &StorageOption,
        enc_key: Option<EncryptionKey>,
    ) -> Result<Option<ConnectionType>, StorageError> {
        if let Some(key) = enc_key {
            let enc_connection = EncryptedConnection::new(key, opts)?;
            Ok(Some(ConnectionType::Encrypted(enc_connection)))
        } else if matches!(opts, StorageOption::Persistent(_)) {
            Ok(Some(ConnectionType::Unencrypted(UnencryptedConnection)))
        } else {
            Ok(None)
        }
    }

    fn new_pool(
        opts: &StorageOption,
        builder: PoolBuilder,
    ) -> Result<(Pool, RawDbConnection), StorageError> {
        let pool = match opts {
            StorageOption::Ephemeral => builder
                .max_size(1)
                .build(ConnectionManager::new(":memory:"))?,
            StorageOption::Persistent(ref path) => builder
                .max_size(crate::configuration::MAX_DB_POOL_SIZE)
                .build(ConnectionManager::new(path))?,
        };

        // Take one of the connections and use it as the only writer.
        let mut write_conn = pool.get()?;
        write_conn.batch_execute("PRAGMA query_only = OFF;")?;
        Ok((pool, write_conn))
    }

    fn raw_conn(&self) -> Result<RawDbConnection, StorageError> {
        let pool_guard = self.pool.read();

        let pool = pool_guard
            .as_ref()
            .ok_or(StorageError::PoolNeedsConnection)?;

        tracing::trace!(
            "pulling connection from pool, idle={}, total={}",
            pool.state().idle_connections,
            pool.state().connections
        );

        Ok(pool.get()?)
    }

    fn raw_write_conn(&self) -> Result<Arc<Mutex<RawDbConnection>>, StorageError> {
        let write_conn = self
            .write_conn
            .lock();
            .as_ref()
            .ok_or(StorageError::PoolNeedsConnection)?;
        Ok(write_conn.clone())
    }
}

impl XmtpDb for NativeDb {
    type Connection = RawDbConnection;
    type TransactionManager = PoolTransactionManager<AnsiTransactionManager>;

    /// Returns the Wrapped [`super::db_connection::DbConnection`] Connection implementation for this Database
    fn conn(&self) -> Result<DbConnectionPrivate<Self::Connection>, StorageError> {
        let conn = match self.opts {
            StorageOption::Ephemeral => None,
            StorageOption::Persistent(_) => Some(self.raw_conn()?),
        };

        Ok(DbConnectionPrivate::from_arc_mutex(
            self.raw_write_conn()?,
            conn.map(|conn| Arc::new(parking_lot::Mutex::new(conn))),
            self.transaction_lock.clone(),
        ))
    }

    fn validate(&self, opts: &StorageOption) -> Result<(), StorageError> {
        if let Some(c) = &*self.customizer.lock() {
            c.validate(opts)
        } else {
            Ok(())
        }
    }

    fn release_connection(&self) -> Result<(), StorageError> {
        tracing::warn!("released sqlite database connection");
        let transaction_lock = self.transaction_lock.lock();
        let write_conn = self.write_conn.take();
        let mut pool_guard = self.pool.write();
        pool_guard.take();
        Ok(())
    }

    fn reconnect(&self) -> Result<(), StorageError> {
        tracing::info!("reconnecting sqlite database connection");
        {
            let mut transaction_lock = self.transaction_lock.lock();
            std::mem::swap(&mut *transaction_lock, &mut ())
        }

        {
            let mut customizer = self.customizer.lock();
            if let Some(ref mut old_c) = *customizer {
                let mut new_c = old_c.clone().reinit(&self.opts)?;
                std::mem::swap(old_c, &mut new_c);
            }
        }
        let builder = Pool::builder();
        let (mut new_pool, mut new_write_conn) = Self::new_pool(&self.opts, builder)?;
        {
            let mut pool_lock = self.pool.write();
            *pool_lock = Some(new_pool);
            // pool_lock.map(|mut p| std::mem::swap(&mut p, &mut new_pool));
        }
        {
            self.write_conn.as_ref().map(|w| *w.lock() = new_write_conn);
            // std::mem::swap(&mut *write_conn, &mut new_write_conn);
        }
        Ok(())
    }
}
