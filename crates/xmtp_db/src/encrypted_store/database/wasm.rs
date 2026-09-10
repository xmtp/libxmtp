//! WebAssembly specific connection for a SQLite Database
//! Stores a single connection behind a RefCell that's used for every libxmtp operation
use crate::DbConnection;
use crate::PersistentOrMem;
use crate::{ConnectionExt, StorageOption, XmtpDb};
use diesel::prelude::SqliteConnection;
use diesel::{connection::SimpleConnection, prelude::*};
use sqlite_wasm_vfs::sahpool::OpfsSAHPoolCfg;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use thiserror::Error;
use web_sys::wasm_bindgen::JsCast;
use xmtp_common::ErrorCode;

mod restore;
pub use restore::{clear_opfs_databases, delete_opfs_database, import_opfs_database};

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
}

impl xmtp_common::RetryableError for PlatformStorageError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::SAH(_) => true,
            Self::Connection(_) => true,
            Self::DieselResult(_) => true,
            Self::Disconnected | Self::DatabaseInUse | Self::Initialization(_) => true,
            Self::Replaced | Self::RestoreDestinationExists | Self::InvalidRestoreInput => false,
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

pub static SQLITE: tokio::sync::OnceCell<SyncOpfsUtil> = tokio::sync::OnceCell::const_new();

/// Get a reference to the initialized OPFS util, if available.
pub fn get_sqlite() -> Option<Result<&'static OpfsSAHPoolUtil, &'static String>> {
    SQLITE.get().map(|w| w.0.as_ref())
}

/// Initialize the SQLite WebAssembly Library
/// Generally this should not be required to call, since it
/// is called as part of creating a new EncryptedMessageStore.
/// However, if opfs needs to be used before client creation, this should
/// be called.
pub async fn init_sqlite() {
    let wrapper = SQLITE.get_or_init(init_opfs).await;
    if let Err(e) = wrapper.as_ref() {
        tracing::error!("{e}");
    }
}

async fn maybe_resize() -> Result<(), PlatformStorageError> {
    if let Some(Ok(util)) = get_sqlite() {
        let capacity = util.get_capacity();
        let used = util.count();
        if used >= capacity / 2 {
            let adding = (capacity * 2) - capacity;
            tracing::debug!(
                "{used} files in pool, increasing capacity to {}",
                adding + capacity
            );
            util.add_capacity(adding).await?;
        }
    }
    Ok(())
}

pub async fn init_opfs() -> SyncOpfsUtil {
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
    // the error is not send or sync as required by tokio OnceCell
    SyncOpfsUtil(r.map_err(|e| format!("{e}")))
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
    pub async fn new(opts: &StorageOption) -> Result<Self, PlatformStorageError> {
        use crate::StorageOption::*;
        let conn = match opts {
            Ephemeral => PersistentOrMem::Mem(WasmDbConnection::new_ephemeral("xmtp-ephemeral")?),
            Persistent(db_path) => {
                init_sqlite().await;
                let _opening = restore::PendingOpen::acquire()?;
                maybe_resize().await?;
                tracing::debug!("creating persistent opfs db @{}", db_path);
                PersistentOrMem::Persistent(WasmDbConnection::new(db_path)?)
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
        restore::check_open_allowed()?;
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
        restore::check_open_allowed()?;
        let mut state = self
            .conn
            .try_borrow_mut()
            .map_err(|_| crate::ConnectionError::ReconnectInTransaction)?;
        if state.replaced {
            return Err(PlatformStorageError::Replaced.into());
        }
        if state.connection.is_none() {
            let mut conn =
                SqliteConnection::establish(&self.path).map_err(PlatformStorageError::from)?;
            conn.batch_execute("PRAGMA foreign_keys = on;")?;
            state.connection = Some(conn);
        }
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
