//! WebAssembly specific connection for a SQLite Database
//! Stores a single connection behind a RefCell that's used for every libxmtp operation
use crate::DbConnection;
use crate::PersistentOrMem;
use crate::{ConnectionExt, StorageOption, XmtpDb};
use diesel::prelude::SqliteConnection;
use diesel::{connection::SimpleConnection, prelude::*};
use sqlite_wasm_vfs::sahpool::OpfsSAHPoolCfg;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use thiserror::Error;
use web_sys::wasm_bindgen::JsCast;
use xmtp_common::ErrorCode;

mod restore;
mod working_copy;
pub use restore::{
    clear_opfs_databases, delete_opfs_database, export_opfs_database, import_opfs_database,
    list_opfs_databases, opfs_database_count, opfs_database_exists, opfs_pool_capacity,
};
pub use working_copy::{OpfsWorkingCopy, OpfsWorkingCopyError};

#[derive(Debug, Error, ErrorCode)]
pub enum PlatformStorageError {
    /// OPFS error.
    ///
    /// Origin Private File System (OPFS) error. Retryable.
    #[error("OPFS {0}")]
    SAH(#[from] OpfsSAHError),
    /// Connection error.
    ///
    /// Diesel connection error. Retryable.
    #[error(transparent)]
    Connection(#[from] diesel::ConnectionError),
    /// Diesel result error.
    ///
    /// Database query error. Retryable.
    #[error(transparent)]
    DieselResult(#[from] diesel::result::Error),
    /// The persistent connection was closed. Reconnect before using it.
    #[error("database connection is closed")]
    Disconnected,
    /// The file was restored or deleted. This object cannot reconnect.
    #[error("database file changed; create a new client")]
    Replaced,
    /// A target connection is still open or has an active query.
    #[error("close all target database connections before changing the file")]
    DatabaseInUse,
    /// Whole-database import does not replace an existing OPFS file.
    #[error("import destination exists; use a new path or close and delete the target first")]
    RestoreDestinationExists,
    /// The restore input is not a complete SQLite database.
    #[error("restore input is not a SQLite database")]
    InvalidRestoreInput,
    /// The OPFS utility could not be initialized.
    #[error("OPFS initialization failed: {0}")]
    Initialization(String),
    /// A pool transition failed or was cancelled. Terminate this worker before retrying.
    #[error("OPFS pool is unusable; terminate this worker before retrying")]
    PoolUnusable,
    /// Persistent OPFS storage needs a plain path, not a SQLite URI. Not retryable.
    #[error("persistent OPFS storage requires a plain path; SQLite URIs are not supported")]
    InvalidDatabasePath,
}

impl xmtp_common::RetryableError for PlatformStorageError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::SAH(_) => true,
            Self::Connection(_) => true,
            Self::DieselResult(_) => true,
            Self::Disconnected | Self::DatabaseInUse | Self::Initialization(_) => true,
            Self::Replaced
            | Self::RestoreDestinationExists
            | Self::InvalidRestoreInput
            | Self::PoolUnusable
            | Self::InvalidDatabasePath => false,
        }
    }
}

#[derive(Clone)]
pub struct WasmDb {
    conn: Arc<PersistentOrMem<WasmDbConnection, std::convert::Infallible, WasmDbConnection>>,
    opts: StorageOption,
}

pub use sqlite_wasm_vfs::sahpool::{OpfsSAHError, OpfsSAHPoolUtil};

/// Wrapper to allow OpfsSAHPoolUtil in a static OnceCell on wasm (single-threaded).
pub struct SyncOpfsUtil(pub Result<OpfsSAHPoolUtil, String>);
// SAFETY: wasm32 is single-threaded; these are never accessed across threads.
unsafe impl Send for SyncOpfsUtil {}
unsafe impl Sync for SyncOpfsUtil {}

