//! Client creation, signature requests, and the client object.

use super::*;

use crate::fork_recovery::FfiForkRecoveryOpts;
use crate::identity::FfiIdentifier;
use crate::logger::init_logger;
use crate::message::FfiDecodedMessage;
use crate::server_configuration::FfiServerConfiguration;
use crate::worker::{FfiDeviceSyncMode, FfiSyncWorker};
use crate::worker_config::FfiWorkerConfig;
use crate::{FfiError, GenericError};

use std::{collections::HashMap, convert::TryInto, sync::Arc};
use tokio::sync::Mutex;
use xmtp_configuration::{MAX_DB_POOL_SIZE, MIN_DB_POOL_SIZE};

use xmtp_db::NativeDb;

use xmtp_db::{EncryptedMessageStore, EncryptionKey, consent_record::StoredConsentRecord};
use xmtp_id::associations::{Identifier, verify_signed_with_public_context};
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_id::{
    InboxId,
    associations::{
        AccountId,
        builder::SignatureRequest,
        unverified::{NewUnverifiedSmartContractWalletSignature, UnverifiedSignature},
    },
};
use xmtp_mls::context::XmtpSharedContext;
use xmtp_mls::identity::IdentityStrategy;
use xmtp_mls::identity_updates::apply_signature_request_with_verifier;
use xmtp_mls::identity_updates::revoke_installations_with_verifier;
use xmtp_proto::api::HasStats;

use xmtp_proto::api_client::AggregateStats;
use xmtp_proto::types::{ApiIdentifier, GroupMessageMetadata};

#[derive(uniffi::Record)]
pub struct FfiMessageMetadata {
    pub cursor: FfiCursor,
    pub created_ns: i64,
}

impl TryFrom<GroupMessageMetadata> for FfiMessageMetadata {
    type Error = GenericError;

    fn try_from(metadata: GroupMessageMetadata) -> Result<Self, Self::Error> {
        Ok(FfiMessageMetadata {
            cursor: metadata.cursor.into(),
            created_ns: metadata.created_ns.timestamp_nanos_opt().ok_or_else(|| {
                GenericError::Generic {
                    err: "Received a timestamp from the server more than 584 years from 1970"
                        .to_string(),
                }
            })?,
        })
    }
}

#[uniffi::export(async_runtime = "tokio")]
#[tracing::instrument(level = "debug", skip_all)]
pub async fn get_newest_message_metadata(
    api: Arc<XmtpApiClient>,
    group_ids: Vec<Vec<u8>>,
) -> Result<HashMap<Vec<u8>, FfiMessageMetadata>, FfiError> {
    let group_ids: Vec<xmtp_proto::types::GroupId> = group_ids
        .into_iter()
        .map(xmtp_proto::types::GroupId::try_from)
        .collect::<Result<_, _>>()
        .map_err(|e| FfiError::generic(e.to_string()))?;

    let metadata = api.wrapper.get_newest_message_metadata(&group_ids).await?;

    metadata
        .into_iter()
        .map(|(k, v)| Ok((k.to_vec(), FfiMessageMetadata::try_from(v)?)))
        .collect()
}

/**
 * Static revoke a list of installations
 */
#[uniffi::export]
#[tracing::instrument(level = "debug", skip_all)]
pub fn revoke_installations(
    api: Arc<XmtpApiClient>,
    recovery_identifier: FfiIdentifier,
    inbox_id: &InboxId,
    installation_ids: Vec<Vec<u8>>,
) -> Result<Arc<FfiSignatureRequest>, FfiError> {
    let scw_verifier = Arc::new(Box::new(api.inner()) as Box<dyn SmartContractSignatureVerifier>);
    let ident = recovery_identifier.try_into()?;

    let signature_request = revoke_installations_with_verifier(&ident, inbox_id, installation_ids)?;

    Ok(Arc::new(FfiSignatureRequest {
        inner: Arc::new(tokio::sync::Mutex::new(signature_request)),
        scw_verifier: scw_verifier.clone(),
    }))
}

/**
 * Static apply a signature request
 */
