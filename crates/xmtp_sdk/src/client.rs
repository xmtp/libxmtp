use std::sync::{
    Arc, Weak,
    atomic::{AtomicU64, Ordering},
};
use xmtp_id::associations::{
    AccountId,
    unverified::{NewUnverifiedSmartContractWalletSignature, UnverifiedSignature},
};
use xmtp_mls::{
    builder::{DeviceSyncMode, ForkRecoveryOpts},
    context::XmtpSharedContext,
    identity::IdentityStrategy,
};

use crate::{
    Archives, Attachments, BackendSource, Conversations, Diagnostics, InboxId, InstallationId,
    Preferences, PublicIdentity, Signature, Signer, SignerKind, SigningRequest, Storage, XmtpError,
    signer,
};
use xmtp_common::{MaybeSend, MaybeSync};

pub(crate) type CoreClient = xmtp_mls::Client<xmtp_mls::MlsContext>;

static NEXT_CLIENT_KEY: AtomicU64 = AtomicU64::new(1);

/// Set when a failed create cannot close the store of its client, or when a
/// create or build is cancelled after its store opened. The store
/// can still hold OPFS access handles, so the browser worker must keep its
/// storage lock and end. The flag stays set because the store stays open.
#[cfg(any(test, target_arch = "wasm32"))]
pub(crate) static STORE_LEFT_OPEN: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Makes the next `discard` disconnect fail, to test a store that stays open.
#[cfg(test)]
pub(crate) static FAIL_DISCARD_DISCONNECT: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Reports the store open when a create or build future is dropped after its
/// store may have opened. A cancelled call drops its future at an await, so
/// the cleanup of a failed create does not run. The store and its OPFS access
/// handles can then stay open, so the browser worker must keep its storage
/// lock and end. A create or build that returns disarms the guard, because
/// its own error path closes the store or reports it open.
#[derive(Default)]
struct OpenStoreGuard {
    armed: Arc<std::sync::atomic::AtomicBool>,
}

impl OpenStoreGuard {
    /// A guard with the same state, for the task that runs the call. The
    /// call's own guard then reports the store open when the call is dropped
    /// while its task still runs.
    fn share(&self) -> Self {
        Self {
            armed: self.armed.clone(),
        }
    }

    /// Call before a step that can open the store. An in-memory store holds
    /// no OPFS access handles and no storage lock.
    fn arm(&mut self, storage: &StorageOptions) {
        if !matches!(storage.location, StorageLocation::InMemory) {
            self.armed.store(true, Ordering::Relaxed);
        }
    }

    fn disarm(&self) {
        self.armed.store(false, Ordering::Relaxed);
    }
}

type BuildTaskOutput = (Result<Client, XmtpError>, OpenStoreGuard);

impl Drop for OpenStoreGuard {
    fn drop(&mut self) {
        if !self.armed.swap(false, Ordering::Relaxed) {
            return;
        }
        tracing::error!("a cancelled client create or build can leave its store open");
        #[cfg(any(test, target_arch = "wasm32"))]
        STORE_LEFT_OPEN.store(true, Ordering::Relaxed);
    }
}

/// Run a create or build on a runtime task. Swift polls a call on a
/// cooperative thread whose small stack a debug core build overflows.
/// Dropping the call aborts the task. If the task has already finished, the
/// cleanup task closes its unconsumed client. On wasm32 the work runs inline.
#[cfg(not(target_arch = "wasm32"))]
async fn on_build_task(
    work: xmtp_common::BoxDynFuture<'static, BuildTaskOutput>,
) -> Result<BuildTaskOutput, XmtpError> {
    #[cfg(test)]
    let probe = build_task_probe::CURRENT.try_with(Arc::clone).ok();
    #[cfg(test)]
    let work = {
        let probe = probe.clone();
        Box::pin(async move {
            let result = match &probe {
                Some(probe) => build_task_probe::CURRENT.scope(probe.clone(), work).await,
                None => work.await,
            };
            if let (Some(probe), Ok(client)) = (probe, &result.0) {
                *probe.client.lock() = Some(client.inner.clone());
            }
            result
        })
    };
    struct AbortOnDrop {
        task: Option<tokio::task::JoinHandle<BuildTaskOutput>>,
        runtime: tokio::runtime::Handle,
    }

    impl Drop for AbortOnDrop {
        fn drop(&mut self) {
            let Some(task) = self.task.take() else {
                return;
            };
            task.abort();
            // Abort does not remove an output that the task already returned.
            // Keep ownership until that output has been closed or consumed.
            self.runtime.spawn(async move {
                if let Ok((Ok(client), _guard)) = task.await {
                    let _ = client.discard().await;
                }
            });
        }
    }

    let task = tokio::task::spawn(work);
    #[cfg(test)]
    if let Some(probe) = &probe {
        *probe.task.lock() = Some(task.abort_handle());
        probe.started.notify_one();
    }
    let mut owner = AbortOnDrop {
        task: Some(task),
        runtime: tokio::runtime::Handle::current(),
    };
    let output = owner.task.as_mut().expect("build task").await;
    owner.task.take();
    output.map_err(XmtpError::unknown)
}