impl std::ops::Deref for SyncOpfsUtil {
    type Target = Result<OpfsSAHPoolUtil, String>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

static SQLITE: tokio::sync::OnceCell<SyncOpfsUtil> = tokio::sync::OnceCell::const_new();

/// Get the raw OPFS utility for legacy callers.
/// New callers must use the guarded database utility functions. The raw utility
/// does not enforce admission, transition exclusion, or failed-pool fencing.
pub fn get_sqlite() -> Option<Result<&'static OpfsSAHPoolUtil, &'static String>> {
    SQLITE.get().map(|w| w.0.as_ref())
}

/// Initialize the SQLite WebAssembly Library
/// Generally this should not be required to call, since it
/// is called as part of creating a new EncryptedMessageStore.
/// However, if opfs needs to be used before client creation, this should
/// be called.
pub async fn init_sqlite() {
    if let Err(e) = init_sqlite_lenient().await {
        tracing::error!("{e}");
    }
}

/// The legacy install. A failed install stays retryable in this worker, as
/// before strict opens: a later call installs once another owner releases
/// the pool.
async fn init_sqlite_lenient() -> Result<(), PlatformStorageError> {
    let _utility = restore::ActiveUtility::acquire()?;
    let _transition = POOL_TRANSITION.lock().await;
    resume_sqlite(false).await?;
    Ok(())
}

// Admission is synchronous. Every admitted operation takes this mutex before
// touching VFS maps or starting install, resume, resize, or clear.
static POOL_TRANSITION: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

thread_local! {
    static POOL_UNUSABLE: Cell<bool> = const { Cell::new(false) };
}

/// A failed or cancelled VFS transition can leave partial access handles.
/// The owner must terminate this worker while it still holds its Web Lock.
pub fn opfs_requires_worker_restart() -> bool {
    POOL_UNUSABLE.with(Cell::get)
}

fn check_pool_usable() -> Result<(), PlatformStorageError> {
    if opfs_requires_worker_restart() {
        return Err(PlatformStorageError::PoolUnusable);
    }
    Ok(())
}

/// Mark a partial VFS transition unusable on error or future cancellation.
struct VfsTransitionFailure(bool);

impl VfsTransitionFailure {
    fn complete(mut self) {
        self.0 = false;
    }
}

impl Drop for VfsTransitionFailure {
    fn drop(&mut self) {
        if self.0 {
            POOL_UNUSABLE.with(|failed| failed.set(true));
        }
    }
}

/// Install or resume under utility admission and the shared transition guard.
pub async fn try_init_sqlite() -> Result<(), PlatformStorageError> {
    let _utility = restore::ActiveUtility::acquire()?;
    let _transition = POOL_TRANSITION.lock().await;
    resume_sqlite(true).await?;
    Ok(())
}

/// The caller must hold POOL_TRANSITION and an admission guard. A strict
/// transition that fails or is cancelled makes the pool unusable in this
/// worker. A lenient one, for legacy callers, leaves it retryable.
async fn resume_sqlite(strict: bool) -> Result<&'static OpfsSAHPoolUtil, PlatformStorageError> {
    check_pool_usable()?;
    let failure = VfsTransitionFailure(strict);
    let util = SQLITE.get_or_try_init(init_opfs).await?;
    let util = util
        .0
        .as_ref()
        .map_err(|error| PlatformStorageError::Initialization(error.clone()))?;
    util.unpause_vfs().await?;
    failure.complete();
    Ok(util)
}

/// Give up OPFS access handles after the last SQLite file closes.
/// Do not pause while any admitted work can still touch the pool.
pub fn pause_sqlite_if_idle() {
    if restore::operation_pending() || opfs_requires_worker_restart() {
        return;
    }
    let Ok(_transition) = POOL_TRANSITION.try_lock() else {
        return;
    };
    // The pinned VFS tracks open filenames in a set, not connection counts.
    // Its idle check alone cannot prove that every local connection is closed.
    if restore::closed_target(None).is_err() {
        return;
    }
    if let Some(Ok(util)) = get_sqlite()
        && let Err(error) = util.pause_vfs()
    {
        tracing::debug!("OPFS pool stays open: {error}");
    }
}

/// The caller must hold POOL_TRANSITION and PendingOpen.
async fn maybe_resize(util: &OpfsSAHPoolUtil) -> Result<(), PlatformStorageError> {
    let capacity = util.get_capacity();
    let used = util.count();
    if used >= capacity / 2 {
        let adding = capacity;
        tracing::debug!(
            "{used} files in pool, increasing capacity to {}",
            adding + capacity
        );
        let failure = VfsTransitionFailure(true);
        util.add_capacity(adding).await?;
        failure.complete();
    }
    Ok(())
}