#[uniffi::export(async_runtime = "tokio")]
#[tracing::instrument(level = "debug", skip_all)]
pub async fn apply_signature_request(
    api: Arc<XmtpApiClient>,
    signature_request: Arc<FfiSignatureRequest>,
) -> Result<(), FfiError> {
    let signature_request = signature_request.inner.lock().await;
    let scw_verifier = Arc::new(Box::new(api.inner()) as Box<dyn SmartContractSignatureVerifier>);

    let store = EncryptedMessageStore::new(NativeDb::builder().ephemeral().build_unencrypted()?)?;
    apply_signature_request_with_verifier(
        &api.wrapper,
        &store.db(),
        signature_request.clone(),
        &scw_verifier,
    )
    .await?;

    Ok(())
}

#[derive(uniffi::Record, Clone)]
pub struct DbOptions {
    pub db: Option<String>,
    pub encryption_key: Option<Vec<u8>>,
    pub max_db_pool_size: Option<u32>,
    pub min_db_pool_size: Option<u32>,
    /// When true, use a single DB connection instead of a pool (one file
    /// descriptor). Pool-size options are ignored. Defaults to unset so existing
    /// foreign callers that construct `DbOptions` without this field still compile.
    #[uniffi(default = None)]
    pub use_single_connection: Option<bool>,
}

impl DbOptions {
    pub fn new(
        db: Option<String>,
        encryption_key: Option<Vec<u8>>,
        max_db_pool_size: Option<u32>,
        min_db_pool_size: Option<u32>,
        use_single_connection: Option<bool>,
    ) -> Self {
        Self {
            db,
            encryption_key,
            max_db_pool_size,
            min_db_pool_size,
            use_single_connection,
        }
    }
}