#[cfg(all(test, not(target_arch = "wasm32")))]
pub(crate) mod build_task_probe;

#[cfg(target_arch = "wasm32")]
async fn on_build_task(
    work: xmtp_common::BoxDynFuture<'static, BuildTaskOutput>,
) -> Result<BuildTaskOutput, XmtpError> {
    Ok(work.await)
}

/// @xmtp-worker Reports whether storage requires worker termination. A failed
/// or cancelled create can leave a store open. A failed VFS transition can
/// leave partial access handles. Keep the storage lock until the worker ends.
/// Apps do not call this function.
#[cfg(all(target_arch = "wasm32", not(feature = "pure-only")))]
#[uniffi::export]
pub fn storage_requires_worker_restart() -> bool {
    STORE_LEFT_OPEN.load(Ordering::Relaxed) || xmtp_db::opfs_requires_worker_restart()
}

/// @xmtp-internal @xmtp-worker Finish idle storage work before the package terminates its worker.
/// The worker calls this only after all owners and accepted calls have drained.
#[cfg(all(target_arch = "wasm32", not(feature = "pure-only")))]
#[uniffi::export]
pub async fn prepare_storage_for_shutdown() {
    xmtp_db::pause_sqlite_if_idle();
    crate::logging::wait_for_worker_idle().await;
}

#[derive(Default)]
pub(crate) struct EventReaderRegistry {
    closing: bool,
    readers: Vec<Weak<crate::EventReader>>,
}

// Keep exported module paths stable for generated bindings.
include!("client/options.rs");

#[derive(uniffi::Object)]
pub struct Client {
    pub(crate) inner: Arc<CoreClient>,
    pub(crate) key: u64,
    pub(crate) identity: PublicIdentity,
    pub(crate) options: ClientOptions,
    pub(crate) storage_path: Option<String>,
    pub(crate) signer: Option<Arc<dyn Signer>>,
    pub(crate) auth_handle: Option<xmtp_api_backend::AuthHandle>,
    pub(crate) listeners: Arc<crate::events::dispatch::ListenerRegistry>,
    pub(crate) event_readers: Arc<parking_lot::Mutex<EventReaderRegistry>>,
    /// Holds `inbox_state` after it enters the call gate.
    #[cfg(test)]
    pub(crate) call_gate: parking_lot::Mutex<Option<Arc<crate::reader::HandoffGate>>>,
}

mod creation;
#[cfg(feature = "conformance")]
mod event_conformance;
#[cfg(feature = "conformance")]
pub use event_conformance::SdkConformanceListenerCounts;
mod location;

/// Open the database at `path` if its file exists, with the inbox ID of its
/// stored identity. A missing file stays missing.
pub(crate) async fn open_store_if_present(
    options: &StorageOptions,
    path: &std::path::Path,
) -> Result<Option<(xmtp_db::DefaultStore, Option<String>)>, XmtpError> {
    use xmtp_db::{Fetch, identity::StoredIdentity};

    let path = location::path_string(path)?;
    #[cfg(not(target_arch = "wasm32"))]
    let exists = std::path::Path::new(&path).try_exists().map_err(|error| {
        XmtpError::storage_location(format!("database path is unusable: {error}"))
    })?;
    #[cfg(target_arch = "wasm32")]
    let exists = xmtp_db::opfs_database_exists(&path)
        .await
        .map_err(map_wasm_storage_error)?;
    if !exists {
        return Ok(None);
    }
    let store = open_store(options, Some(&path)).await?;
    let stored: Option<StoredIdentity> = store.db().fetch(&()).map_err(XmtpError::from_core)?;
    Ok(Some((store, stored.map(|identity| identity.inbox_id))))
}

/// Open a database that holds a stored identity. Build never creates one.
pub(crate) async fn open_existing_store(
    options: &StorageOptions,
    path: &std::path::Path,
) -> Result<(xmtp_db::DefaultStore, String), XmtpError> {
    match open_store_if_present(options, path).await? {
        Some((store, Some(inbox_id))) => Ok((store, inbox_id)),
        _ => Err(XmtpError::identity_not_found()),
    }
}

include!("client/api.rs");

