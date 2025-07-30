mod pool;
mod sqlcipher_connection;

use crate::StorageError;
use crate::configuration::BUSY_TIMEOUT;
use crate::database::instrumentation::TestInstrumentation;
/// Native SQLite connection using SqlCipher
use crate::{ConnectionError, ConnectionExt, DbConnection, NotFound};
use arc_swap::ArcSwapOption;
use diesel::sqlite::SqliteConnection;
use diesel::{
    Connection,
    connection::SimpleConnection,
    r2d2::{self, CustomizeConnection, PooledConnection},
};
use parking_lot::Mutex;
use std::sync::Arc;
use thiserror::Error;
use xmtp_common::{RetryableError, retryable};

use pool::*;

pub type RawDbConnection = PooledConnection<ConnectionManager>;

pub use self::sqlcipher_connection::EncryptedConnection;
use crate::{EncryptionKey, StorageOption, XmtpDb};

use super::PersistentOrMem;

trait XmtpConnection:
    ValidatedConnection
    + ConnectionOptions
    + CustomizeConnection<SqliteConnection, r2d2::Error>
    + dyn_clone::DynClone
{
}

trait ConnectionOptions {
    fn options(&self) -> &StorageOption;
    fn is_persistent(&self) -> bool {
        matches!(self.options(), StorageOption::Persistent(_))
    }
}

impl<T> XmtpConnection for T where
    T: ValidatedConnection
        + CustomizeConnection<SqliteConnection, r2d2::Error>
        + ConnectionOptions
        + dyn_clone::DynClone
{
}

dyn_clone::clone_trait_object!(XmtpConnection);

pub(crate) trait ValidatedConnection {
    fn validate(
        &self,
        _opts: &StorageOption,
        _conn: &mut SqliteConnection,
    ) -> Result<(), PlatformStorageError> {
        Ok(())
    }
}

/// Pragmas to execute on acquiring a new SQLite connection
/// According to [pragmas](https://docs.rs/diesel/latest/diesel/prelude/struct.SqliteConnection.html#concurrency)
/// for concurrency
/// these pragmas only required to be ran once per session.
fn connection_pragmas(c: &mut impl SimpleConnection) -> diesel::result::QueryResult<()> {
    // pragmas must be in a separate call to ensure they apply correctly
    // _NOTE:_ order is important to ensure later pragmas do not timeout
    c.batch_execute(&format!("PRAGMA busy_timeout = {};", BUSY_TIMEOUT))?; // sleep for 5s if the database is busy
    c.batch_execute("PRAGMA synchronous = NORMAL;")?; // fsync only in critical moments
    c.batch_execute("PRAGMA wal_autocheckpoint = 1000;")?; // write WAL changes back every 1000 pages, for an in average 1MB WAL file. May affect readers if number is increased
    c.batch_execute("PRAGMA wal_checkpoint(TRUNCATE);")?; // free some space by truncating possibly massive WAL files from the last run.
    c.batch_execute("PRAGMA query_only = OFF;")?; // Enable writing with the connection
    c.batch_execute("PRAGMA journal_size_limit = 67108864")?; // maximum size of the WAL file, corresponds to 64MB
    c.batch_execute("PRAGMA mmap_size = 134217728")?; // maximum size of the internal mmap pool. Corresponds to 128MB
    c.batch_execute("PRAGMA cache_size = 2000")?; // maximum number of database disk pages that will be hold in memory. Corresponds to ~8MB
    c.batch_execute("PRAGMA foreign_keys = ON;")?; // enforce foreign keys

    Ok(())
}

/// An Unencrypted Connection
/// Creates a Sqlite3 Database/Connection in WAL mode.
/// _*NOTE:*_Unencrypted Connections are not validated and mostly meant for testing.
/// It is not recommended to use an unencrypted connection in production.
#[derive(Clone, Debug)]
pub struct UnencryptedConnection {
    options: StorageOption,
}