/// It returns a new client of the specified `inbox_id`.
/// Note that the `inbox_id` must be either brand new or already associated with the `account_identifier`.
/// i.e. `inbox_id` cannot be associated with another account address.
///
/// Prior to calling this function, it's suggested to form `inbox_id`, `account_identifier`, and `nonce` like below.
///
/// ```text
/// inbox_id = get_inbox_id_for_address(account_identifier)
/// nonce = 0
///
/// // if inbox_id is not associated, we will create new one.
/// if !inbox_id {
///     if !legacy_key { nonce = random_u64() }
///     inbox_id = generate_inbox_id(account_identifier, nonce)
/// } // Otherwise, we will just use the inbox and ignore the nonce.
/// db_path = $inbox_id-$env
///
/// xmtp.create_client(account_identifier, nonce, inbox_id, Option<legacy_signed_private_key_proto>)
/// ```
///
/// `change_callbacks` is unstable: notifications for group-state changes,
/// registered here because the changes they report arrive from the stream and
/// sync paths, where no SDK call is on the stack to carry them. `None` (the
/// SDK-side default) registers nothing. See
/// [`change_callbacks::FfiUnstableChangeCallbacks`].
#[allow(clippy::too_many_arguments)]
#[uniffi::export(
    async_runtime = "tokio",
    default(change_callbacks = None)
)]
#[tracing::instrument(level = "debug", skip_all)]
pub async fn create_client(
    api: Arc<XmtpApiClient>,
    db: DbOptions,
    inbox_id: &InboxId,
    account_identifier: FfiIdentifier,
    nonce: u64,
    legacy_signed_private_key_proto: Option<Vec<u8>>,
    device_sync_mode: Option<FfiDeviceSyncMode>,
    allow_offline: Option<bool>,
    fork_recovery_opts: Option<FfiForkRecoveryOpts>,
    worker_config: Option<FfiWorkerConfig>,
    change_callbacks: Option<change_callbacks::FfiUnstableChangeCallbacks>,
) -> Result<Arc<FfiXmtpClient>, FfiError> {
    let ident = account_identifier.clone();
    init_logger();
    // See `connect_to_backend` — ensure the rustls provider is installed before an HTTP
    // client is built. Idempotent. See issue #3846.
    xmtp_cryptography::install_crypto_provider();

    let DbOptions {
        db,
        encryption_key,
        max_db_pool_size,
        min_db_pool_size,
        use_single_connection,
    } = db;

    log::info!(
        "Creating message store with path: {:?} and encryption key: {} of length {:?}",
        db,
        encryption_key.is_some(),
        encryption_key.as_ref().map(|k| k.len())
    );

    let single = use_single_connection.unwrap_or(false);

    let base = if let Some(path) = db {
        NativeDb::builder().persistent(path)
    } else {
        NativeDb::builder().ephemeral()
    };

    let db = if single {
        if max_db_pool_size.is_some() || min_db_pool_size.is_some() {
            log::info!("use_single_connection is set; ignoring max/min db pool size options");
        }
        let b = base.single_connection();
        if let Some(key) = encryption_key {
            let key: EncryptionKey = key
                .try_into()
                .map_err(|_| "Malformed 32 byte encryption key".to_string())?;
            b.key(key).build()
        } else {
            b.build_unencrypted()
        }
    } else {
        let b = base
            .max_pool_size(max_db_pool_size.unwrap_or(MAX_DB_POOL_SIZE))
            .min_pool_size(min_db_pool_size.unwrap_or(MIN_DB_POOL_SIZE));
        if let Some(key) = encryption_key {
            let key: EncryptionKey = key
                .try_into()
                .map_err(|_| "Malformed 32 byte encryption key".to_string())?;
            b.key(key).build()
        } else {
            b.build_unencrypted()
        }
    }?;

    let store = EncryptedMessageStore::new(db)?;

    log::info!("Creating XMTP client");
    let used_legacy_key = legacy_signed_private_key_proto.is_some();
    let identity_strategy = IdentityStrategy::new(
        inbox_id.clone(),
        ident.clone().try_into()?,
        nonce,
        legacy_signed_private_key_proto,
    );

    let api_client = api.api_client.clone();

    let mut builder = xmtp_mls::Client::builder(identity_strategy)
        .api_client_with_streams(api_client)
        .with_remote_verifier()?
        .with_allow_offline(allow_offline)
        .store(store);

    if let Some(sync_worker_mode) = device_sync_mode {
        builder = builder.device_sync_worker_mode(sync_worker_mode.into());
    }

    if let Some(fork_recovery_opts) = fork_recovery_opts {
        builder = builder.fork_recovery_opts(fork_recovery_opts.into());
    }

    if let Some(worker_config) = worker_config {
        builder = builder.worker_config(worker_config.into());
    }

    if let Some(change_callbacks) = change_callbacks {
        builder = builder.unstable_change_callbacks(change_callbacks.into());
    }

    let xmtp_client = builder.default_mls_store()?.build().await?;
    if used_legacy_key {
        xmtp_client.ensure_registration_visible().await?;
    }

    log::info!(
        "Created XMTP client for inbox_id: {}",
        xmtp_client.inbox_id()
    );
    let worker = FfiSyncWorker {
        handle: xmtp_client.context.sync_metrics(),
    };
    Ok(Arc::new(FfiXmtpClient {
        inner_client: Arc::new(xmtp_client),
        worker,
        account_identifier,
    }))
}

#[allow(unused)]
#[uniffi::export(async_runtime = "tokio")]
#[tracing::instrument(level = "debug", skip_all)]
pub async fn get_inbox_id_for_identifier(
    api: Arc<XmtpApiClient>,
    account_identifier: FfiIdentifier,
) -> Result<Option<String>, FfiError> {
    init_logger();
    let account_identifier: Identifier = account_identifier.try_into()?;
    let api_identifier: ApiIdentifier = account_identifier.into();

    let results = api
        .wrapper
        .get_inbox_ids(vec![api_identifier.clone()])
        .await
        .map_err(GenericError::from_error)?;

    Ok(results.into_iter().next().flatten())
}

#[derive(uniffi::Object)]
pub struct FfiSignatureRequest {
    pub(crate) inner: Arc<Mutex<SignatureRequest>>,
    pub(crate) scw_verifier: Arc<Box<dyn SmartContractSignatureVerifier>>,
}

#[derive(uniffi::Record, Clone)]
pub struct FfiPasskeySignature {
    pub(crate) public_key: Vec<u8>,
    pub(crate) signature: Vec<u8>,
    pub(crate) authenticator_data: Vec<u8>,
    pub(crate) client_data_json: Vec<u8>,
}

