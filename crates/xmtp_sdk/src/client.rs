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
    Archives, BackendSource, Conversations, Diagnostics, InboxId, InstallationId, Preferences,
    PublicIdentity, Signature, Signer, SignerKind, SigningRequest, Storage, XmtpError, signer,
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
    armed: bool,
}

impl OpenStoreGuard {
    /// Call before a step that can open the store. An in-memory store holds
    /// no OPFS access handles and no storage lock.
    fn arm(&mut self, storage: &StorageOptions) {
        self.armed |= !matches!(storage.location, StorageLocation::InMemory);
    }

    fn disarm(mut self) {
        self.armed = false;
    }
}

impl Drop for OpenStoreGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        tracing::error!("a cancelled client create or build can leave its store open");
        #[cfg(any(test, target_arch = "wasm32"))]
        STORE_LEFT_OPEN.store(true, Ordering::Relaxed);
    }
}

/// @xmtp-worker Reports whether a failed create left the store of its client
/// open. The browser worker reads this after a failed create, before it
/// releases the storage lock. Apps do not call it.
#[cfg(all(target_arch = "wasm32", not(feature = "pure-only")))]
#[uniffi::export]
pub fn store_left_open() -> bool {
    STORE_LEFT_OPEN.load(Ordering::Relaxed)
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

async fn open_existing_store(
    options: &StorageOptions,
    inbox_id: &str,
) -> Result<xmtp_db::DefaultStore, XmtpError> {
    use xmtp_db::{Fetch, identity::StoredIdentity};

    if matches!(options.location, StorageLocation::InMemory) {
        return Err(XmtpError::identity_not_found());
    }
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(path) = native_storage_path(options, inbox_id)?
        && !std::path::Path::new(&path)
            .try_exists()
            .map_err(XmtpError::storage)?
    {
        return Err(XmtpError::identity_not_found());
    }
    #[cfg(target_arch = "wasm32")]
    {
        let path =
            wasm_storage_path(options, inbox_id)?.ok_or_else(XmtpError::identity_not_found)?;
        if !xmtp_db::opfs_database_exists(&path)
            .await
            .map_err(map_wasm_storage_error)?
        {
            return Err(XmtpError::identity_not_found());
        }
    }
    let store = open_store(options, inbox_id).await?;
    let stored: Option<StoredIdentity> = store.db().fetch(&()).map_err(XmtpError::unknown)?;
    if stored.is_none() {
        return Err(XmtpError::identity_not_found());
    }
    Ok(store)
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
    inbox_id: &str,
) -> Result<xmtp_db::DefaultStore, XmtpError> {
    use xmtp_db::{EncryptedMessageStore, EncryptionKey, NativeDb};

    let path = native_storage_path(options, inbox_id)?;
    let builder = match path {
        Some(path) => NativeDb::builder().persistent(path),
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
                        EncryptionKey::try_from(bytes.as_slice()).map_err(XmtpError::unknown)?;
                    $builder.key(key).build().map_err(XmtpError::unknown)?
                }
                None => $builder.build_unencrypted().map_err(XmtpError::unknown)?,
            }
        }};
    }
    let db = if options.single_connection {
        finish!(builder.single_connection())
    } else {
        finish!(builder)
    };
    EncryptedMessageStore::new(db).map_err(XmtpError::unknown)
}

