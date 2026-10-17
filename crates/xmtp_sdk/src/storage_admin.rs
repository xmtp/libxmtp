use std::future::Future;

use parking_lot::Mutex;
use tokio_util::sync::CancellationToken;

use crate::{XmtpError, client::map_wasm_storage_error};

#[derive(Default)]
struct State {
    ended: bool,
    active: usize,
}

/// One browser storage admin handle. The worker owns its OPFS pool lease.
#[derive(uniffi::Object)]
pub struct StorageAdmin {
    state: Mutex<State>,
    drained: CancellationToken,
}

struct Operation<'a>(&'a StorageAdmin);

impl Drop for Operation<'_> {
    fn drop(&mut self) {
        let mut state = self.0.state.lock();
        state.active -= 1;
        if state.ended && state.active == 0 {
            self.0.drained.cancel();
        }
    }
}

impl StorageAdmin {
    async fn run<T>(
        &self,
        operation: impl Future<Output = Result<T, xmtp_db::StorageError>>,
    ) -> Result<T, XmtpError> {
        {
            let mut state = self.state.lock();
            if state.ended {
                return Err(XmtpError::closed());
            }
            state.active += 1;
        }
        let _operation = Operation(self);
        operation.await.map_err(map_wasm_storage_error)
    }
}

#[xmtp_macro::sdk_export]
impl StorageAdmin {
    #[uniffi::constructor]
    pub async fn open() -> Result<Self, XmtpError> {
        xmtp_db::opfs_pool_capacity()
            .await
            .map_err(map_wasm_storage_error)?;
        Ok(Self {
            state: Mutex::new(State::default()),
            drained: CancellationToken::new(),
        })
    }

    pub async fn list_files(&self) -> Result<Vec<String>, XmtpError> {
        self.run(xmtp_db::list_opfs_databases()).await
    }

    pub async fn file_count(&self) -> Result<u32, XmtpError> {
        self.run(xmtp_db::opfs_database_count()).await
    }

    pub async fn pool_capacity(&self) -> Result<u32, XmtpError> {
        self.run(xmtp_db::opfs_pool_capacity()).await
    }

    pub async fn file_exists(&self, path: String) -> Result<bool, XmtpError> {
        self.run(xmtp_db::opfs_database_exists(&path)).await
    }

    /// Export requires a closed target; the VFS cannot snapshot an open database.
    pub async fn export_db(&self, path: String) -> Result<Vec<u8>, XmtpError> {
        self.run(xmtp_db::export_opfs_database(&path)).await
    }

    /// Trust boundary: import checks SQLite integrity and the libxmtp
    /// migration version. It does not check table definitions, triggers, or
    /// rows. Import only databases from a trusted source.
    pub async fn import_db(&self, path: String, data: Vec<u8>) -> Result<(), XmtpError> {
        self.run(xmtp_db::import_opfs_database(&path, &data)).await
    }

    pub async fn delete_file(&self, path: String) -> Result<bool, XmtpError> {
        self.run(xmtp_db::delete_opfs_database(&path)).await
    }

    pub async fn clear_all(&self) -> Result<(), XmtpError> {
        self.run(xmtp_db::clear_opfs_databases()).await
    }

    /// Fence new calls and wait for calls already accepted by this handle.
    pub async fn end(&self) -> Result<(), XmtpError> {
        {
            let mut state = self.state.lock();
            state.ended = true;
            if state.active == 0 {
                self.drained.cancel();
            }
        }
        self.drained.cancelled().await;
        xmtp_db::pause_sqlite_if_idle();
        Ok(())
    }
}

#[cfg(test)]
mod tests;