#[uniffi::export(async_runtime = "tokio")]
impl FfiSignatureRequest {
    // Signature that's signed by EOA wallet
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn add_ecdsa_signature(&self, signature_bytes: Vec<u8>) -> Result<(), FfiError> {
        let mut inner = self.inner.lock().await;
        inner
            .add_signature(
                UnverifiedSignature::new_recoverable_ecdsa(signature_bytes),
                &self.scw_verifier,
            )
            .await?;

        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn add_passkey_signature(
        &self,
        signature: FfiPasskeySignature,
    ) -> Result<(), FfiError> {
        let mut inner = self.inner.lock().await;

        let new_signature = UnverifiedSignature::new_passkey(
            signature.public_key,
            signature.signature,
            signature.authenticator_data,
            signature.client_data_json,
        );

        inner
            .add_signature(new_signature, &self.scw_verifier)
            .await?;

        Ok(())
    }

    // Signature that's signed by smart contract wallet
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn add_scw_signature(
        &self,
        signature_bytes: Vec<u8>,
        address: String,
        chain_id: u64,
        block_number: Option<u64>,
    ) -> Result<(), FfiError> {
        let mut inner = self.inner.lock().await;
        let account_id = AccountId::new_evm(chain_id, address);

        let new_signature = NewUnverifiedSmartContractWalletSignature::new(
            signature_bytes,
            account_id,
            block_number,
        );

        inner
            .add_new_unverified_smart_contract_signature(new_signature, &self.scw_verifier)
            .await?;

        Ok(())
    }

    pub async fn is_ready(&self) -> bool {
        self.inner.lock().await.is_ready()
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn signature_text(&self) -> Result<String, FfiError> {
        Ok(self.inner.lock().await.signature_text())
    }

    /// missing signatures that are from `MemberKind::Address`
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn missing_address_signatures(&self) -> Result<Vec<String>, FfiError> {
        let inner = self.inner.lock().await;
        Ok(inner
            .missing_address_signatures()
            .iter()
            .map(|member| member.to_string())
            .collect())
    }
}

#[derive(Default, Clone, Copy, uniffi::Enum)]
pub enum FfiClientMode {
    #[default]
    Default,
    Notification,
}

#[derive(uniffi::Object)]
pub struct FfiXmtpClient {
    pub(crate) inner_client: Arc<RustXmtpClient>,
    #[allow(dead_code)]
    worker: FfiSyncWorker,
    #[allow(dead_code)]
    pub(crate) account_identifier: FfiIdentifier,
}

#[uniffi::export(async_runtime = "tokio")]
impl FfiXmtpClient {
    pub fn api_statistics(&self) -> FfiApiStats {
        self.inner_client
            .context
            .api()
            .api_client
            .mls_stats()
            .into()
    }

    pub fn api_identity_statistics(&self) -> FfiIdentityStats {
        self.inner_client
            .context
            .api()
            .api_client
            .identity_stats()
            .into()
    }

    pub fn api_aggregate_statistics(&self) -> String {
        let api = self.inner_client.context.api().api_client.mls_stats();
        let identity = self.inner_client.context.api().api_client.identity_stats();
        let aggregate = AggregateStats { mls: api, identity };
        format!("{:?}", aggregate)
    }

    pub fn clear_all_statistics(&self) {
        self.inner_client
            .context
            .api()
            .api_client
            .mls_stats()
            .clear();
        self.inner_client
            .context
            .api()
            .api_client
            .identity_stats()
            .clear();
    }

    #[tracing::instrument(skip_all)]
    pub fn inbox_id(&self) -> InboxId {
        self.inner_client.inbox_id().to_string()
    }

    #[tracing::instrument(skip_all)]
    pub fn conversations(&self) -> Arc<FfiConversations> {
        Arc::new(FfiConversations {
            inner_client: self.inner_client.clone(),
        })
    }

    #[tracing::instrument(skip_all)]
    pub fn conversation(&self, conversation_id: Vec<u8>) -> Result<FfiConversation, FfiError> {
        let conversation_id = xmtp_proto::types::GroupId::try_from(conversation_id)
            .map_err(|e| FfiError::generic(e.to_string()))?;
        self.inner_client
            .stitched_group(&conversation_id)
            .map(Into::into)
            .map_err(Into::into)
    }

    #[tracing::instrument(skip_all)]
    pub fn dm_conversation(&self, target_inbox_id: String) -> Result<FfiConversation, FfiError> {
        let convo = self
            .inner_client
            .dm_group_from_target_inbox(target_inbox_id)?;
        Ok(convo.into())
    }

    #[tracing::instrument(skip_all)]
    pub fn message(&self, message_id: Vec<u8>) -> Result<FfiMessage, FfiError> {
        let message = self.inner_client.message(message_id)?;
        Ok(message.into())
    }

    #[tracing::instrument(skip_all)]
    pub fn enriched_message(&self, message_id: Vec<u8>) -> Result<FfiDecodedMessage, FfiError> {
        let message = self.inner_client.message_v2(message_id)?;
        Ok(message.into())
    }

    #[tracing::instrument(skip_all)]
    pub fn delete_message(&self, message_id: Vec<u8>) -> Result<u32, FfiError> {
        let deleted_count = self.inner_client.delete_message(message_id)?;
        Ok(deleted_count as u32)
    }

    #[tracing::instrument(skip_all)]
    pub async fn can_message(
        &self,
        account_identifiers: Vec<FfiIdentifier>,
    ) -> Result<HashMap<FfiIdentifier, bool>, FfiError> {
        let inner = self.inner_client.as_ref();

        let account_identifiers: Result<Vec<Identifier>, _> = account_identifiers
            .into_iter()
            .map(|ident| ident.try_into())
            .collect();
        let account_identifiers = account_identifiers?;

        let results = inner
            .can_message(&account_identifiers)
            .await?
            .into_iter()
            .map(|(ident, can_msg)| (ident.into(), can_msg))
            .collect();

        Ok(results)
    }

    #[tracing::instrument(skip_all)]
    pub fn installation_id(&self) -> Vec<u8> {
        self.inner_client.installation_public_key().to_vec()
    }

    #[tracing::instrument(skip_all)]
    pub fn release_db_connection(&self) -> Result<(), FfiError> {
        Ok(self.inner_client.release_db_connection()?)
    }

    #[tracing::instrument(skip_all)]
    pub async fn db_reconnect(&self) -> Result<(), FfiError> {
        Ok(self.inner_client.reconnect_db()?)
    }

    /// Cleanly shut down this client: cancel in-flight workers and detached
    /// streams, then release the DB connection. Idempotent — a second call
    /// resolves to `Ok`.
    ///
    /// `await` this before deleting the SQLite file or dropping the client
    /// reference to avoid late log spew from detached workers/streams firing
    /// against a dead DB.
    ///
    /// Named `shutdown` rather than `close` because uniffi reserves `close`
    /// on every exported object for the Kotlin `Disposable` handle-disposal
    /// method, which would conflict with this one.
    #[tracing::instrument(skip_all)]
    pub async fn shutdown(&self) -> Result<(), FfiError> {
        Ok(self.inner_client.close().await?)
    }

    /// Process fixed starting targets and their enrolled Welcome discoveries.
    /// The timeout bounds the whole call; `None` uses the client's barrier timeout.
    /// Failure preserves committed progress and reports partial counts and unfinished targets.
    /// Existing streams remain active, and application delivery progress is unchanged.
    #[tracing::instrument(skip_all)]
    pub async fn catch_up_to_live(
        &self,
        opts: Option<FfiCatchUpOptions>,
    ) -> Result<FfiCatchUpSummary, FfiError> {
        let timeout = opts
            .and_then(|o| o.timeout_ms)
            .map(std::time::Duration::from_millis);
        Ok(self.inner_client.catch_up_to_live(timeout).await?.into())
    }

    #[tracing::instrument(skip_all)]
    pub async fn find_inbox_id(
        &self,
        identifier: FfiIdentifier,
    ) -> Result<Option<String>, FfiError> {
        let inner = self.inner_client.as_ref();
        let conn = self.inner_client.context.db();
        let result = inner
            .find_inbox_id_from_identifier(&conn, identifier.try_into()?)
            .await?;
        Ok(result)
    }

    /**
     * Get the client's inbox state.
     *
     * If `refresh_from_network` is true, the client will go to the network first to refresh the state.
     * Otherwise, the state will be read from the local database.
     */
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn inbox_state(&self, refresh_from_network: bool) -> Result<FfiInboxState, FfiError> {
        let state = self.inner_client.inbox_state(refresh_from_network).await?;
        let inbox_id = state.inbox_id();

        // Get the creation signature kind
        let creation_signature_kind = self
            .inner_client
            .inbox_creation_signature_kind(inbox_id, refresh_from_network)
            .await?
            .map(Into::into);

        let mut ffi_state: FfiInboxState = state.into();
        ffi_state.creation_signature_kind = creation_signature_kind;
        Ok(ffi_state)
    }

    // Returns a HashMap of installation_id to FfiKeyPackageStatus
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn get_key_package_statuses_for_installation_ids(
        &self,
        installation_ids: Vec<Vec<u8>>,
    ) -> Result<HashMap<Vec<u8>, FfiKeyPackageStatus>, FfiError> {
        let key_packages = self
            .inner_client
            .get_key_packages_for_installation_ids(installation_ids)
            .await?;

        let key_packages: HashMap<Vec<u8>, FfiKeyPackageStatus> = key_packages
            .into_iter()
            .map(
                |(installation_id, key_package_result)| match key_package_result {
                    Ok(key_package) => (installation_id, key_package.into()),
                    Err(e) => (
                        installation_id,
                        FfiKeyPackageStatus {
                            lifetime: None,
                            validation_error: Some(e.to_string()),
                        },
                    ),
                },
            )
            .collect();

        Ok(key_packages)
    }

    /**
     * Get the inbox state for each `inbox_id`.
     *
     * If `refresh_from_network` is true, the client will go to the network first to refresh the state.
     * Otherwise, the state will be read from the local database.
     */
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn addresses_from_inbox_id(
        &self,
        refresh_from_network: bool,
        inbox_ids: Vec<String>,
    ) -> Result<Vec<FfiInboxState>, FfiError> {
        let state = self
            .inner_client
            .inbox_addresses(
                refresh_from_network,
                inbox_ids.iter().map(String::as_str).collect(),
            )
            .await?;
        Ok(state.into_iter().map(Into::into).collect())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn get_latest_inbox_state(
        &self,
        inbox_id: String,
    ) -> Result<FfiInboxState, FfiError> {
        let state = self
            .inner_client
            .identity_updates()
            .get_latest_association_state(&self.inner_client.context.db(), &inbox_id)
            .await?;

        // Get the creation signature kind (read from local DB, no network refresh)
        let creation_signature_kind = self
            .inner_client
            .inbox_creation_signature_kind(state.inbox_id(), false)
            .await?
            .map(Into::into);

        let mut ffi_state: FfiInboxState = state.into();
        ffi_state.creation_signature_kind = creation_signature_kind;
        Ok(ffi_state)
    }

    #[tracing::instrument(level = "debug", skip_all, fields(refresh_from_network))]
    pub async fn fetch_inbox_updates_count(
        &self,
        refresh_from_network: bool,
        inbox_ids: Vec<String>,
    ) -> Result<HashMap<InboxId, u32>, FfiError> {
        let ids = inbox_ids.iter().map(AsRef::as_ref).collect();
        self.inner_client
            .fetch_inbox_updates_count(refresh_from_network, ids)
            .await
            .map_err(Into::into)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn fetch_own_inbox_updates_count(
        &self,
        refresh_from_network: bool,
    ) -> Result<u32, FfiError> {
        self.inner_client
            .fetch_own_inbox_updates_count(refresh_from_network)
            .await
            .map_err(Into::into)
    }

    #[tracing::instrument(skip_all)]
    pub async fn set_consent_states(&self, records: Vec<FfiConsent>) -> Result<(), FfiError> {
        let inner = self.inner_client.as_ref();
        let stored_records: Vec<StoredConsentRecord> =
            records.into_iter().map(StoredConsentRecord::from).collect();

        inner.set_consent_states(&stored_records).await?;
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    pub async fn get_consent_state(
        &self,
        entity_type: FfiConsentEntityType,
        entity: String,
    ) -> Result<FfiConsentState, FfiError> {
        let inner = self.inner_client.as_ref();
        let result = inner.get_consent_state(entity_type.into(), entity).await?;

        Ok(result.into())
    }

    /// A utility function to sign a piece of text with this installation's private key.
    #[tracing::instrument(skip_all)]
    pub fn sign_with_installation_key(&self, text: &str) -> Result<Vec<u8>, FfiError> {
        let inner = self.inner_client.as_ref();
        Ok(inner.context.sign_with_public_context(text)?)
    }

    /// A utility function to easily verify that a piece of text was signed by this installation.
    #[tracing::instrument(skip_all)]
    pub fn verify_signed_with_installation_key(
        &self,
        signature_text: &str,
        signature_bytes: Vec<u8>,
    ) -> Result<(), FfiError> {
        let inner = self.inner_client.as_ref();
        let public_key = inner.installation_public_key().to_vec();

        self.verify_signed_with_public_key(signature_text, signature_bytes, public_key)
    }

    /// A utility function to easily verify that a string has been signed by another libXmtp installation.
    /// Only works for verifying libXmtp public context signatures.
    #[tracing::instrument(level = "debug", skip_all)]
    pub fn verify_signed_with_public_key(
        &self,
        signature_text: &str,
        signature_bytes: Vec<u8>,
        public_key: Vec<u8>,
    ) -> Result<(), FfiError> {
        let signature_bytes: [u8; 64] =
            signature_bytes
                .try_into()
                .map_err(|v: Vec<u8>| GenericError::Generic {
                    err: format!(
                        "signature_bytes is not 64 bytes long. (Actual size: {})",
                        v.len()
                    ),
                })?;

        let public_key: [u8; 32] =
            public_key
                .try_into()
                .map_err(|v: Vec<u8>| GenericError::Generic {
                    err: format!(
                        "public_key is not 32 bytes long. (Actual size: {})",
                        v.len()
                    ),
                })?;

        Ok(verify_signed_with_public_context(
            signature_text,
            &signature_bytes,
            &public_key,
        )?)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn sync_preferences(&self) -> Result<FfiGroupSyncSummary, FfiError> {
        self.sync_all_device_sync_groups().await
    }

    /// What this deployment published about itself, as resolved at build.
    ///
    /// The snapshot is fixed for the life of the client. A refresh rewrites the
    /// stored copy and never changes this value.
    #[tracing::instrument(level = "debug", skip_all)]
    pub fn server_configuration(&self) -> FfiServerConfiguration {
        self.inner_client.server_configuration().into()
    }

    /// Fetch the deployment configuration now, rewrite the stored copy, and
    /// return what the backend answered.
    ///
    /// Applies the same validation, storage, and identifier binding the refresh
    /// worker applies. The snapshot [`FfiXmtpClient::server_configuration`]
    /// returns is unchanged: a new value takes effect at the next build.
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn refresh_server_configuration(&self) -> Result<FfiServerConfiguration, FfiError> {
        let configuration = self.inner_client.refresh_server_configuration().await?;
        Ok((&configuration).into())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub fn signature_request(&self) -> Option<Arc<FfiSignatureRequest>> {
        let scw_verifier = self.inner_client.scw_verifier().clone();
        self.inner_client
            .identity()
            .signature_request()
            .map(move |request| {
                Arc::new(FfiSignatureRequest {
                    inner: Arc::new(Mutex::new(request)),
                    scw_verifier,
                })
            })
    }

    /// Register the identity and wait until it is visible on the network.
    ///
    /// The `visibility_confirmation_options` parameter is deprecated. Registration
    /// always waits, so this parameter has no effect.
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn register_identity(
        &self,
        signature_request: Arc<FfiSignatureRequest>,
        visibility_confirmation_options: Option<FfiVisibilityConfirmationOptions>,
    ) -> Result<(), FfiError> {
        {
            let signature_request = signature_request.inner.lock().await;
            self.inner_client
                .register_identity(signature_request.clone())
                .await?;
        }

        let _ = visibility_confirmation_options;

        Ok(())
    }

    /// Adds a wallet address to the existing client
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn add_identity(
        &self,
        new_identity: FfiIdentifier,
    ) -> Result<Arc<FfiSignatureRequest>, FfiError> {
        let signature_request = self
            .inner_client
            .identity_updates()
            .associate_identity(new_identity.try_into()?)
            .await?;
        let scw_verifier = self.inner_client.scw_verifier();
        let request = Arc::new(FfiSignatureRequest {
            inner: Arc::new(tokio::sync::Mutex::new(signature_request)),
            scw_verifier: scw_verifier.clone(),
        });

        Ok(request)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn apply_signature_request(
        &self,
        signature_request: Arc<FfiSignatureRequest>,
    ) -> Result<(), FfiError> {
        let signature_request = signature_request.inner.lock().await;
        self.inner_client
            .identity_updates()
            .apply_signature_request(signature_request.clone())
            .await?;

        Ok(())
    }

    /// Revokes or removes an identity from the existing client
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn revoke_identity(
        &self,
        identifier: FfiIdentifier,
    ) -> Result<Arc<FfiSignatureRequest>, FfiError> {
        let Self { inner_client, .. } = self;

        let signature_request = inner_client
            .identity_updates()
            .revoke_identities(vec![identifier.try_into()?])
            .await?;
        let scw_verifier = inner_client.scw_verifier();
        let request = Arc::new(FfiSignatureRequest {
            inner: Arc::new(tokio::sync::Mutex::new(signature_request)),
            scw_verifier: scw_verifier.clone(),
        });

        Ok(request)
    }

    /**
     * Revokes all installations except the one the client is currently using
     * Returns Some FfiSignatureRequest if we have installations to revoke.
     * If we have no other installations to revoke, returns None.
     */
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn revoke_all_other_installations_signature_request(
        &self,
    ) -> Result<Option<Arc<FfiSignatureRequest>>, FfiError> {
        let installation_id = self.inner_client.installation_public_key();
        let inbox_state = self.inner_client.inbox_state(true).await?;
        let other_installation_ids: Vec<Vec<u8>> = inbox_state
            .installation_ids()
            .into_iter()
            .filter(|id| id != installation_id)
            .collect();

        if other_installation_ids.is_empty() {
            return Ok(None);
        }

        let signature_request = self
            .inner_client
            .identity_updates()
            .revoke_installations(other_installation_ids)
            .await?;

        Ok(Some(Arc::new(FfiSignatureRequest {
            inner: Arc::new(tokio::sync::Mutex::new(signature_request)),
            scw_verifier: self.inner_client.scw_verifier().clone(),
        })))
    }

    /**
     * Revoke a list of installations
     */
    #[tracing::instrument(skip_all)]
    pub async fn revoke_installations(
        &self,
        installation_ids: Vec<Vec<u8>>,
    ) -> Result<Arc<FfiSignatureRequest>, FfiError> {
        let signature_request = self
            .inner_client
            .identity_updates()
            .revoke_installations(installation_ids)
            .await?;

        Ok(Arc::new(FfiSignatureRequest {
            inner: Arc::new(tokio::sync::Mutex::new(signature_request)),
            scw_verifier: self.inner_client.scw_verifier().clone(),
        }))
    }

    /**
     * Change the recovery identifier for your inboxId
     */
    #[tracing::instrument(skip_all)]
    pub async fn change_recovery_identifier(
        &self,
        new_recovery_identifier: FfiIdentifier,
    ) -> Result<Arc<FfiSignatureRequest>, FfiError> {
        let signature_request = self
            .inner_client
            .identity_updates()
            .change_recovery_identifier(new_recovery_identifier.try_into()?)
            .await?;

        Ok(Arc::new(FfiSignatureRequest {
            inner: Arc::new(tokio::sync::Mutex::new(signature_request)),
            scw_verifier: self.inner_client.scw_verifier().clone(),
        }))
    }

    /// Wait until this client's registration is visible on the network.
    ///
    /// Pass `None` to use the default timeout.
    #[xmtp_common::err_span]
    pub async fn wait_for_registration_visible(
        &self,
        options: Option<FfiVisibilityConfirmationOptions>,
    ) -> Result<(), FfiError> {
        self.inner_client
            .wait_for_registration_visible(options.unwrap_or_default().into())
            .await?;

        Ok(())
    }
}