impl UnencryptedConnection {
    pub fn new(options: StorageOption) -> Self {
        Self { options }
    }
}

impl ValidatedConnection for UnencryptedConnection {}

impl ConnectionOptions for UnencryptedConnection {
    fn options(&self) -> &StorageOption {
        &self.options
    }
}

impl CustomizeConnection<SqliteConnection, r2d2::Error> for UnencryptedConnection {
    fn on_acquire(&self, c: &mut SqliteConnection) -> Result<(), r2d2::Error> {
        if cfg!(any(test, feature = "test-utils")) {
            c.set_instrumentation(TestInstrumentation);
        }
        connection_pragmas(c)?;
        Ok(())
    }
}

impl ConnectionOptions for NopConnection {
    fn options(&self) -> &StorageOption {
        &self.options
    }
}

#[derive(Clone, Debug)]
pub struct NopConnection {
    options: StorageOption,
}

impl Default for NopConnection {
    fn default() -> Self {
        NopConnection {
            options: StorageOption::Ephemeral,
        }
    }
}

impl ValidatedConnection for NopConnection {}
impl CustomizeConnection<SqliteConnection, r2d2::Error> for NopConnection {
    fn on_acquire(&self, c: &mut SqliteConnection) -> Result<(), r2d2::Error> {
        if cfg!(any(test, feature = "test-utils")) {
            c.set_instrumentation(TestInstrumentation);
        }
        Ok(())
    }
}

impl StorageOption {
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
    #[error("Using a DB Pool requires a persistent path")]
    PoolRequiresPath,
    #[error("The SQLCipher Sqlite extension is not present, but an encryption key is given")]
    SqlCipherNotLoaded,
    #[error("PRAGMA key or salt has incorrect value")]
    SqlCipherKeyIncorrect,
    #[error("Database is locked")]
    DatabaseLocked,
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
            Self::DatabaseLocked => true,
            Self::DieselResult(result) => retryable!(result),
            Self::Io(_) => true,
            Self::DieselConnect(_) => true,

            _ => false,
        }
    }
}

#[derive(Clone, Debug)]
/// Database used in `native` (everywhere but web)
pub struct NativeDb {
    customizer: Box<dyn XmtpConnection>,
    conn: Arc<PersistentOrMem<NativeDbConnection, EphemeralDbConnection>>,
    opts: StorageOption,
}

impl NativeDb {
    pub fn new(opts: &StorageOption, enc_key: EncryptionKey) -> Result<Self, StorageError> {
        Self::new_inner(opts, Some(enc_key)).map_err(Into::into)
    }

    pub fn new_unencrypted(opts: &StorageOption) -> Result<Self, StorageError> {
        Self::new_inner(opts, None).map_err(Into::into)
    }

    /// This function is private so that an unencrypted database cannot be created by accident
    fn new_inner(
        opts: &StorageOption,
        enc_key: Option<EncryptionKey>,
    ) -> Result<Self, PlatformStorageError> {
        let customizer = if let Some(key) = enc_key {
            let enc_connection = EncryptedConnection::new(key, opts)?;
            Box::new(enc_connection) as Box<dyn XmtpConnection>
        } else if matches!(opts, StorageOption::Persistent(_)) {
            Box::new(UnencryptedConnection::new(opts.clone())) as Box<dyn XmtpConnection>
        } else {
            Box::new(NopConnection::default()) as Box<dyn XmtpConnection>
        };
        // customizer.validate(opts)?;

        let conn = if customizer.is_persistent() {
            PersistentOrMem::Persistent(NativeDbConnection::new(customizer.clone())?)
        } else {
            PersistentOrMem::Mem(EphemeralDbConnection::new()?)
        };

        Ok(Self {
            opts: opts.clone(),
            conn: conn.into(),
            customizer,
        })
    }
}