pub(crate) fn database_name(options: &StorageOptions, inbox_id: &str) -> Result<String, XmtpError> {
    let label = options.label.as_deref().unwrap_or("");
    if [label, inbox_id].iter().any(|part| {
        part.contains('/')
            || part.contains('\\')
            || part.contains(':')
            || part.chars().any(char::is_control)
    }) {
        return Err(XmtpError::invalid(
            "storage label or inbox ID contains an unsafe character",
        ));
    }
    let label = if label.is_empty() {
        String::new()
    } else {
        format!("{label}-")
    };
    Ok(format!("xmtp-{label}{inbox_id}.db3"))
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn native_storage_path(
    options: &StorageOptions,
    inbox_id: &str,
) -> Result<Option<String>, XmtpError> {
    let path = match &options.location {
        StorageLocation::InMemory => None,
        StorageLocation::Path(path) => Some(path.clone()),
        StorageLocation::Directory(directory) => {
            let mut builder = std::fs::DirBuilder::new();
            builder.recursive(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder.create(directory).map_err(XmtpError::unknown)?;
            Some(
                std::path::Path::new(directory)
                    .join(database_name(options, inbox_id)?)
                    .to_string_lossy()
                    .into_owned(),
            )
        }
        StorageLocation::Default => return Err(XmtpError::storage_location_required()),
    };
    Ok(path)
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn open_store(
    options: &StorageOptions,
    inbox_id: &str,
) -> Result<xmtp_db::DefaultStore, XmtpError> {
    use xmtp_db::{EncryptedMessageStore, WasmDb};

    let location = wasm_store_location(options, inbox_id)?;
    let db = WasmDb::new(&location)
        .await
        .map_err(map_wasm_storage_error)?;
    EncryptedMessageStore::new(db).map_err(map_wasm_storage_error)
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn map_wasm_storage_error(error: impl std::error::Error + 'static) -> XmtpError {
    use xmtp_db::{OpfsSAHError, PlatformStorageError, StorageError};

    let mut cause: Option<&(dyn std::error::Error + 'static)> = Some(&error);
    while let Some(current) = cause {
        // A transparent StorageError delegates source() to the platform error.
        // Inspect its variant so a platform error with no source is not lost.
        let platform = match current.downcast_ref::<StorageError>() {
            Some(StorageError::Platform(platform)) => Some(platform),
            _ => current.downcast_ref::<PlatformStorageError>(),
        };
        match platform {
            Some(PlatformStorageError::InvalidDatabasePath) => {
                return XmtpError::invalid(error.to_string());
            }
            Some(
                PlatformStorageError::DatabaseInUse
                | PlatformStorageError::SAH(OpfsSAHError::CreateSyncAccessHandle(_)),
            ) => return XmtpError::storage_busy(error.to_string()),
            _ => cause = current.source(),
        }
    }
    XmtpError::unknown(error)
}

#[cfg(all(test, target_arch = "wasm32"))]
#[path = "client/wasm_storage_tests.rs"]
mod wasm_storage_tests;

#[cfg(all(test, target_arch = "wasm32"))]
mod wasm_storage_error_tests {
    use super::*;

    #[xmtp_common::test]
    fn unsupported_opfs_is_not_storage_busy() {
        let error = map_wasm_storage_error(xmtp_db::PlatformStorageError::SAH(
            xmtp_db::OpfsSAHError::NotSupported,
        ));
        assert!(matches!(error, XmtpError::Unknown(_)));
    }
}

#[cfg(any(test, target_arch = "wasm32"))]
pub(crate) fn wasm_storage_path(
    options: &StorageOptions,
    inbox_id: &str,
) -> Result<Option<String>, XmtpError> {
    match &options.location {
        StorageLocation::InMemory => Ok(None),
        StorageLocation::Default => Err(XmtpError::storage_location_required()),
        StorageLocation::Directory(directory) => {
            let name = database_name(options, inbox_id)?;
            Ok(Some(format!("{}/{name}", directory.trim_end_matches('/'))))
        }
        StorageLocation::Path(path) => Ok(Some(path.clone())),
    }
}

#[cfg(any(test, target_arch = "wasm32"))]
pub(crate) fn wasm_store_location(
    options: &StorageOptions,
    inbox_id: &str,
) -> Result<xmtp_db::StorageOption, XmtpError> {
    use xmtp_db::StorageOption;

    let location = match wasm_storage_path(options, inbox_id)? {
        None => StorageOption::Ephemeral,
        Some(path) => StorageOption::Persistent(path),
    };
    Ok(location)
}