pub(crate) async fn end_client(
    client: &CoreClient,
    listeners: &crate::events::dispatch::ListenerRegistry,
    event_readers: &parking_lot::Mutex<EventReaderRegistry>,
) -> Result<(), XmtpError> {
    let readers: Vec<_> = {
        let mut registry = event_readers.lock();
        registry.closing = true;
        registry.readers.iter().filter_map(Weak::upgrade).collect()
    };
    listeners.stop_all();
    for reader in &readers {
        reader.close();
    }
    for reader in &readers {
        reader.wait_for_reads().await;
    }
    client.close().await.map_err(XmtpError::from_client)
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn open_store(
    options: &StorageOptions,
    path: Option<&str>,
) -> Result<xmtp_db::DefaultStore, XmtpError> {
    use xmtp_db::{EncryptedMessageStore, EncryptionKey, NativeDb};

    let builder = match path {
        Some(path) => NativeDb::builder().persistent(path.to_owned()),
        None => NativeDb::builder().ephemeral(),
    };
    let min = options
        .pool
        .as_ref()
        .and_then(|pool| pool.min)
        .unwrap_or(xmtp_configuration::MIN_DB_POOL_SIZE);
    let max = options
        .pool
        .as_ref()
        .and_then(|pool| pool.max)
        .unwrap_or(xmtp_configuration::MAX_DB_POOL_SIZE);
    if min > max {
        return Err(XmtpError::invalid("storage pool minimum exceeds maximum"));
    }
    let builder = builder.min_pool_size(min).max_pool_size(max);
    macro_rules! finish {
        ($builder:expr) => {{
            match &options.encryption_key {
                Some(bytes) => {
                    let key =
                        EncryptionKey::try_from(bytes.as_slice()).map_err(XmtpError::from_core)?;
                    $builder.key(key).build().map_err(XmtpError::from_core)?
                }
                None => $builder.build_unencrypted().map_err(XmtpError::from_core)?,
            }
        }};
    }
    let db = if options.single_connection {
        finish!(builder.single_connection())
    } else {
        finish!(builder)
    };
    EncryptedMessageStore::new(db).map_err(XmtpError::from_core)
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn open_store(
    options: &StorageOptions,
    path: Option<&str>,
) -> Result<xmtp_db::DefaultStore, XmtpError> {
    use xmtp_db::{EncryptedMessageStore, StorageOption, WasmDb};

    let _ = options;
    let location = match path {
        Some(path) => StorageOption::Persistent(path.to_owned()),
        None => StorageOption::Ephemeral,
    };
    let db = WasmDb::new_strict(&location)
        .await
        .map_err(map_wasm_storage_error)?;
    EncryptedMessageStore::new(db).map_err(map_wasm_storage_error)
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn map_wasm_storage_error(error: impl crate::error::CoreError) -> XmtpError {
    use xmtp_db::{ConnectionError, OpfsSAHError, PlatformStorageError, StorageError};

    let mut cause: Option<&(dyn std::error::Error + 'static)> = Some(&error);
    while let Some(current) = cause {
        // Transparent storage and connection errors delegate source(). Inspect
        // their variants so a platform error with no source is not lost.
        let platform = match current.downcast_ref::<StorageError>() {
            Some(StorageError::Platform(platform)) => Some(platform),
            Some(StorageError::Connection(ConnectionError::Platform(platform))) => Some(platform),
            _ => match current.downcast_ref::<ConnectionError>() {
                Some(ConnectionError::Platform(platform)) => Some(platform),
                _ => current.downcast_ref::<PlatformStorageError>(),
            },
        };
        match platform {
            Some(PlatformStorageError::InvalidDatabasePath) => {
                return XmtpError::invalid(error.to_string());
            }
            Some(
                PlatformStorageError::DatabaseInUse
                | PlatformStorageError::SAH(OpfsSAHError::CreateSyncAccessHandle(_)),
            ) => return XmtpError::storage_busy(error.to_string()),
            // A browser location needs OPFS, which a Node worker lacks.
            Some(PlatformStorageError::SAH(OpfsSAHError::NotSupported)) => {
                return XmtpError::storage_location(error.to_string());
            }
            _ => cause = current.source(),
        }
    }
    XmtpError::from_core(error)
}

#[cfg(all(test, target_arch = "wasm32"))]
#[path = "client/wasm_storage_tests.rs"]
mod wasm_storage_tests;

#[cfg(all(test, target_arch = "wasm32"))]
mod wasm_storage_error_tests {
    use super::*;

    #[xmtp_common::test(unwrap_try = true)]
    fn unsupported_opfs_is_a_storage_location_failure() {
        let error = map_wasm_storage_error(xmtp_db::PlatformStorageError::SAH(
            xmtp_db::OpfsSAHError::NotSupported,
        ));
        let XmtpError::StorageLocation(details) = error else {
            panic!("unsupported OPFS must report StorageLocation: {error:?}");
        };
        assert_eq!(details.code, "StorageLocation");
        assert!(matches!(details.category, crate::ErrorCategory::Storage));
        assert!(!details.retryable);
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn unsupported_opfs_is_not_storage_busy() {
        let error = XmtpError::from_core(xmtp_db::PlatformStorageError::SAH(
            xmtp_db::OpfsSAHError::NotSupported,
        ));
        let XmtpError::Storage(details) = error else {
            panic!("a generic OPFS failure must report Storage: {error:?}");
        };
        assert_eq!(details.code, "Storage");
        assert!(matches!(details.category, crate::ErrorCategory::Storage));
        assert!(!details.retryable);
    }
}