impl XmtpDb for NativeDb {
    type Connection = Arc<PersistentOrMem<NativeDbConnection, EphemeralDbConnection>>;
    type DbQuery = DbConnection<Self::Connection>;

    fn conn(&self) -> Self::Connection {
        self.conn.clone()
    }

    fn db(&self) -> Self::DbQuery {
        DbConnection::new(self.conn.clone())
    }

    fn opts(&self) -> &StorageOption {
        &self.opts
    }

    fn validate(
        &self,
        opts: &StorageOption,
        conn: &mut SqliteConnection,
    ) -> Result<(), ConnectionError> {
        self.customizer.validate(opts, conn)?;
        Ok(())
    }

    fn disconnect(&self) -> Result<(), ConnectionError> {
        self.conn.disconnect()
    }

    fn reconnect(&self) -> Result<(), ConnectionError> {
        self.conn.reconnect()
    }
}

pub struct EphemeralDbConnection {
    conn: Arc<Mutex<SqliteConnection>>,
}

impl std::fmt::Debug for EphemeralDbConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "EphemeralConnection {{ is_locked={} }}",
            self.conn.is_locked()
        )
    }
}

impl EphemeralDbConnection {
    pub fn new() -> Result<Self, PlatformStorageError> {
        let mut c = SqliteConnection::establish(":memory:")?;
        UnencryptedConnection::on_acquire(
            &UnencryptedConnection::new(StorageOption::Ephemeral),
            &mut c,
        )?;
        Ok(Self {
            conn: Arc::new(Mutex::new(c)),
        })
    }

    fn db_disconnect(&self) -> Result<(), PlatformStorageError> {
        Ok(())
    }

    fn db_reconnect(&self) -> Result<(), PlatformStorageError> {
        let mut w = self.conn.lock();
        let conn = SqliteConnection::establish(":memory:")?;
        *w = conn;
        Ok(())
    }
}

impl ConnectionExt for EphemeralDbConnection {
    fn raw_query_read<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        let mut conn = self.conn.lock();
        fun(&mut conn).map_err(ConnectionError::from)
    }

    fn raw_query_write<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        let mut conn = self.conn.lock();
        fun(&mut conn).map_err(ConnectionError::from)
    }

    fn disconnect(&self) -> Result<(), crate::ConnectionError> {
        Ok(self.db_disconnect()?)
    }

    fn reconnect(&self) -> Result<(), crate::ConnectionError> {
        Ok(self.db_reconnect()?)
    }
}

pub struct NativeDbConnection {
    pub(super) pool: ArcSwapOption<DbPool>,
    customizer: Box<dyn XmtpConnection>,
}

impl std::fmt::Debug for NativeDbConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "NativeDbConnection {{ path: {}, state={:?} }}",
            &self.customizer.options(),
            self.pool.load().as_ref().map(|s| s.state()),
        )
    }
}

impl NativeDbConnection {
    fn new(customizer: Box<dyn XmtpConnection>) -> Result<Self, PlatformStorageError> {
        Ok(Self {
            pool: ArcSwapOption::new(Some(Arc::new(DbPool::new(customizer.clone())?))),
            customizer,
        })
    }

    fn db_disconnect(&self) -> Result<(), PlatformStorageError> {
        tracing::warn!("released sqlite database connection");
        self.pool.store(None);
        Ok(())
    }

    fn db_reconnect(&self) -> Result<(), PlatformStorageError> {
        tracing::info!("reconnecting sqlite database connection");
        self.pool
            .store(Some(Arc::new(DbPool::new(self.customizer.clone())?)));
        Ok(())
    }
}