async fn init_opfs() -> Result<SyncOpfsUtil, PlatformStorageError> {
    let cfg = OpfsSAHPoolCfg {
        vfs_name: xmtp_configuration::WASM_VFS_NAME.into(),
        directory: xmtp_configuration::WASM_VFS_DIRECTORY.into(),
        clear_on_init: false,
        initial_capacity: 6,
    };

    let r = sqlite_wasm_vfs::sahpool::install::<sqlite_wasm_rs::WasmOsCallback>(&cfg, true).await;
    if let Err(ref e) = r {
        match e {
            OpfsSAHError::CreateSyncAccessHandle(e) => log_exception(e),
            OpfsSAHError::Read(e) => log_exception(e),
            OpfsSAHError::Write(e) => log_exception(e),
            OpfsSAHError::GetFileHandle(e) => log_exception(e),
            OpfsSAHError::Flush(e) => log_exception(e),
            OpfsSAHError::IterHandle(e) => log_exception(e),
            OpfsSAHError::GetPath(e) => log_exception(e),
            OpfsSAHError::RemoveEntity(e) => log_exception(e),
            OpfsSAHError::GetSize(e) => log_exception(e),
            _ => (),
        }
        tracing::warn!("Encountered possible vfs error {e}");
    }
    r.map(|util| SyncOpfsUtil(Ok(util)))
        .map_err(PlatformStorageError::SAH)
}

/// URI parsing can give different inputs the same VFS filename. The registry
/// requires plain paths so admission uses the exact filename SQLite will open.
fn validate_persistent_path(path: &str) -> Result<(), PlatformStorageError> {
    if path.starts_with("file:") || path.starts_with("sqlite://") {
        return Err(PlatformStorageError::InvalidDatabasePath);
    }
    Ok(())
}

/// Synchronous opens cannot wait for resume. Reject them while it is needed.
fn check_pool_ready() -> Result<(), PlatformStorageError> {
    check_pool_usable()?;
    match get_sqlite() {
        Some(Ok(util)) if !util.is_paused() => Ok(()),
        _ => Err(PlatformStorageError::DatabaseInUse),
    }
}

fn log_exception(e: &wasm_bindgen::JsValue) {
    if let Ok(exception) = e.clone().dyn_into::<web_sys::DomException>() {
        tracing::error!(
            "error code={}, {}:{}",
            exception.name(),
            exception.message(),
            exception.code()
        );
    }
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
    /// Open a database for existing callers. If this worker cannot install or
    /// resume the OPFS pool, log the error and open the path on SQLite's
    /// default VFS. Such a database is not persistent. For example, a second
    /// worker cannot take a pool that another worker owns.
    pub async fn new(opts: &StorageOption) -> Result<Self, PlatformStorageError> {
        Self::open(opts, false).await
    }

    /// Open a database that must use the OPFS pool. Pool failures return their
    /// typed error. The path must be plain, and it must have no live connection.
    /// Its connections stop when the pool becomes unusable.
    pub async fn new_strict(opts: &StorageOption) -> Result<Self, PlatformStorageError> {
        Self::open(opts, true).await
    }

    async fn open(opts: &StorageOption, strict: bool) -> Result<Self, PlatformStorageError> {
        use crate::StorageOption::*;
        let conn = match opts {
            Ephemeral => PersistentOrMem::Mem(WasmDbConnection::new_ephemeral("xmtp-ephemeral")?),
            Persistent(db_path) => {
                if strict {
                    validate_persistent_path(db_path)?;
                }
                let _opening = restore::PendingOpen::acquire_for(strict)?;
                let _transition = POOL_TRANSITION.lock().await;
                if strict {
                    restore::closed_target(Some(db_path))?;
                    let util = resume_sqlite(true).await?;
                    maybe_resize(util).await?;
                } else {
                    match resume_sqlite(false).await {
                        Ok(util) => maybe_resize(util).await?,
                        Err(error) => tracing::error!(
                            "OPFS pool is unavailable; {db_path} opens on the default SQLite VFS: {error}"
                        ),
                    }
                }
                tracing::debug!("creating persistent opfs db @{}", db_path);
                PersistentOrMem::Persistent(WasmDbConnection::connect(db_path, strict)?)
            }
        };
        Ok(Self {
            conn: Arc::new(conn),
            opts: opts.clone(),
        })
    }
}

/// A shared SQLite handle with explicit close and file-replacement fencing.
pub struct WasmDbConnection {
    conn: Rc<RefCell<ConnectionState>>,
    path: String,
    persistent: bool,
    /// Strict connections need a usable OPFS pool and an exclusive path.
    strict: bool,
}

/// Lifecycle state shared by every clone of one database connection.
struct ConnectionState {
    /// `None` means the persistent handle was closed and may need reconnect.
    connection: Option<SqliteConnection>,
    /// A file change permanently prevents this object from reconnecting.
    replaced: bool,
}

impl WasmDbConnection {
    pub fn new(path: &str) -> Result<Self, PlatformStorageError> {
        validate_persistent_path(path)?;
        let _opening = restore::PendingOpen::acquire()?;
        let _transition = POOL_TRANSITION
            .try_lock()
            .map_err(|_| PlatformStorageError::DatabaseInUse)?;
        restore::closed_target(Some(path))?;
        check_pool_ready()?;
        Self::connect(path, true)
    }

