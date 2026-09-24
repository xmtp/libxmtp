use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use xmtp_id::associations::{
    AccountId,
    unverified::{NewUnverifiedSmartContractWalletSignature, UnverifiedSignature},
};
use xmtp_mls::{builder::DeviceSyncMode, identity::IdentityStrategy};

use crate::{
    Backend, BackendOptions, Conversations, InboxID, InstallationID, PublicIdentity, Signature,
    Signer, SignerKind, SigningRequest, XmtpError, signer,
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
}

#[derive(Clone, uniffi::Record)]
pub struct ClientOptions {
    #[uniffi(default)]
    pub backend: BackendOptions,
    pub storage: StorageOptions,
    #[uniffi(default = true)]
    pub device_sync: bool,
}

impl Default for ClientOptions {
    fn default() -> Self {
        Self {
            backend: BackendOptions::default(),
            storage: StorageOptions::default(),
            device_sync: true,
        }
    }
}

#[derive(uniffi::Object)]
pub struct Client {
    pub(crate) inner: Arc<CoreClient>,
    pub(crate) key: u64,
}

impl Client {
    async fn build_inner(
        identity: PublicIdentity,
        options: ClientOptions,
        inbox_id: Option<InboxID>,
    ) -> Result<Self, XmtpError> {
        let identifier = identity.to_core()?;
        let inbox_id = match inbox_id {
            Some(value) => value.0,
            None => identifier.inbox_id(0).map_err(XmtpError::unknown)?,
        };
        let backend = Backend::from_options(options.backend)?;
        let store = open_store(&options.storage, &inbox_id).await?;
        let mode = if options.device_sync {
            DeviceSyncMode::Enabled
        } else {
            DeviceSyncMode::Disabled
        };
        let inner = xmtp_mls::Client::builder(IdentityStrategy::new(inbox_id, identifier, 0, None))
            .api_client_with_streams(backend.api)
            .with_remote_verifier()
            .map_err(XmtpError::unknown)?
            .store(store)
            .device_sync_worker_mode(mode)
            .default_mls_store()
            .map_err(XmtpError::unknown)?
            .build()
            .await
            .map_err(XmtpError::unknown)?;
        let key = NEXT_CLIENT_KEY
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |key| {
                key.checked_add(1)
            })
            .map_err(|_| XmtpError::unknown("client key space exhausted"))?;
        Ok(Self {
            inner: Arc::new(inner),
            key,
        })
    }

    async fn register_with_signer(
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
                SignerKind::Scw {
                    chain_id,
                    block_number,
                },
                Signature::Scw {
                    bytes,
                    address,
                    chain_id: signed_chain_id,
                    block_number: signed_block,
                },
            ) if chain_id == signed_chain_id && block_number == signed_block => {
                request
                    .add_new_unverified_smart_contract_signature(
                        NewUnverifiedSmartContractWalletSignature::new(
                            bytes,
                            AccountId::new_evm(chain_id, address),
                            block_number,
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
            .map_err(XmtpError::unknown)
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
        let client = Self::build_inner(identity, options, None).await?;
        client.register_with_signer(signer, kind).await?;
        Ok(client)
    }

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

    pub fn conversations(&self) -> Arc<Conversations> {
        Arc::new(Conversations {
            client: self.inner.clone(),
            client_key: self.key,
        })
    }

    pub async fn end(&self) -> Result<(), XmtpError> {
        self.inner.close().await.map_err(XmtpError::unknown)
    }
}

#[cfg(not(target_arch = "wasm32"))]
async fn open_store(
    options: &StorageOptions,
    inbox_id: &str,
) -> Result<xmtp_db::DefaultStore, XmtpError> {
    use xmtp_db::{EncryptedMessageStore, EncryptionKey, NativeDb};

    let path = match &options.location {
        StorageLocation::InMemory => None,
        StorageLocation::Path(path) => Some(path.clone()),
        StorageLocation::Directory(directory) => {
            std::fs::create_dir_all(directory).map_err(XmtpError::unknown)?;
            let label = options.label.as_deref().unwrap_or(inbox_id);
            Some(
                std::path::Path::new(directory)
                    .join(format!("{label}.db"))
                    .to_string_lossy()
                    .into_owned(),
            )
        }
        StorageLocation::Default => {
            let directory = std::env::temp_dir().join("xmtp-sdk");
            std::fs::create_dir_all(&directory).map_err(XmtpError::unknown)?;
            Some(
                directory
                    .join(format!("{inbox_id}.db"))
                    .to_string_lossy()
                    .into_owned(),
            )
        }
    };
    let builder = match path {
        Some(path) => NativeDb::builder().persistent(path),
        None => NativeDb::builder().ephemeral(),
    };
    let db = match &options.encryption_key {
        Some(bytes) => {
            let key = EncryptionKey::try_from(bytes.as_slice()).map_err(XmtpError::unknown)?;
            builder.key(key).build().map_err(XmtpError::unknown)?
        }
        None => builder.build_unencrypted().map_err(XmtpError::unknown)?,
    };
    EncryptedMessageStore::new(db).map_err(XmtpError::unknown)
}

#[cfg(target_arch = "wasm32")]
async fn open_store(
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
        StorageLocation::Default => StorageOption::Persistent(inbox_id.to_owned()),
        StorageLocation::Directory(path) | StorageLocation::Path(path) => {
            StorageOption::Persistent(path.clone())
        }
    };
    let db = WasmDb::new(&location).await.map_err(XmtpError::unknown)?;
    EncryptedMessageStore::new(db).map_err(XmtpError::unknown)
}