impl ConnectionExt for NativeDbConnection {
    #[tracing::instrument(level = "trace", skip_all)]
    fn raw_query_read<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        if let Some(pool) = &*self.pool.load() {
            tracing::trace!(
                "pulling connection from pool, idle={}, total={}",
                pool.state().idle_connections,
                pool.state().connections
            );
            let mut conn = pool.get().map_err(PlatformStorageError::from)?;
            fun(&mut conn).map_err(ConnectionError::from)
        } else {
            Err(ConnectionError::from(
                PlatformStorageError::PoolNeedsConnection,
            ))
        }
    }

    #[tracing::instrument(level = "trace", skip_all)]
    fn raw_query_write<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        if let Some(pool) = &*self.pool.load() {
            tracing::trace!(
                "pulling connection from pool for write, idle={}, total={}",
                pool.state().idle_connections,
                pool.state().connections
            );
            let mut conn = pool.get().map_err(PlatformStorageError::from)?;
            fun(&mut conn).map_err(ConnectionError::from)
        } else {
            Err(ConnectionError::from(
                PlatformStorageError::PoolNeedsConnection,
            ))
        }
    }

    fn disconnect(&self) -> Result<(), ConnectionError> {
        Ok(self.db_disconnect()?)
    }

    fn reconnect(&self) -> Result<(), ConnectionError> {
        Ok(self.db_reconnect()?)
    }
}

#[cfg(test)]
mod tests {
    use crate::{EncryptedMessageStore, XmtpTestDb};

    use super::*;
    use crate::{Fetch, Store, identity::StoredIdentity};
    use xmtp_common::{rand_vec, tmp_path};

    #[tokio::test]
    async fn releases_db_lock() {
        let db_path = tmp_path();
        {
            let store = crate::TestDb::create_persistent_store(Some(db_path.clone())).await;
            let conn = &store.conn();

            let inbox_id = "inbox_id";
            StoredIdentity::new(inbox_id.to_string(), rand_vec::<24>(), rand_vec::<24>())
                .store(conn)
                .unwrap();

            let fetched_identity: StoredIdentity = conn.fetch(&()).unwrap().unwrap();

            assert_eq!(fetched_identity.inbox_id, inbox_id);

            store.release_connection().unwrap();
            if let PersistentOrMem::Persistent(p) = &*store.db.conn() {
                assert!(p.pool.load().is_none())
            } else {
                panic!("conn expected")
            }
            store.reconnect().unwrap();
            let fetched_identity2: StoredIdentity = conn.fetch(&()).unwrap().unwrap();

            assert_eq!(fetched_identity2.inbox_id, inbox_id);
        }

        EncryptedMessageStore::<()>::remove_db_files(db_path)
    }

    #[tokio::test]
    async fn mismatched_encryption_key() {
        use crate::database::PlatformStorageError;
        let mut enc_key = [1u8; 32];

        let db_path = tmp_path();
        let opts = StorageOption::Persistent(db_path.clone());

        {
            let db = NativeDb::new(&opts, enc_key).unwrap();
            db.init(&opts).unwrap();

            StoredIdentity::new(
                "dummy_address".to_string(),
                rand_vec::<24>(),
                rand_vec::<24>(),
            )
            .store(&db.conn())
            .unwrap();
        } // Drop it
        enc_key[3] = 145; // Alter the enc_key
        let err = NativeDb::new(&opts, enc_key).unwrap_err();
        // Ensure it fails
        assert!(
            matches!(
                err,
                crate::StorageError::Platform(PlatformStorageError::SqlCipherKeyIncorrect)
            ),
            "Expected SqlCipherKeyIncorrect error, got {}",
            err
        );
        EncryptedMessageStore::<()>::remove_db_files(db_path)
    }

    #[xmtp_common::test]
    fn test_db_lock() {
        let path = xmtp_common::tmp_path();
        let opts = StorageOption::Persistent(path.to_string());

        NativeDbConnection::new(Box::new(UnencryptedConnection::new(opts))).unwrap();
        // let _store = EncryptedMessageStore::new(db).expect("constructing message store failed.");
        // let mut connection = SqliteConnection::establish(&path).unwrap();
        // connection_pragmas(&mut connection).unwrap();
    }
}
