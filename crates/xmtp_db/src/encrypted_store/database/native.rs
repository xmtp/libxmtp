mod pool;
mod sqlcipher_connection;

use crate::StorageError;
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
use xmtp_common::{BoxDynError, ErrorCode, RetryableError, retryable};
use xmtp_configuration::{BUSY_TIMEOUT, MAX_DB_POOL_SIZE, MIN_DB_POOL_SIZE};

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
    fn validate(&self, _conn: &mut SqliteConnection) -> Result<(), PlatformStorageError> {
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

#[derive(Debug, Error, ErrorCode)]
pub enum PlatformStorageError {
    /// Pool error.
    ///
    /// Database connection pool error. Retryable.
    #[error("Pool error: {0}")]
    Pool(#[from] diesel::r2d2::PoolError),
    /// DB connection error.
    ///
    /// R2D2 connection manager error. Not retryable.
    #[error("Error with connection to Sqlite {0}")]
    DbConnection(#[from] diesel::r2d2::Error),
    /// Pool needs connection.
    ///
    /// Pool must reconnect before use. Retryable.
    #[error("Pool needs to  reconnect before use")]
    PoolNeedsConnection,
    /// Pool requires path.
    ///
    /// DB pool requires a persistent file path. Not retryable.
    #[error("Using a DB Pool requires a persistent path")]
    PoolRequiresPath,
    /// SQLCipher not loaded.
    ///
    /// Encryption key given but SQLCipher not available. Retryable.
    #[error("The SQLCipher Sqlite extension is not present, but an encryption key is given")]
    SqlCipherNotLoaded,
    /// SQLCipher key incorrect.
    ///
    /// PRAGMA key or salt has wrong value. Not retryable.
    #[error("PRAGMA key or salt has incorrect value")]
    SqlCipherKeyIncorrect,
    /// Database locked.
    ///
    /// Database file is locked by another process. Retryable.
    #[error("Database is locked")]
    DatabaseLocked,
    /// Diesel result error.
    ///
    /// Database query error. May be retryable.
    #[error(transparent)]
    DieselResult(#[from] diesel::result::Error),
    /// Not found.
    ///
    /// Record not found in storage. Not retryable.
    #[error(transparent)]
    NotFound(#[from] NotFound),
    /// I/O error.
    ///
    /// File system I/O error. Retryable.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// Hex decode error.
    ///
    /// Failed to decode hex string. Not retryable.
    #[error(transparent)]
    FromHex(#[from] hex::FromHexError),
    /// Diesel connection error.
    ///
    /// Failed to establish connection. Retryable.
    #[error(transparent)]
    DieselConnect(#[from] diesel::ConnectionError),
    /// Boxed error.
    ///
    /// Wrapped dynamic error. Not retryable.
    #[error(transparent)]
    Boxed(#[from] BoxDynError),
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

/// Database used in `native` (everywhere but web)
#[derive(Clone, Debug)]
pub struct NativeDb {
    customizer: Box<dyn XmtpConnection>,
    conn: Arc<PersistentOrMem<NativeDbConnection, SingleDbConnection, EphemeralDbConnection>>,
    opts: StorageOption,
}

use native_db_builder::{
    Empty, IsComplete, IsSet, IsUnset, SetKey, SetOpts, SetSingleConnection,
};

impl NativeDb {
    pub fn builder() -> NativeDbBuilder<Empty> {
        native_db()
    }
}

#[bon::builder]
pub fn native_db(
    #[builder(setters(vis = "", name = opts_internal))] opts: StorageOption,
    #[builder(required, setters(vis = "", name = key_internal))] key: Option<EncryptionKey>,
    #[builder(default = MAX_DB_POOL_SIZE)] max_pool_size: u32,
    /// minimum amount of connections maintained at any time
    #[builder(default = MIN_DB_POOL_SIZE)]
    min_pool_size: u32,
    /// When true, use a single `Mutex<SqliteConnection>` instead of a pool.
    /// Costs one file descriptor per database. `max_pool_size`/`min_pool_size`
    /// are ignored in this mode. Only meaningful for persistent databases.
    #[builder(default = false, setters(vis = "", name = single_connection_internal))]
    single_connection: bool,
) -> Result<NativeDb, StorageError> {
    NativeDb::new_inner(&opts, key, max_pool_size, min_pool_size, single_connection)
        .map_err(Into::into)
}

impl<S: native_db_builder::State> NativeDbBuilder<S> {
    pub fn ephemeral(self) -> NativeDbBuilder<SetOpts<S>>
    where
        S::Opts: IsUnset,
    {
        self.opts_internal(StorageOption::Ephemeral)
    }

    pub fn persistent(self, path: impl Into<String>) -> NativeDbBuilder<SetOpts<S>>
    where
        S::Opts: IsUnset,
    {
        self.opts_internal(StorageOption::Persistent(path.into()))
    }

    pub fn key(self, key: impl Into<EncryptionKey>) -> NativeDbBuilder<SetKey<S>>
    where
        S::Key: IsUnset,
    {
        self.key_internal(Some(key.into()))
    }

    /// Use a single `Mutex<SqliteConnection>` instead of a connection pool.
    /// Costs exactly one file descriptor. Only meaningful for persistent
    /// databases; ignored for ephemeral ones.
    pub fn single_connection(self) -> NativeDbBuilder<SetSingleConnection<S>>
    where
        S::SingleConnection: IsUnset,
    {
        self.single_connection_internal(true)
    }

    /// Explicitly build the db without encryption
    pub fn build_unencrypted(self) -> Result<NativeDb, StorageError>
    where
        S::Key: IsUnset,
        S::Opts: IsSet,
    {
        let this = self.key_internal(Option::<EncryptionKey>::None);
        this.call()
    }

    /// Build the native db with encryption
    pub fn build(self) -> Result<NativeDb, StorageError>
    where
        S: IsComplete,
    {
        self.call()
    }
}

impl NativeDb {
    /// This function is private so that an unencrypted database cannot be created by accident
    fn new_inner(
        opts: &StorageOption,
        enc_key: Option<EncryptionKey>,
        max_pool_size: u32,
        min_pool_size: u32,
        single_connection: bool,
    ) -> Result<Self, PlatformStorageError> {
        let customizer = if let Some(key) = enc_key {
            let enc_connection = EncryptedConnection::new(key, opts)?;
            if let Some(path) = enc_connection.options().path() {
                let mut conn = SqliteConnection::establish(path)?;
                enc_connection.validate(&mut conn)?;
            }
            Box::new(enc_connection) as Box<dyn XmtpConnection>
        } else if matches!(opts, StorageOption::Persistent(_)) {
            Box::new(UnencryptedConnection::new(opts.clone())) as Box<dyn XmtpConnection>
        } else {
            Box::new(NopConnection::default()) as Box<dyn XmtpConnection>
        };
        let conn = if customizer.is_persistent() {
            if single_connection {
                PersistentOrMem::Single(SingleDbConnection::new(customizer.clone())?)
            } else {
                PersistentOrMem::Persistent(NativeDbConnection::new(
                    customizer.clone(),
                    max_pool_size,
                    min_pool_size,
                )?)
            }
        } else {
            if single_connection {
                tracing::info!(
                    "single_connection requested for an ephemeral database; ignoring (ephemeral is already single-connection)"
                );
            }
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
    type Connection = Arc<PersistentOrMem<NativeDbConnection, SingleDbConnection, EphemeralDbConnection>>;
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

    fn validate(&self, conn: &mut SqliteConnection) -> Result<(), ConnectionError> {
        self.customizer.validate(conn)?;
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
    fn raw_query<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
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

/// A native database backed by a single `Mutex<SqliteConnection>` instead of a
/// pool. Costs exactly one file descriptor. Chosen via the `single_connection`
/// builder flag — useful for services that run many clients in one process
/// (where a per-client pool would exhaust the OS file-descriptor limit) and do
/// serial work per client. There is no connection reentrancy in the codebase,
/// so a non-reentrant `Mutex` is safe (see the design spec).
pub struct SingleDbConnection {
    conn: Arc<Mutex<SqliteConnection>>,
    customizer: Box<dyn XmtpConnection>,
}

impl std::fmt::Debug for SingleDbConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "SingleDbConnection {{ path: {}, is_locked={} }}",
            self.customizer.options(),
            self.conn.is_locked()
        )
    }
}

impl SingleDbConnection {
    fn new(customizer: Box<dyn XmtpConnection>) -> Result<Self, PlatformStorageError> {
        let StorageOption::Persistent(path) = customizer.options() else {
            return Err(PlatformStorageError::PoolRequiresPath);
        };
        let conn = Self::establish(path, &customizer)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            customizer,
        })
    }

    /// Establish a fresh connection and apply the same setup the pool applies:
    /// the customizer's `on_acquire` (sqlcipher key + `connection_pragmas`),
    /// then the one-time WAL/busy_timeout pragmas that `DbPool::new` runs.
    fn establish(
        path: &str,
        customizer: &Box<dyn XmtpConnection>,
    ) -> Result<SqliteConnection, PlatformStorageError> {
        let mut conn = SqliteConnection::establish(path)?;
        // Same per-connection setup the r2d2 customizer applies on checkout.
        customizer
            .on_acquire(&mut conn)
            .map_err(|e| PlatformStorageError::Boxed(Box::new(e)))?;
        // Same one-time pragmas DbPool::new applies on pool creation.
        conn.batch_execute(&format!("PRAGMA busy_timeout = {};", BUSY_TIMEOUT))?;
        conn.batch_execute("PRAGMA journal_mode = WAL;")?;
        Ok(conn)
    }

    fn db_disconnect(&self) -> Result<(), PlatformStorageError> {
        tracing::warn!("single-connection: disconnect is a no-op (connection stays open)");
        Ok(())
    }

    fn db_reconnect(&self) -> Result<(), PlatformStorageError> {
        tracing::info!("single-connection: reconnecting sqlite database connection");
        let StorageOption::Persistent(path) = self.customizer.options() else {
            return Err(PlatformStorageError::PoolRequiresPath);
        };
        let conn = Self::establish(path, &self.customizer)?;
        let mut w = self.conn.lock();
        *w = conn;
        Ok(())
    }
}

impl ConnectionExt for SingleDbConnection {
    fn raw_query<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
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
    max_pool_size: u32,
    min_pool_size: u32,
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
    fn new(
        customizer: Box<dyn XmtpConnection>,
        max_pool_size: u32,
        min_pool_size: u32,
    ) -> Result<Self, PlatformStorageError> {
        let pool = DbPool::builder()
            .customizer(customizer.clone())
            .max_size(max_pool_size)
            .min_size(min_pool_size)
            .build()?;

        Ok(Self {
            pool: ArcSwapOption::new(Some(Arc::new(pool))),
            customizer,
            max_pool_size,
            min_pool_size,
        })
    }

    fn db_disconnect(&self) -> Result<(), PlatformStorageError> {
        tracing::warn!("released sqlite database connection");
        self.pool.store(None);
        Ok(())
    }

    fn db_reconnect(&self) -> Result<(), PlatformStorageError> {
        tracing::info!("reconnecting sqlite database connection");
        let pool = DbPool::builder()
            .max_size(self.max_pool_size)
            .min_size(self.min_pool_size)
            .customizer(self.customizer.clone())
            .build()?;
        self.pool.store(Some(Arc::new(pool)));
        Ok(())
    }
}

impl ConnectionExt for NativeDbConnection {
    fn raw_query<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        if let Some(pool) = &*self.pool.load() {
            let mut conn = pool.get()?;
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
        {
            let db = NativeDb::builder()
                .persistent(db_path.clone())
                .key(enc_key)
                .build()
                .unwrap();
            db.init().unwrap();

            StoredIdentity::new(
                "dummy_address".to_string(),
                rand_vec::<24>(),
                rand_vec::<24>(),
            )
            .store(&db.conn())
            .unwrap();
        } // Drop it
        enc_key[3] = 145; // Alter the enc_key
        let err = NativeDb::builder()
            .persistent(db_path.clone())
            .key(enc_key)
            .build()
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
        EncryptedMessageStore::<()>::remove_db_files(db_path)
    }

    #[tokio::test]
    async fn single_connection_roundtrip_and_reconnect() {
        use crate::{Fetch, Store, identity::StoredIdentity};

        let db_path = tmp_path();
        {
            let db = NativeDb::builder()
                .persistent(db_path.clone())
                .key([7u8; 32])
                .single_connection()
                .build()
                .unwrap();
            db.init().unwrap();

            assert!(
                matches!(&*db.conn(), PersistentOrMem::Single(_)),
                "expected Single arm for single_connection() persistent db"
            );

            let conn = db.conn();
            let inbox_id = "single_conn_inbox";
            StoredIdentity::new(inbox_id.to_string(), rand_vec::<24>(), rand_vec::<24>())
                .store(&conn)
                .unwrap();

            let fetched: StoredIdentity = conn.fetch(&()).unwrap().unwrap();
            assert_eq!(fetched.inbox_id, inbox_id);

            conn.reconnect().unwrap();
            let fetched2: StoredIdentity = conn.fetch(&()).unwrap().unwrap();
            assert_eq!(fetched2.inbox_id, inbox_id);
        }
        EncryptedMessageStore::<()>::remove_db_files(db_path)
    }

    #[tokio::test]
    async fn single_connection_mismatched_key_fails() {
        use crate::database::PlatformStorageError;

        let db_path = tmp_path();
        {
            let db = NativeDb::builder()
                .persistent(db_path.clone())
                .key([1u8; 32])
                .single_connection()
                .build()
                .unwrap();
            db.init().unwrap();
            StoredIdentity::new("addr".to_string(), rand_vec::<24>(), rand_vec::<24>())
                .store(&db.conn())
                .unwrap();
        }
        let mut bad = [1u8; 32];
        bad[3] = 200;
        let err = NativeDb::builder()
            .persistent(db_path.clone())
            .key(bad)
            .single_connection()
            .build()
            .unwrap_err();
        assert!(
            matches!(
                err,
                crate::StorageError::Platform(PlatformStorageError::SqlCipherKeyIncorrect)
            ),
            "expected SqlCipherKeyIncorrect, got {err}"
        );
        EncryptedMessageStore::<()>::remove_db_files(db_path)
    }

    // Exercises a transaction + nested savepoint on a single (non-reentrant
    // Mutex) connection. The single-connection mode threads one `&mut
    // SqliteConnection` down through the transaction closure, so re-deriving a
    // transaction-scoped key store inside the closure (and again inside the
    // savepoint) must NOT re-acquire the outer Mutex and deadlock. Reaching the
    // assertion at all proves there is no deadlock; the COUNT proves the writes
    // persisted.
    #[tokio::test]
    async fn single_connection_nested_transaction_no_deadlock() {
        use crate::{
            ConnectionExt, Store, StorageError, StoreOrIgnore, TransactionalKeyStore,
            XmtpMlsStorageProvider,
            refresh_state::{EntityKind, RefreshState},
            sql_key_store::SqlKeyStore,
        };
        use diesel::prelude::*;

        let db_path = tmp_path();
        {
            let db = NativeDb::builder()
                .persistent(db_path.clone())
                .key([9u8; 32])
                .single_connection()
                .build()
                .unwrap();
            db.init().unwrap();

            // The storage provider wraps the single connection (an `Arc<PersistentOrMem<..>>`
            // that implements `ConnectionExt`). `SqlKeyStore<C>` implements
            // `XmtpMlsStorageProvider`, exposing `.transaction()`.
            let provider = SqlKeyStore::new(db.conn());

            provider
                .transaction(|conn| {
                    // `conn` is `&mut SqliteConnection`; `key_store()` (from
                    // `TransactionalKeyStore`) gives a transaction-scoped provider.
                    // `identity` is a singleton table, so only the outer write
                    // targets it; the nested savepoint writes to a multi-row
                    // table (`refresh_state`) to avoid a constraint collision.
                    let storage = conn.key_store();
                    StoredIdentity::new("txn_outer".to_string(), rand_vec::<24>(), rand_vec::<24>())
                        .store(&storage.db())?;

                    // Nested write inside a SQLite savepoint, re-deriving the
                    // key store from the savepoint's `&mut SqliteConnection`.
                    storage.savepoint(|sp_conn| {
                        let inner = sp_conn.key_store();
                        RefreshState {
                            entity_id: rand_vec::<24>(),
                            entity_kind: EntityKind::Welcome,
                            sequence_id: 1,
                            originator_id: 0,
                        }
                        .store_or_ignore(&inner.db())?;
                        Ok::<_, StorageError>(())
                    })?;
                    Ok::<_, StorageError>(())
                })
                .unwrap();

            // Reaching here means no deadlock. Confirm the outer write persisted.
            let count: i64 = db
                .conn()
                .raw_query(|c| {
                    use diesel::dsl::sql;
                    use diesel::sql_types::BigInt;
                    diesel::select(sql::<BigInt>("(SELECT COUNT(*) FROM identity)")).get_result(c)
                })
                .unwrap();
            assert!(count >= 1, "expected at least the outer identity row");
        }
        EncryptedMessageStore::<()>::remove_db_files(db_path)
    }
}
