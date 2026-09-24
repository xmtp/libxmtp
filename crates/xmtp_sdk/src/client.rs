use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use xmtp_id::associations::{
    AccountId,
    unverified::{NewUnverifiedSmartContractWalletSignature, UnverifiedSignature},
};
use xmtp_mls::{
    builder::{DeviceSyncMode, ForkRecoveryOpts},
    identity::IdentityStrategy,
};

use crate::{
    BackendSource, Conversations, InboxID, InstallationID, PublicIdentity, Signature, Signer,
    SignerKind, SigningRequest, XmtpError, signer,
};

pub(crate) type CoreClient = xmtp_mls::Client<xmtp_mls::MlsContext>;

static NEXT_CLIENT_KEY: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Default, uniffi::Enum)]
pub enum StorageLocation {
    #[default]
    Default,
    InMemory,
    Directory(String),
    Path(String),
}

#[derive(Clone, Debug, Default, uniffi::Record)]
pub struct StorageOptions {
    pub location: StorageLocation,
    #[uniffi(default = None)]
    pub label: Option<String>,
    #[uniffi(default = None)]
    pub encryption_key: Option<Vec<u8>>,
    #[uniffi(default = None)]
    pub pool: Option<StoragePoolOptions>,
    #[uniffi(default = false)]
    pub single_connection: bool,
}

