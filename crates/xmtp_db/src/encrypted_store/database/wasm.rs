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

#[derive(Debug, Error, ErrorCode)]
pub enum PlatformStorageError {
    #[error("OPFS {0}")]
    SAH(#[from] OpfsSAHError),
    #[error(transparent)]
    Connection(#[from] diesel::ConnectionError),
    #[error(transparent)]
    DieselResult(#[from] diesel::result::Error),
}

impl xmtp_common::RetryableError for PlatformStorageError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::SAH(_) => true,
            Self::Connection(_) => true,
            Self::DieselResult(_) => true,
        }
    }
}

#[derive(Clone)]
pub struct WasmDb {
    conn: Arc<PersistentOrMem<WasmDbConnection, WasmDbConnection>>,
    opts: StorageOption,
}

pub use sqlite_wasm_vfs::sahpool::{OpfsSAHError, OpfsSAHPoolUtil};

/// Wrapper to allow OpfsSAHPoolUtil in a static OnceCell on wasm (single-threaded).
pub struct SyncOpfsUtil(Result<OpfsSAHPoolUtil, String>);
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

async fn init_opfs() -> SyncOpfsUtil {
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

pub struct WasmDbConnection {
    conn: Rc<RefCell<SqliteConnection>>,
    path: String,
}

impl WasmDbConnection {
    pub fn new(path: &str) -> Result<Self, PlatformStorageError> {
        let mut conn = SqliteConnection::establish(path)?;
        conn.batch_execute("PRAGMA foreign_keys = on;")?;
        Ok(Self {
            conn: Rc::new(RefCell::new(conn)),
            path: path.to_string(),
        })
    }

    pub fn new_ephemeral(path: &str) -> Result<Self, PlatformStorageError> {
        let name = xmtp_common::rand_string::<12>();
        let path = format!("file:/{path}-{name}?vfs=memdb");
        let mut conn = SqliteConnection::establish(&path)?;
        conn.batch_execute("PRAGMA foreign_keys = on;")?;

        Ok(Self {
            conn: Rc::new(RefCell::new(conn)),
            path,
        })
    }

    pub fn path(&self) -> &str {
        self.path.as_str()
    }
}

impl ConnectionExt for WasmDbConnection {
    fn raw_query_read<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        let mut conn = self.conn.borrow_mut();
        Ok(fun(&mut conn)?)
    }

    fn raw_query_write<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        let mut conn = self.conn.borrow_mut();
        Ok(fun(&mut conn)?)
    }

    fn disconnect(&self) -> Result<(), crate::ConnectionError> {
        Ok(())
    }

    fn reconnect(&self) -> Result<(), crate::ConnectionError> {
        Ok(())
    }
}

impl XmtpDb for WasmDb {
    type Connection = Arc<PersistentOrMem<WasmDbConnection, WasmDbConnection>>;
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
        Ok(())
    }

    fn disconnect(&self) -> Result<(), crate::ConnectionError> {
        Ok(())
    }

    fn opts(&self) -> &StorageOption {
        &self.opts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);
    use crate::DbConnection;
    use crate::EncryptedMessageStore;
    use crate::identity::StoredIdentity;

    pub async fn with_opfs<'a, F, R>(path: impl Into<Option<&'a str>>, f: F) -> R
    where
        F: FnOnce(crate::DefaultDbConnection) -> R,
    {
        let util = init_opfs().await;
        let o: Option<&'a str> = path.into();
        let p = o.map(String::from).unwrap_or(xmtp_common::tmp_path());
        let db = crate::database::WasmDb::new(&StorageOption::Persistent(p))
            .await
            .unwrap();
        let store = EncryptedMessageStore::new(db).unwrap();
        let conn = store.conn();
        let r = f(DbConnection::new(conn));
        if let SyncOpfsUtil(Ok(u)) = util {
            u.clear_all().await.unwrap();
        }
        r
    }

    #[allow(unused)]
    pub async fn with_opfs_async<'a, R>(
        path: impl Into<Option<&'a str>>,
        f: impl AsyncFnOnce(crate::DefaultDbConnection) -> R,
    ) -> R {
        let util = init_opfs().await;
        let o: Option<&'a str> = path.into();
        let p = o.map(String::from).unwrap_or(xmtp_common::tmp_path());
        let db = crate::database::WasmDb::new(&StorageOption::Persistent(p))
            .await
            .unwrap();
        let store = EncryptedMessageStore::new(db).unwrap();
        let conn = store.conn();
        let r = f(DbConnection::new(conn)).await;
        if let SyncOpfsUtil(Ok(u)) = util {
            u.clear_all().await.unwrap();
        }
        r
    }

    #[xmtp_common::test]
    async fn test_opfs() {
        use crate::traits::Store;

        let path = "test_db";
        with_opfs(path, |c1| {
            let intent = StoredIdentity::builder()
                .inbox_id("test")
                .installation_keys(vec![0, 1, 1, 1])
                .credential_bytes(vec![0, 0, 0, 0])
                .next_key_package_rotation_ns(1)
                .build()
                .unwrap();
            intent.store(&c1).unwrap();
        })
        .await;
    }

    #[xmtp_common::test]
    async fn opfs_dynamically_resizes() {
        use xmtp_common::tmp_path as path;
        init_sqlite().await;
        if let Some(Ok(util)) = get_sqlite() {
            util.clear_all().await.unwrap();
            let current_capacity = util.get_capacity();
            if current_capacity > 6 {
                util.reduce_capacity(current_capacity - 6).await.unwrap();
            }
        }
        with_opfs_async(&*path(), async move |_| {
            with_opfs_async(&*path(), async move |_| {
                with_opfs_async(&*path(), async move |_| {
                    with_opfs(&*path(), |_| {
                        // should have been resized here
                        if let Some(Ok(util)) = get_sqlite() {
                            let cap = util.get_capacity();
                            assert_eq!(cap, 12);
                        } else {
                            panic!("opfs failed to init")
                        }
                    })
                    .await
                })
                .await
            })
            .await
        })
        .await
    }
}