    // The caller holds persistent-open admission and POOL_TRANSITION. A strict
    // caller has also checked that this plain path has no live connection.
    fn connect(path: &str, strict: bool) -> Result<Self, PlatformStorageError> {
        let mut conn = SqliteConnection::establish(path)?;
        conn.batch_execute("PRAGMA foreign_keys = on;")?;
        #[derive(QueryableByName)]
        struct DatabaseFile {
            #[diesel(sql_type = diesel::sql_types::Text)]
            file: String,
        }
        let file = diesel::sql_query("SELECT file FROM pragma_database_list WHERE name = 'main'")
            .get_result::<DatabaseFile>(&mut conn)?;
        let conn = Rc::new(RefCell::new(ConnectionState {
            connection: Some(conn),
            replaced: false,
        }));
        restore::register_connection(&file.file, &conn);
        Ok(Self {
            conn,
            path: path.to_string(),
            persistent: true,
            strict,
        })
    }

    pub fn new_ephemeral(path: &str) -> Result<Self, PlatformStorageError> {
        let name = xmtp_common::rand_string::<12>();
        let path = format!("file:/{path}-{name}?vfs=memdb");
        let mut conn = SqliteConnection::establish(&path)?;
        conn.batch_execute("PRAGMA foreign_keys = on;")?;

        Ok(Self {
            conn: Rc::new(RefCell::new(ConnectionState {
                connection: Some(conn),
                replaced: false,
            })),
            path,
            persistent: false,
            strict: false,
        })
    }

    pub fn path(&self) -> &str {
        self.path.as_str()
    }
}

impl ConnectionExt for WasmDbConnection {
    fn raw_query<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        if self.strict {
            check_pool_usable()?;
        }
        let mut state = self
            .conn
            .try_borrow_mut()
            .map_err(|_| PlatformStorageError::DatabaseInUse)?;
        if state.replaced {
            return Err(PlatformStorageError::Replaced.into());
        }
        let conn = state
            .connection
            .as_mut()
            .ok_or(PlatformStorageError::Disconnected)?;
        Ok(fun(conn)?)
    }

    fn disconnect(&self) -> Result<(), crate::ConnectionError> {
        // Preserve the existing ephemeral reconnect behavior. No file can replace it.
        if !self.persistent {
            return Ok(());
        }
        let mut state = self
            .conn
            .try_borrow_mut()
            .map_err(|_| crate::ConnectionError::DisconnectInTransaction)?;
        state.connection = None;
        Ok(())
    }

    fn reconnect(&self) -> Result<(), crate::ConnectionError> {
        let _opening = self
            .persistent
            .then(|| restore::PendingOpen::acquire_for(self.strict))
            .transpose()?;
        let _transition = if self.strict {
            Some(
                POOL_TRANSITION
                    .try_lock()
                    .map_err(|_| PlatformStorageError::DatabaseInUse)?,
            )
        } else {
            None
        };
        {
            let state = self
                .conn
                .try_borrow()
                .map_err(|_| crate::ConnectionError::ReconnectInTransaction)?;
            if state.replaced {
                return Err(PlatformStorageError::Replaced.into());
            }
            if state.connection.is_some() {
                return Ok(());
            }
        }
        if self.strict {
            check_pool_ready()?;
            restore::closed_target(Some(&self.path))?;
        }
        let mut state = self
            .conn
            .try_borrow_mut()
            .map_err(|_| crate::ConnectionError::ReconnectInTransaction)?;
        let mut conn =
            SqliteConnection::establish(&self.path).map_err(PlatformStorageError::from)?;
        conn.batch_execute("PRAGMA foreign_keys = on;")?;
        state.connection = Some(conn);
        Ok(())
    }
}

impl XmtpDb for WasmDb {
    type Connection =
        Arc<PersistentOrMem<WasmDbConnection, std::convert::Infallible, WasmDbConnection>>;
    type DbQuery = DbConnection<Self::Connection>;

    fn conn(&self) -> Self::Connection {
        self.conn.clone()
    }

    fn db(&self) -> Self::DbQuery {
        DbConnection::new(self.conn.clone())
    }

    fn validate(&self, _c: &mut SqliteConnection) -> Result<(), crate::ConnectionError> {
        Ok(())
    }

    fn reconnect(&self) -> Result<(), crate::ConnectionError> {
        self.conn.reconnect()
    }

    fn disconnect(&self) -> Result<(), crate::ConnectionError> {
        self.conn.disconnect()
    }

    fn opts(&self) -> &StorageOption {
        &self.opts
    }
}
