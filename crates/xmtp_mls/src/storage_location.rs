//! Resolve a client's database and attachment paths from a deployment record.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::OnceLock,
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use xmtp_attachments::sanitize_path_component_with_limit;

/// Rust requires each layout's paths. The builder rejects empty paths.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StorageLocation {
    DataDir(PathBuf),
    Explicit {
        db_path: PathBuf,
        attachments_dir: PathBuf,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedPaths {
    pub db_path: PathBuf,
    pub attachments_dir: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum StorageLocationError {
    #[error("storage location needs an inbox id before opening the database")]
    InboxId,
    #[error("data directory needs a backend URL")]
    BackendUrl,
    #[error("offline storage location has no deployment record for this backend URL")]
    OfflineMissingDeployment,
    #[error("store or attachments_dir conflicts with data_location")]
    ConflictingStore,
    #[error("storage location is missing {field}")]
    MissingPath { field: &'static str },
    #[error("deployment identifier differs from the opened data directory")]
    DeploymentMismatch,
    #[error("storage location cannot read or write its deployment record: {0}")]
    Io(#[from] std::io::Error),
    #[error("storage location cannot encode its deployment record: {0}")]
    Json(#[from] serde_json::Error),
    #[error("storage location cannot access OPFS")]
    Opfs,
}

#[derive(Default, Serialize, Deserialize)]
struct DeploymentFile {
    version: u8,
    deployments: BTreeMap<String, String>,
}

static RECORD_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[cfg(all(target_arch = "wasm32", feature = "test-utils"))]
#[doc(hidden)]
#[derive(Clone)]
pub enum DeploymentWriteFault {
    Write,
    Sync,
    Replace,
    Pause {
        entered: std::sync::Arc<tokio::sync::Notify>,
        resume: std::sync::Arc<tokio::sync::Notify>,
    },
}

#[cfg(all(target_arch = "wasm32", feature = "test-utils"))]
static DEPLOYMENT_WRITE_FAULT: parking_lot::Mutex<Option<DeploymentWriteFault>> =
    parking_lot::Mutex::new(None);

#[cfg(all(target_arch = "wasm32", feature = "test-utils"))]
#[doc(hidden)]
pub fn set_deployment_write_fault(fault: DeploymentWriteFault) {
    *DEPLOYMENT_WRITE_FAULT.lock() = Some(fault);
}

#[cfg(all(target_arch = "wasm32", feature = "test-utils"))]
#[doc(hidden)]
pub async fn write_deployments_for_test(
    data_dir: &Path,
    bytes: &[u8],
) -> Result<(), StorageLocationError> {
    write_file(data_dir, bytes).await
}

/// The file name table applies before the suffix. The suffix distinguishes
/// identifiers whose file name table result is the same.
pub fn deployment_component(identifier: &str) -> String {
    let name = sanitize_path_component_with_limit(identifier, 190).to_ascii_lowercase();
    let digest = hex::encode(Sha256::digest(identifier.as_bytes()));
    format!("{name}-{digest}")
}

#[derive(Clone, Debug)]
pub(crate) struct DeploymentRecorder {
    data_dir: PathBuf,
    backend_url: String,
    opened_identifier: Option<String>,
}

impl DeploymentRecorder {
    pub(crate) fn new(data_dir: PathBuf, backend_url: &str) -> Self {
        Self {
            data_dir,
            backend_url: crate::server_configuration::normalized_url(backend_url).to_owned(),
            opened_identifier: None,
        }
    }

    pub(crate) fn for_opened_identifier(mut self, identifier: String) -> Self {
        self.opened_identifier = Some(identifier);
        self
    }

    pub(crate) async fn lookup(&self) -> Result<Option<String>, StorageLocationError> {
        if self.backend_url.is_empty() {
            return Err(StorageLocationError::BackendUrl);
        }
        let _guard = RECORD_LOCK.get_or_init(|| Mutex::new(())).lock().await;
        Ok(read_file(&self.data_dir)
            .await?
            .deployments
            .remove(&self.backend_url))
    }

    pub(crate) async fn record(&self, identifier: &str) -> Result<(), StorageLocationError> {
        if self
            .opened_identifier
            .as_deref()
            .is_some_and(|opened| opened != identifier)
        {
            return Err(StorageLocationError::DeploymentMismatch);
        }
        if self.backend_url.is_empty() {
            return Err(StorageLocationError::BackendUrl);
        }
        let _guard = RECORD_LOCK.get_or_init(|| Mutex::new(())).lock().await;
        let mut file = read_file(&self.data_dir).await?;
        file.version = 1;
        file.deployments
            .insert(self.backend_url.clone(), identifier.to_owned());
        write_file(&self.data_dir, &serde_json::to_vec(&file)?).await
    }
}

impl StorageLocation {
    pub(crate) fn validate(&self) -> Result<(), StorageLocationError> {
        match self {
            Self::DataDir(data_dir) if data_dir.as_os_str().is_empty() => {
                Err(StorageLocationError::MissingPath { field: "data_dir" })
            }
            Self::Explicit { db_path, .. } if db_path.as_os_str().is_empty() => {
                Err(StorageLocationError::MissingPath { field: "db_path" })
            }
            Self::Explicit {
                attachments_dir, ..
            } if attachments_dir.as_os_str().is_empty() => Err(StorageLocationError::MissingPath {
                field: "attachments_dir",
            }),
            _ => Ok(()),
        }
    }

    pub(crate) fn recorder(&self, backend_url: &str) -> Option<DeploymentRecorder> {
        match self {
            Self::DataDir(path) => Some(DeploymentRecorder::new(path.clone(), backend_url)),
            Self::Explicit { .. } => None,
        }
    }

    /// Resolve from a recorded identifier. A missing record is an error;
    /// the builder fetches GetConfiguration before it calls this on a miss.
    pub async fn resolve(
        &self,
        backend_url: &str,
        inbox_id: &str,
    ) -> Result<ResolvedPaths, StorageLocationError> {
        match self {
            Self::Explicit {
                db_path,
                attachments_dir,
            } => Ok(ResolvedPaths {
                db_path: db_path.clone(),
                attachments_dir: attachments_dir.clone(),
            }),
            Self::DataDir(path) => {
                let recorder = DeploymentRecorder::new(path.clone(), backend_url);
                let identifier = recorder
                    .lookup()
                    .await?
                    .ok_or(StorageLocationError::OfflineMissingDeployment)?;
                self.resolve_identifier(inbox_id, &identifier)
            }
        }
    }

    pub(crate) fn resolve_identifier(
        &self,
        inbox_id: &str,
        identifier: &str,
    ) -> Result<ResolvedPaths, StorageLocationError> {
        match self {
            Self::Explicit {
                db_path,
                attachments_dir,
            } => Ok(ResolvedPaths {
                db_path: db_path.clone(),
                attachments_dir: attachments_dir.clone(),
            }),
            Self::DataDir(path) => {
                if inbox_id.is_empty() || !inbox_id.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return Err(StorageLocationError::InboxId);
                }
                let root = path
                    .join(deployment_component(identifier))
                    .join(inbox_id.to_ascii_lowercase());
                Ok(ResolvedPaths {
                    db_path: root.join("xmtp.db3"),
                    attachments_dir: root.join("attachments"),
                })
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
async fn read_file(data_dir: &Path) -> Result<DeploymentFile, StorageLocationError> {
    match tokio::fs::read(data_dir.join("deployments.json")).await {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes).unwrap_or_default()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(DeploymentFile::default()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(not(target_arch = "wasm32"))]
async fn write_file(data_dir: &Path, bytes: &[u8]) -> Result<(), StorageLocationError> {
    use tokio::io::AsyncWriteExt as _;

    xmtp_attachments::create_private_directory(data_dir).await?;
    let path = data_dir.join("deployments.json");
    let temp = data_dir.join(format!(
        ".deployments-{}.tmp",
        xmtp_common::rand_string::<16>()
    ));
    let write = async {
        let mut options = tokio::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options.open(&temp).await?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            file.set_permissions(std::fs::Permissions::from_mode(0o600))
                .await?;
        }
        file.write_all(bytes).await
    }
    .await;
    if let Err(error) = write {
        let _ = tokio::fs::remove_file(&temp).await;
        return Err(error.into());
    }
    if let Err(error) = tokio::fs::rename(&temp, &path).await {
        let _ = tokio::fs::remove_file(&temp).await;
        return Err(error.into());
    }
    Ok(())
}

#[cfg(target_arch = "wasm32")]
async fn read_file(data_dir: &Path) -> Result<DeploymentFile, StorageLocationError> {
    use xmtp_attachments::{LocalStore, OpfsStore};
    let store = OpfsStore::new(&data_dir.to_string_lossy())
        .await
        .map_err(|_| StorageLocationError::Opfs)?;
    if !store
        .exists("deployments.json")
        .await
        .map_err(|_| StorageLocationError::Opfs)?
    {
        return Ok(DeploymentFile::default());
    }
    let file = store
        .open_read("deployments.json")
        .await
        .map_err(|_| StorageLocationError::Opfs)?;
    let mut bytes = Vec::new();
    let mut offset = 0;
    while offset < file.len() {
        let chunk = file
            .read_chunk(offset, 64 * 1024)
            .await
            .map_err(|_| StorageLocationError::Opfs)?;
        if chunk.is_empty() {
            return Err(StorageLocationError::Opfs);
        }
        offset += chunk.len() as u64;
        bytes.extend_from_slice(&chunk);
    }
    Ok(serde_json::from_slice(&bytes).unwrap_or_default())
}

#[cfg(target_arch = "wasm32")]
struct DeploymentTempGuard {
    store: xmtp_attachments::OpfsStore,
    path: String,
    committed: bool,
}

#[cfg(target_arch = "wasm32")]
impl Drop for DeploymentTempGuard {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        use xmtp_attachments::LocalStore;
        let store = self.store.clone();
        let path = self.path.clone();
        drop(xmtp_common::task::spawn(async move {
            if let Err(error) = store.remove_file(&path).await {
                tracing::warn!(?error, %path, "cannot remove temporary deployment record");
            }
        }));
    }
}

#[cfg(target_arch = "wasm32")]
async fn write_file(data_dir: &Path, bytes: &[u8]) -> Result<(), StorageLocationError> {
    use xmtp_attachments::{DownloadSink, LocalStore, OpfsStore};
    let store = OpfsStore::new(&data_dir.to_string_lossy())
        .await
        .map_err(|_| StorageLocationError::Opfs)?;
    let temp = format!(".tmp/deployments-{}", xmtp_common::rand_string::<16>());
    let mut writer = store
        .create_temp(&temp)
        .await
        .map_err(|_| StorageLocationError::Opfs)?;
    let mut guard = DeploymentTempGuard {
        store: store.clone(),
        path: temp.clone(),
        committed: false,
    };
    #[cfg(all(target_arch = "wasm32", feature = "test-utils"))]
    let fault = { DEPLOYMENT_WRITE_FAULT.lock().take() };
    let write_result: Result<(), StorageLocationError> = async {
        #[cfg(all(target_arch = "wasm32", feature = "test-utils"))]
        if let Some(DeploymentWriteFault::Pause { entered, resume }) = &fault {
            entered.notify_one();
            resume.notified().await;
        }
        #[cfg(all(target_arch = "wasm32", feature = "test-utils"))]
        if matches!(fault, Some(DeploymentWriteFault::Write)) {
            return Err(StorageLocationError::Opfs);
        }
        writer
            .write(bytes)
            .await
            .map_err(|_| StorageLocationError::Opfs)?;
        #[cfg(all(target_arch = "wasm32", feature = "test-utils"))]
        if matches!(fault, Some(DeploymentWriteFault::Sync)) {
            return Err(StorageLocationError::Opfs);
        }
        store
            .sync(&mut writer)
            .await
            .map_err(|_| StorageLocationError::Opfs)?;
        Ok(())
    }
    .await;
    drop(writer);
    let result = match write_result {
        Err(error) => Err(error),
        Ok(()) => {
            #[cfg(all(target_arch = "wasm32", feature = "test-utils"))]
            if matches!(fault, Some(DeploymentWriteFault::Replace)) {
                Err(StorageLocationError::Opfs)
            } else {
                store
                    .replace(&temp, "deployments.json")
                    .await
                    .map_err(|_| StorageLocationError::Opfs)
            }
            #[cfg(not(all(target_arch = "wasm32", feature = "test-utils")))]
            // OPFS file move replaces the destination in current browser engines.
            store
                .replace(&temp, "deployments.json")
                .await
                .map_err(|_| StorageLocationError::Opfs)
        }
    };
    if result.is_ok() {
        guard.committed = true;
    } else if let Err(error) = store.remove_file(&temp).await {
        tracing::warn!(?error, %temp, "cannot remove temporary deployment record");
    } else {
        guard.committed = true;
    }
    result
}

#[cfg(test)]
mod tests;