#[derive(Clone, Debug, Default, uniffi::Record)]
pub struct StoragePoolOptions {
    #[uniffi(default = None)]
    pub min: Option<u32>,
    #[uniffi(default = None)]
    pub max: Option<u32>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct RegistrationOptions {
    #[uniffi(default = true)]
    pub auto: bool,
    #[uniffi(default = None)]
    pub nonce: Option<u64>,
}

impl Default for RegistrationOptions {
    fn default() -> Self {
        Self {
            auto: true,
            nonce: None,
        }
    }
}

#[derive(Clone, Debug, Default, uniffi::Enum)]
pub enum ForkRecoveryPolicy {
    #[default]
    None,
    AllowlistedGroups,
    All,
}

#[derive(Clone, Debug, Default, uniffi::Record)]
pub struct ForkRecoveryOptions {
    pub policy: ForkRecoveryPolicy,
    #[uniffi(default)]
    pub groups: Vec<crate::ConversationID>,
    #[uniffi(default = false)]
    pub disable_responses: bool,
    #[uniffi(default = None)]
    pub worker_interval_ns: Option<u64>,
}

impl From<ForkRecoveryOptions> for ForkRecoveryOpts {
    fn from(value: ForkRecoveryOptions) -> Self {
        use xmtp_mls::builder::ForkRecoveryPolicy as CorePolicy;
        Self {
            enable_recovery_requests: match value.policy {
                ForkRecoveryPolicy::None => CorePolicy::None,
                ForkRecoveryPolicy::AllowlistedGroups => CorePolicy::AllowlistedGroups,
                ForkRecoveryPolicy::All => CorePolicy::All,
            },
            groups_to_request_recovery: value.groups.into_iter().map(|id| id.0).collect(),
            disable_recovery_responses: value.disable_responses,
            worker_interval_ns: value.worker_interval_ns,
        }
    }
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum WorkerKind {
    DeviceSync,
    DisappearingMessages,
    KeyPackageCleaner,
    CommitLog,
    TaskRunner,
    ConfigurationRefresh,
    HmacEpoch,
}

impl From<WorkerKind> for xmtp_mls::worker::WorkerKind {
    fn from(value: WorkerKind) -> Self {
        use xmtp_mls::worker::WorkerKind as CoreKind;
        match value {
            WorkerKind::DeviceSync => CoreKind::DeviceSync,
            WorkerKind::DisappearingMessages => CoreKind::DisappearingMessages,
            WorkerKind::KeyPackageCleaner => CoreKind::KeyPackageCleaner,
            WorkerKind::CommitLog => CoreKind::CommitLog,
            WorkerKind::TaskRunner => CoreKind::TaskRunner,
            WorkerKind::ConfigurationRefresh => CoreKind::ConfigurationRefresh,
            WorkerKind::HmacEpoch => CoreKind::HmacEpoch,
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct WorkerInterval {
    pub kind: WorkerKind,
    pub interval_ns: Option<u64>,
    pub jitter_ns: Option<u64>,
    pub enabled: Option<bool>,
}

#[derive(Clone, Debug, Default, uniffi::Record)]
pub struct WorkerOptions {
    #[uniffi(default = None)]
    pub default_interval_ns: Option<u64>,
    #[uniffi(default)]
    pub intervals: Vec<WorkerInterval>,
}

impl From<WorkerOptions> for xmtp_mls::worker::WorkerConfig {
    fn from(value: WorkerOptions) -> Self {
        let mut config = Self {
            default_interval_ns: value.default_interval_ns,
            ..Default::default()
        };
        for entry in value.intervals {
            let kind = entry.kind.into();
            if let Some(interval) = entry.interval_ns {
                config.interval_overrides.insert(kind, interval);
            }
            if let Some(jitter) = entry.jitter_ns {
                config.jitter_overrides.insert(kind, jitter);
            }
            if let Some(enabled) = entry.enabled {
                config.enabled.insert(kind, enabled);
            }
        }
        config
    }
}

#[derive(Clone, uniffi::Record)]
pub struct ClientOptions {
    /// Omission uses empty connection options, as the old field default did.
    #[uniffi(default = None)]
    pub backend: Option<BackendSource>,
    pub storage: StorageOptions,
    #[uniffi(default = true)]
    pub device_sync: bool,
    #[uniffi(default)]
    pub registration: RegistrationOptions,
    #[uniffi(default = None)]
    pub fork_recovery: Option<ForkRecoveryOptions>,
    #[uniffi(default = None)]
    pub workers: Option<WorkerOptions>,
}

impl Default for ClientOptions {
    fn default() -> Self {
        Self {
            backend: None,
            storage: StorageOptions::default(),
            device_sync: true,
            registration: RegistrationOptions::default(),
            fork_recovery: None,
            workers: None,
        }
    }
}

#[derive(uniffi::Object)]
pub struct Client {
    pub(crate) inner: Arc<CoreClient>,
    pub(crate) key: u64,
    pub(crate) identity: PublicIdentity,
    pub(crate) options: ClientOptions,
    pub(crate) signer: Option<Arc<dyn Signer>>,
    pub(crate) auth_handle: Option<xmtp_api_backend::AuthHandle>,
}

impl Client {
    async fn build_inner(
        identity: PublicIdentity,
        options: ClientOptions,
        inbox_id: Option<InboxID>,
    ) -> Result<Self, XmtpError> {
        if matches!(&options.storage.location, StorageLocation::Default) {
            return Err(XmtpError::storage_location_required());
        }
        let identifier = identity.to_core()?;
        let backend = options
            .backend
            .clone()
            .unwrap_or_default()
            .resolve()
            .await?;
        let auth_handle = backend.auth_handle.clone();
        let inbox_id = match inbox_id {
            Some(value) => value.0,
            None => {
                let api = xmtp_api::ApiClientWrapper::new(backend.api.clone(), Default::default());
                let found = api
                    .get_inbox_ids(vec![identifier.clone().into()])
                    .await
                    .map_err(XmtpError::from_api)?;
                match found.into_iter().next().flatten() {
                    Some(value) => value,
                    None => identifier
                        .inbox_id(options.registration.nonce.unwrap_or(0))
                        .map_err(XmtpError::unknown)?,
                }
            }
        };
        let store = open_store(&options.storage, &inbox_id).await?;
        let mode = if options.device_sync {
            DeviceSyncMode::Enabled
        } else {
            DeviceSyncMode::Disabled
        };
        let mut builder = xmtp_mls::Client::builder(IdentityStrategy::new(
            inbox_id,
            identifier,
            options.registration.nonce.unwrap_or(0),
            None,
        ))
        .api_client_with_streams(backend.api.clone())
        .with_remote_verifier()
        .map_err(XmtpError::unknown)?
        .store(store)
        .device_sync_worker_mode(mode);
        if let Some(recovery) = options.fork_recovery.clone() {
            builder = builder.fork_recovery_opts(recovery.into());
        }
        if let Some(workers) = options.workers.clone() {
            builder = builder.worker_config(workers.into());
        }
        let inner = builder
            .default_mls_store()
            .map_err(XmtpError::unknown)?
            .build()
            .await
            .map_err(XmtpError::from_builder)?;
        let key = NEXT_CLIENT_KEY
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |key| {
                key.checked_add(1)
            })
            .map_err(|_| XmtpError::unknown("client key space exhausted"))?;
        Ok(Self {
            inner: Arc::new(inner),
            key,
            identity,
            options,
            signer: None,
            auth_handle,
        })
    }

    pub(crate) async fn register_with_signer(
        &self,
        signer: Arc<dyn Signer>,
        kind: SignerKind,
    ) -> Result<(), XmtpError> {
        let Some(mut request) = self.inner.identity().signature_request() else {
            return Ok(());
        };
        let signature = signer::sign(
            signer,
            SigningRequest {
                text: request.signature_text(),
            },
        )
        .await?;
        let verifier = self.inner.scw_verifier();
        match (kind, signature) {
            (SignerKind::Eoa, Signature::Ecdsa(bytes)) => {
                request
                    .add_signature(UnverifiedSignature::new_recoverable_ecdsa(bytes), &verifier)
                    .await
                    .map_err(XmtpError::unknown)?;
            }
            (
                SignerKind::Passkey,
                Signature::Passkey {
                    signature,
                    public_key,
                    authenticator_data,
                    client_data_json,
                },
            ) => {
                request
                    .add_signature(
                        UnverifiedSignature::new_passkey(
                            public_key,
                            signature,
                            authenticator_data,
                            client_data_json,
                        ),
                        &verifier,
                    )
                    .await
                    .map_err(XmtpError::unknown)?;
            }
            (
                SignerKind::Scw { chain_id, .. },
                Signature::Scw {
                    bytes,
                    address,
                    chain_id: signed_chain_id,
                    block_number: signed_block,
                },
            ) if chain_id == signed_chain_id => {
                request
                    .add_new_unverified_smart_contract_signature(
                        NewUnverifiedSmartContractWalletSignature::new(
                            bytes,
                            AccountId::new_evm(chain_id, address),
                            signed_block,
                        ),
                        &verifier,
                    )
                    .await
                    .map_err(XmtpError::unknown)?;
            }
            _ => return Err(XmtpError::invalid("signature does not match signer kind")),
        }
        self.inner
            .register_identity(request)
            .await
            .map_err(XmtpError::from_client)
    }
}

#[xmtp_macro::sdk_export]
impl Client {
    #[uniffi::constructor]
    pub async fn create(
        signer: Arc<dyn Signer>,
        options: ClientOptions,
    ) -> Result<Self, XmtpError> {
        let identity = signer::identity(signer.clone()).await?;
        let kind = signer::kind(signer.clone()).await?;
        let mut client = Self::build_inner(identity, options, None).await?;
        if client.options.registration.auto {
            client.register_with_signer(signer.clone(), kind).await?;
        }
        client.signer = Some(signer);
        Ok(client)
    }

    /// Without an inbox ID, build queries the backend, so an offline app must pass the inbox ID.
    #[uniffi::constructor]
    pub async fn build(
        identity: PublicIdentity,
        options: ClientOptions,
        inbox_id: Option<InboxID>,
    ) -> Result<Self, XmtpError> {
        Self::build_inner(identity, options, inbox_id).await
    }

    pub fn inbox_id(&self) -> InboxID {
        InboxID(self.inner.inbox_id().to_owned())
    }

    pub fn installation_id(&self) -> InstallationID {
        InstallationID(self.inner.installation_public_key().to_string())
    }

    /// Host runtimes use this key to find the owner of a lifted message.
    pub fn client_key(&self) -> u64 {
        self.key
    }

    pub fn conversations(&self) -> Arc<Conversations> {
        Arc::new(Conversations {
            client: self.inner.clone(),
            client_key: self.key,
        })
    }

    pub async fn end(&self) -> Result<(), XmtpError> {
        self.inner.close().await.map_err(XmtpError::from_client)
    }
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

fn database_name(options: &StorageOptions, inbox_id: &str) -> Result<String, XmtpError> {
    let label = options.label.as_deref().unwrap_or("");
    if [label, inbox_id]
        .iter()
        .any(|part| part.contains('/') || part.contains('\\') || part.chars().any(char::is_control))
    {
        return Err(XmtpError::invalid(
            "storage label or inbox ID contains a path separator",
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
    use xmtp_db::{EncryptedMessageStore, StorageOption, WasmDb};

    if options.encryption_key.is_some() {
        return Err(XmtpError::invalid(
            "encrypted wasm storage is not available",
        ));
    }
    let location = match &options.location {
        StorageLocation::InMemory => StorageOption::Ephemeral,
        StorageLocation::Default => return Err(XmtpError::storage_location_required()),
        StorageLocation::Directory(directory) => {
            let name = database_name(options, inbox_id)?;
            StorageOption::Persistent(format!("{}/{name}", directory.trim_end_matches('/')))
        }
        StorageLocation::Path(path) => StorageOption::Persistent(path.clone()),
    };
    let db = WasmDb::new(&location).await.map_err(XmtpError::unknown)?;
    EncryptedMessageStore::new(db).map_err(XmtpError::unknown)
}
