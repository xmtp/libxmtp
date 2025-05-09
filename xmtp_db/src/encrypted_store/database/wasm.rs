//! WebAssembly specific connection for a SQLite Database
//! Stores a single connection behind a mutex that's used for every libxmtp operation
use crate::PersistentOrMem;
use crate::{ConnectionExt, StorageOption, TransactionGuard, XmtpDb};
use diesel::{connection::TransactionManager, prelude::SqliteConnection};
use diesel::{
    connection::{AnsiTransactionManager, SimpleConnection},
    prelude::*,
};
use parking_lot::Mutex;
use sqlite_wasm_rs::export::OpfsSAHPoolCfg;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use thiserror::Error;
use web_sys::wasm_bindgen::JsCast;

#[derive(Debug, Error)]
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
    conn: super::DefaultConnection,
    opts: StorageOption,
}

pub static SQLITE: tokio::sync::OnceCell<Result<OpfsSAHPoolUtil, String>> =
    tokio::sync::OnceCell::const_new();
pub use sqlite_wasm_rs::export::{OpfsSAHError, OpfsSAHPoolUtil};

/// Initialize the SQLite WebAssembly Library
/// Generally this should not be required to call, since it
/// is called as part of creating a new EncryptedMessageStore.
/// However, if opfs needs to be used before client creation, this should
/// be called.
pub async fn init_sqlite() {
    if let Err(e) = SQLITE.get_or_init(|| init_opfs()).await {
        tracing::error!("{e}");
    }
}

async fn maybe_resize() -> Result<(), PlatformStorageError> {
    if let Some(Ok(util)) = SQLITE.get() {
        let capacity = util.get_capacity();
        let used = util.get_file_count();
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

async fn init_opfs() -> Result<OpfsSAHPoolUtil, String> {
    let cfg = OpfsSAHPoolCfg {
        vfs_name: crate::configuration::VFS_NAME.into(),
        directory: crate::configuration::VFS_DIRECTORY.into(),
        clear_on_init: false,
        initial_capacity: 6,
    };

    let r = sqlite_wasm_rs::export::install_opfs_sahpool(Some(&cfg), true).await;
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
    r.map_err(|e| format!("{e}"))
}

fn log_exception(e: &wasm_bindgen::JsValue) {
    if let Ok(exception) = e.clone().dyn_into::<web_sys::DomException>() {
        tracing::error!(
            "error code={}, {}:{}",
            exception.name(),
            exception.message(),
            exception.code()
        );
        return;
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
        init_sqlite().await;
        maybe_resize().await?;
        let conn = match opts {
            Ephemeral => PersistentOrMem::Mem(WasmDbConnection::new_ephemeral("xmtp-ephemeral")?),
            Persistent(db_path) => {
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
    conn: Arc<Mutex<SqliteConnection>>,
    transaction_lock: Arc<Mutex<()>>,
    in_transaction: Arc<AtomicBool>,
    path: String,
}

impl WasmDbConnection {
    pub fn new(path: &str) -> Result<Self, PlatformStorageError> {
        let mut conn = SqliteConnection::establish(path)?;
        conn.batch_execute("PRAGMA foreign_keys = on;")?;
        Ok(Self {
            transaction_lock: Arc::new(Mutex::new(())),
            in_transaction: Arc::new(AtomicBool::new(false)),
            conn: Arc::new(Mutex::new(conn)),
            path: path.to_string(),
        })
    }

    pub fn new_ephemeral(path: &str) -> Result<Self, PlatformStorageError> {
        let name = xmtp_common::rand_string::<12>();
        let path = format!("file:/{path}-{name}?vfs=memdb");
        let mut conn = SqliteConnection::establish(&path)?;
        conn.batch_execute("PRAGMA foreign_keys = on;")?;

        Ok(Self {
            transaction_lock: Arc::new(Mutex::new(())),
            in_transaction: Arc::new(AtomicBool::new(false)),
            conn: Arc::new(Mutex::new(conn)),
            path,
        })
    }

    pub fn path(&self) -> &str {
        self.path.as_str()
    }
}

impl ConnectionExt for WasmDbConnection {
    type Connection = SqliteConnection;
    type Error = crate::ConnectionError;

    fn start_transaction(&self) -> Result<TransactionGuard<'_>, Self::Error> {
        let guard = self.transaction_lock.lock();
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
        tracing::info!("{}", self.path());
        let mut conn = self.conn.lock();
        Ok(fun(&mut *conn).map_err(crate::ConnectionError::from)?)
    }

    fn raw_query_write<T, F>(&self, fun: F) -> Result<T, Self::Error>
    where
        F: FnOnce(&mut Self::Connection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        tracing::info!("{}", self.path());
        let mut conn = self.conn.lock();
        Ok(fun(&mut *conn).map_err(crate::ConnectionError::from)?)
    }
}

impl XmtpDb for WasmDb {
    type Error = PlatformStorageError;
    type Connection = super::DefaultConnection;

    fn conn(&self) -> Self::Connection {
        self.conn.clone()
    }

    fn validate(&self, _opts: &StorageOption) -> Result<(), crate::ConnectionError> {
        Ok(())
    }

    fn reconnect(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn disconnect(&self) -> Result<(), Self::Error> {
        Ok(())
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
        F: FnOnce(crate::DbConnection) -> R,
    {
        let util = init_opfs().await;
        let o: Option<&'a str> = path.into();
        let p = o
            .map(|o| String::from(o))
            .unwrap_or(xmtp_common::tmp_path());
        let store = EncryptedMessageStore::new(StorageOption::Persistent(p), [0u8; 32])
            .await
            .unwrap();
        let conn = store.conn().expect("acquiring connection failed");
        let r = f(DbConnection::new(conn));
        if let Ok(u) = util {
            u.wipe_files().await.unwrap();
        }
        r
    }

    #[allow(unused)]
    pub async fn with_opfs_async<'a, F, T, R>(path: impl Into<Option<&'a str>>, f: F) -> R
    where
        F: FnOnce(crate::DbConnection) -> T,
        T: Future<Output = R>,
    {
        let util = init_opfs().await;
        let o: Option<&'a str> = path.into();
        let p = o
            .map(|o| String::from(o))
            .unwrap_or(xmtp_common::tmp_path());
        let store = EncryptedMessageStore::new(StorageOption::Persistent(p), [0u8; 32])
            .await
            .unwrap();
        let conn = store.conn().expect("acquiring connection failed");
        let r = f(DbConnection::new(conn)).await;
        if let Ok(u) = util {
            u.wipe_files().await.unwrap();
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
        if let Some(Ok(util)) = SQLITE.get() {
            util.wipe_files().await.unwrap();
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
                        if let Some(Ok(util)) = SQLITE.get() {
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
