//! WebAssembly specific connection for a SQLite Database
//! Stores a single connection behind a mutex that's used for every libxmtp operation
use crate::{StorageOption, XmtpDb, db_connection::DbConnectionPrivate};
use diesel::prelude::SqliteConnection;
use diesel::{connection::AnsiTransactionManager, prelude::*};
use parking_lot::Mutex;
use sqlite_wasm_rs::export::OpfsSAHPoolCfg;
use std::sync::Arc;
use thiserror::Error;
use web_sys::wasm_bindgen::JsCast;

#[derive(Debug, Error)]
pub enum WasmStorageError {
    #[error("OPFS {0}")]
    SAH(#[from] OpfsSAHError),
    #[error(transparent)]
    Connection(#[from] diesel::ConnectionError),
}

#[derive(Clone)]
pub struct WasmDb {
    conn: Arc<Mutex<SqliteConnection>>,
    opts: StorageOption,
    transaction_lock: Arc<Mutex<()>>,
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

async fn maybe_resize() -> Result<(), WasmStorageError> {
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
    pub async fn new(opts: &StorageOption) -> Result<Self, WasmStorageError> {
        use crate::StorageOption::*;
        init_sqlite().await;
        maybe_resize().await?;
        let conn = match opts {
            Ephemeral => {
                let name = xmtp_common::rand_string::<12>();
                let name = format!("file:/xmtp-ephemeral-{}.db?vfs=memdb", name);
                SqliteConnection::establish(name.as_str())
            }
            Persistent(db_path) => {
                tracing::debug!("creating persistent opfs db @{}", db_path);
                SqliteConnection::establish(db_path)
            }
        }?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            opts: opts.clone(),
            transaction_lock: Arc::new(Mutex::new(())),
        })
    }
}

impl XmtpDb for WasmDb {
    type Error = WasmStorageError;
    type Connection = SqliteConnection;
    type TransactionManager = AnsiTransactionManager;

    fn conn(&self) -> Result<DbConnectionPrivate<Self::Connection>, Self::Error> {
        Ok(DbConnectionPrivate::from_arc_mutex(
            self.conn.clone(),
            None,
            self.transaction_lock.clone(),
        ))
    }

    fn validate(&self, _opts: &StorageOption) -> Result<(), Self::Error> {
        Ok(())
    }

    fn release_connection(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn reconnect(&self) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);
    use crate::EncryptedMessageStore;
    use crate::group_intent::IntentKind;
    use crate::group_intent::NewGroupIntent;

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
        let r = f(conn);
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
        let r = f(conn).await;
        if let Ok(u) = util {
            u.wipe_files().await.unwrap();
        }
        r
    }

    #[xmtp_common::test]
    async fn test_opfs() {
        use crate::traits::Store;

        xmtp_common::logger();
        let path = "test_db";
        with_opfs(path, |c1| {
            let intent = NewGroupIntent::builder()
                .kind(IntentKind::SendMessage)
                .group_id(vec![0, 1, 1, 1])
                .data(vec![0, 0, 0, 0])
                .should_push(false)
                .build()
                .unwrap();
            intent.store(&c1).unwrap();
        })
        .await;
    }

    #[xmtp_common::test]
    async fn opfs_dynamically_resizes() {
        use xmtp_common::tmp_path as path;
        xmtp_common::logger();
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
