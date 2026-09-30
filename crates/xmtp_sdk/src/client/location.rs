//! Storage locations of file-backed clients.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use xmtp_mls::{
    builder::ClientBuilderError,
    storage_location::{ResolvedPaths, StorageLocation as CoreLocation, StorageLocationError},
};

use super::{StorageLocation, StorageOptions, open_existing_store, open_store};
use crate::XmtpError;

/// The storage layout of the options, or `None` for in-memory storage. The
/// default location is the host's to resolve.
// implements: STORE-020
// implements: STORE-021
pub(super) fn core_location(storage: &StorageOptions) -> Result<Option<CoreLocation>, XmtpError> {
    let label = storage.label.as_deref().filter(|label| !label.is_empty());
    if let Some(label) = label
        && (matches!(label, "." | "..") || label.contains(['/', '\\', ':', '\0']))
    {
        return Err(XmtpError::storage_location(
            "storage label must name one directory",
        ));
    }
    match &storage.location {
        StorageLocation::Default => Err(XmtpError::storage_location_required()),
        StorageLocation::InMemory => Ok(None),
        StorageLocation::Directory { directory } => {
            let mut data_dir = host_path(directory, "directory")?;
            if let Some(label) = label {
                data_dir.push(label);
            }
            Ok(Some(CoreLocation::DataDir(data_dir)))
        }
        StorageLocation::Explicit {
            db_path,
            attachments_dir,
        } => Ok(Some(CoreLocation::Explicit {
            db_path: host_path(db_path, "dbPath")?,
            attachments_dir: host_path(attachments_dir, "attachmentsDir")?,
        })),
    }
}

// A relative native path resolves against the working directory at client
// creation. Browser paths name files in the OPFS pool and stay relative.
fn host_path(path: &str, field: &str) -> Result<PathBuf, XmtpError> {
    if path.is_empty() {
        return Err(XmtpError::storage_location(format!(
            "storage {field} is empty"
        )));
    }
    #[cfg(not(target_arch = "wasm32"))]
    let path = std::path::absolute(path).map_err(|error| {
        XmtpError::storage_location(format!("storage {field} is unusable: {error}"))
    })?;
    #[cfg(target_arch = "wasm32")]
    let path = PathBuf::from(path);
    Ok(path)
}

pub(crate) fn path_string(path: &Path) -> Result<String, XmtpError> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| XmtpError::storage_location("storage path is not UTF-8"))
}

/// A store that the SDK opened before the build resolved its location.
pub(super) struct OpenedStore {
    pub(super) path: PathBuf,
    pub(super) store: xmtp_db::DefaultStore,
}

/// What the store opener reports back to the build.
#[derive(Default)]
pub(super) struct OpenOutcome {
    /// The database file the client opened.
    pub(super) path: Option<String>,
    /// The SDK error behind a failed open. The builder error loses its kind.
    pub(super) error: Option<XmtpError>,
}

/// Open the store at the paths the builder resolves. Build opens only a
/// database that holds an identity; create opens or creates one.
pub(super) fn store_opener(
    storage: StorageOptions,
    opened: Option<OpenedStore>,
    require_stored_identity: bool,
    outcome: Arc<parking_lot::Mutex<OpenOutcome>>,
) -> impl FnOnce(
    ResolvedPaths,
)
    -> xmtp_common::BoxDynFuture<'static, Result<xmtp_db::DefaultStore, ClientBuilderError>>
+ xmtp_common::MaybeSend
+ xmtp_common::MaybeSync
+ 'static {
    let opened = parking_lot::Mutex::new(opened);
    move |paths| {
        let opened = opened.into_inner();
        Box::pin(async move {
            let result = match opened {
                Some(opened) if opened.path == paths.db_path => Ok(opened.store),
                Some(_) => Err(XmtpError::storage_location(
                    "storage location changed while the client opened",
                )),
                None if require_stored_identity => open_existing_store(&storage, &paths.db_path)
                    .await
                    .map(|(store, _)| store),
                None => open_new_store(&storage, &paths.db_path).await,
            };
            let mut outcome = outcome.lock();
            match result.and_then(|store| Ok((path_string(&paths.db_path)?, store))) {
                Ok((path, store)) => {
                    outcome.path = Some(path);
                    Ok(store)
                }
                Err(error) => {
                    let message = error.to_string();
                    outcome.error = Some(error);
                    Err(StorageLocationError::Io(std::io::Error::other(message)).into())
                }
            }
        })
    }
}

async fn open_new_store(
    storage: &StorageOptions,
    path: &Path,
) -> Result<xmtp_db::DefaultStore, XmtpError> {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(parent) = path.parent() {
        xmtp_attachments::create_private_directory(parent)
            .await
            .map_err(|error| {
                XmtpError::storage_location(format!("database directory is unusable: {error}"))
            })?;
    }
    open_store(storage, Some(&path_string(path)?)).await
}
