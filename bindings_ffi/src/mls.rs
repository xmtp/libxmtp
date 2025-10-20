use crate::identity::{FfiCollectionExt, FfiCollectionTryExt, FfiIdentifier};
pub use crate::inbox_owner::SigningError;
use crate::logger::init_logger;
use crate::message::{FfiDecodedMessage, FfiDeliveryStatus, FfiReactionPayload};
use crate::worker::FfiSyncWorker;
use crate::worker::FfiSyncWorkerMode;
use crate::{FfiReply, FfiSubscribeError, GenericError};
use futures::future::try_join_all;
use prost::Message;
use std::{collections::HashMap, convert::TryInto, sync::Arc};
use tokio::sync::Mutex;
use xmtp_api::{ApiClientWrapper, strategies};
use xmtp_api_d14n::MessageBackendBuilder;
use xmtp_common::time::now_ns;
use xmtp_common::{AbortHandle, GenericStreamHandle, StreamHandle};
use xmtp_content_types::attachment::Attachment;
use xmtp_content_types::attachment::AttachmentCodec;
use xmtp_content_types::multi_remote_attachment::MultiRemoteAttachmentCodec;
use xmtp_content_types::reaction::ReactionCodec;
use xmtp_content_types::read_receipt::ReadReceipt;
use xmtp_content_types::read_receipt::ReadReceiptCodec;
use xmtp_content_types::remote_attachment::RemoteAttachment;
use xmtp_content_types::remote_attachment::RemoteAttachmentCodec;
use xmtp_content_types::reply::Reply;
use xmtp_content_types::reply::ReplyCodec;
use xmtp_content_types::text::TextCodec;
use xmtp_content_types::transaction_reference::TransactionReference;
use xmtp_content_types::transaction_reference::TransactionReferenceCodec;
use xmtp_content_types::{ContentCodec, encoded_content_to_bytes};
use xmtp_db::NativeDb;
use xmtp_db::group::DmIdExt;
use xmtp_db::group::{ConversationType, GroupQueryOrderBy};
use xmtp_db::group_message::{ContentType, MsgQueryArgs};
use xmtp_db::group_message::{SortDirection, StoredGroupMessageWithReactions};
use xmtp_db::user_preferences::HmacKey;
use xmtp_db::{
    EncryptedMessageStore, EncryptionKey, StorageOption,
    consent_record::{ConsentState, ConsentType, StoredConsentRecord},
    group::GroupQueryArgs,
    group_message::{GroupMessageKind, StoredGroupMessage},
};
use xmtp_id::associations::{
    DeserializationError, Identifier, ident, verify_signed_with_public_context,
};
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_id::{
    InboxId,
    associations::{
        AccountId, AssociationState, MemberIdentifier,
        builder::SignatureRequest,
        unverified::{NewUnverifiedSmartContractWalletSignature, UnverifiedSignature},
    },
};
use xmtp_mls::client::inbox_addresses_with_verifier;
use xmtp_mls::common::group::DMMetadataOptions;
use xmtp_mls::common::group::GroupMetadataOptions;
use xmtp_mls::common::group_metadata::GroupMetadata;
use xmtp_mls::common::group_mutable_metadata::MessageDisappearingSettings;
use xmtp_mls::common::group_mutable_metadata::MetadataField;
use xmtp_mls::groups::ConversationDebugInfo;
use xmtp_mls::groups::device_sync::DeviceSyncError;
use xmtp_mls::groups::device_sync::archive::ArchiveImporter;
use xmtp_mls::groups::device_sync::archive::BackupMetadata;
use xmtp_mls::groups::device_sync::archive::exporter::ArchiveExporter;
use xmtp_mls::groups::device_sync::archive::insert_importer;
use xmtp_mls::groups::device_sync_legacy::ENC_KEY_SIZE;
use xmtp_mls::identity_updates::revoke_installations_with_verifier;
use xmtp_mls::identity_updates::{
    apply_signature_request_with_verifier, get_creation_signature_kind,
};
use xmtp_mls::utils::events::upload_debug_archive;
use xmtp_mls::verified_key_package_v2::{VerifiedKeyPackageV2, VerifiedLifetime};
use xmtp_mls::{
    client::Client as MlsClient,
    groups::{
        MlsGroup, PreconfiguredPolicies, UpdateAdminListType,
        device_sync::preference_sync::PreferenceUpdate,
        group_permissions::{
            BasePolicies, GroupMutablePermissions, GroupMutablePermissionsError,
            MembershipPolicies, MetadataBasePolicies, MetadataPolicies, PermissionsBasePolicies,
            PermissionsPolicies, PolicySet,
        },
        intents::{PermissionPolicyOption, PermissionUpdateType, UpdateGroupMembershipResult},
        members::PermissionLevel,
    },
    identity::IdentityStrategy,
    subscriptions::SubscribeError,
};
use xmtp_proto::api_client::AggregateStats;
use xmtp_proto::api_client::ApiStats;
use xmtp_proto::api_client::IdentityStats;
use xmtp_proto::types::ApiIdentifier;
use xmtp_proto::types::Cursor;
use xmtp_proto::xmtp::device_sync::{BackupElementSelection, BackupOptions};
use xmtp_proto::xmtp::mls::message_contents::EncodedContent;
use xmtp_proto::xmtp::mls::message_contents::content_types::{MultiRemoteAttachment, ReactionV2};

// Re-export types from message module that are used in public APIs
pub use crate::message::{
    FfiAttachment, FfiMultiRemoteAttachment, FfiReadReceipt, FfiRemoteAttachment,
    FfiTransactionReference,
};

#[cfg(any(test, feature = "bench"))]
pub mod test_utils;

#[cfg(any(test, feature = "bench"))]
pub mod inbox_owner;

pub type RustXmtpClient = MlsClient<xmtp_mls::MlsContext>;
pub type RustMlsGroup = MlsGroup<xmtp_mls::MlsContext>;

/// the opaque Xmtp Api Client for iOS/Android bindings
#[derive(uniffi::Object, Clone)]
pub struct XmtpApiClient(xmtp_mls::XmtpApiClient);

/// connect to the XMTP backend
/// specifying `gateway_host` enables the D14n backend
/// and assumes `host` is set to the correct
/// d14n backend url.
#[uniffi::export(async_runtime = "tokio")]
pub async fn connect_to_backend(
    v3_host: String,
    gateway_host: Option<String>,
    is_secure: bool,
    app_version: Option<String>,
) -> Result<Arc<XmtpApiClient>, GenericError> {
    init_logger();

    log::info!(
        v3_host,
        is_secure,
        "Creating API client for host: {}, gateway: {:?}, isSecure: {}",
        v3_host,
        gateway_host,
        is_secure
    );
    let mut backend = MessageBackendBuilder::default();
    let backend = backend
        .v3_host(&v3_host)
        .maybe_gateway_host(gateway_host)
        .app_version(app_version.clone().unwrap_or_default())
        .is_secure(is_secure)
        .build()?;
    Ok(Arc::new(XmtpApiClient(backend)))
}

#[uniffi::export(async_runtime = "tokio")]
pub async fn is_connected(api: Arc<XmtpApiClient>) -> bool {
    api.0.is_connected().await
}

/**
 * Static Get the inbox state for each `inbox_id`.
 */
#[uniffi::export(async_runtime = "tokio")]
pub async fn inbox_state_from_inbox_ids(
    api: Arc<XmtpApiClient>,
    inbox_ids: Vec<String>,
) -> Result<Vec<FfiInboxState>, GenericError> {
    let api: ApiClientWrapper<xmtp_mls::XmtpApiClient> =
        ApiClientWrapper::new(api.0.clone(), strategies::exponential_cooldown());
    let scw_verifier = Arc::new(Box::new(api.clone()) as Box<dyn SmartContractSignatureVerifier>);

    let db = NativeDb::new_unencrypted(&StorageOption::Ephemeral)?;
    let store = EncryptedMessageStore::new(db)?;

    let states = inbox_addresses_with_verifier(
        &api.clone(),
        &store.db(),
        inbox_ids.iter().map(String::as_str).collect(),
        &scw_verifier,
    )
    .await?;

    let mapped_futures = states.into_iter().map(|state| async {
        // TODO: Implement this field as part of the core association state.
        // https://github.com/xmtp/libxmtp/issues/2583
        let signature_kind =
            get_creation_signature_kind(&store.db(), scw_verifier.clone(), state.inbox_id())
                .await?;

        let mut ffi_state: FfiInboxState = state.into();
        ffi_state.creation_signature_kind = signature_kind.map(Into::into);

        Ok::<FfiInboxState, GenericError>(ffi_state)
    });

    try_join_all(mapped_futures).await
}

/**
 * Static revoke a list of installations
 */
#[uniffi::export]
pub fn revoke_installations(
    api: Arc<XmtpApiClient>,
    recovery_identifier: FfiIdentifier,
    inbox_id: &InboxId,
    installation_ids: Vec<Vec<u8>>,
) -> Result<Arc<FfiSignatureRequest>, GenericError> {
    let api: ApiClientWrapper<xmtp_mls::XmtpApiClient> =
        ApiClientWrapper::new(api.0.clone(), strategies::exponential_cooldown());
    let scw_verifier = Arc::new(Box::new(api) as Box<dyn SmartContractSignatureVerifier>);
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
pub async fn apply_signature_request(
    api: Arc<XmtpApiClient>,
    signature_request: Arc<FfiSignatureRequest>,
) -> Result<(), GenericError> {
    let api = ApiClientWrapper::new(Arc::new(api.0.clone()), strategies::exponential_cooldown());
    let signature_request = signature_request.inner.lock().await;
    let scw_verifier = Arc::new(Box::new(api.clone()) as Box<dyn SmartContractSignatureVerifier>);

    apply_signature_request_with_verifier(&api.clone(), signature_request.clone(), &scw_verifier)
        .await?;

    Ok(())
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
#[allow(clippy::too_many_arguments)]
#[uniffi::export(async_runtime = "tokio")]
pub async fn create_client(
    api: Arc<XmtpApiClient>,
    sync_api: Arc<XmtpApiClient>,
    db: Option<String>,
    encryption_key: Option<Vec<u8>>,
    inbox_id: &InboxId,
    account_identifier: FfiIdentifier,
    nonce: u64,
    legacy_signed_private_key_proto: Option<Vec<u8>>,
    device_sync_server_url: Option<String>,
    device_sync_mode: Option<FfiSyncWorkerMode>,
    allow_offline: Option<bool>,
    disable_events: Option<bool>,
) -> Result<Arc<FfiXmtpClient>, GenericError> {
    let ident = account_identifier.clone();
    init_logger();

    log::info!(
        "Creating message store with path: {:?} and encryption key: {} of length {:?}",
        db,
        encryption_key.is_some(),
        encryption_key.as_ref().map(|k| k.len())
    );

    let storage_option = match db {
        Some(path) => StorageOption::Persistent(path),
        None => StorageOption::Ephemeral,
    };

    let store = match encryption_key {
        Some(key) => {
            let key: EncryptionKey = key
                .try_into()
                .map_err(|_| "Malformed 32 byte encryption key".to_string())?;
            let db = NativeDb::new(&storage_option, key)?;
            EncryptedMessageStore::new(db)?
        }
        None => {
            let db = NativeDb::new_unencrypted(&storage_option)?;
            EncryptedMessageStore::new(db)?
        }
    };
    log::info!("Creating XMTP client");
    let identity_strategy = IdentityStrategy::new(
        inbox_id.clone(),
        ident.clone().try_into()?,
        nonce,
        legacy_signed_private_key_proto,
    );

    let mut builder = xmtp_mls::Client::builder(identity_strategy)
        .api_clients(
            Arc::unwrap_or_clone(api).0,
            Arc::unwrap_or_clone(sync_api).0,
        )
        .enable_api_stats()?
        .enable_api_debug_wrapper()?
        .with_remote_verifier()?
        .with_allow_offline(allow_offline)
        .with_disable_events(disable_events)
        .store(store);

    if let Some(sync_worker_mode) = device_sync_mode {
        builder = builder.device_sync_worker_mode(sync_worker_mode.into());
    }

    if let Some(url) = &device_sync_server_url {
        builder = builder.device_sync_server_url(url);
    }

    let xmtp_client = builder.default_mls_store()?.build().await?;

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
pub async fn get_inbox_id_for_identifier(
    api: Arc<XmtpApiClient>,
    account_identifier: FfiIdentifier,
) -> Result<Option<String>, GenericError> {
    init_logger();
    let mut api =
        ApiClientWrapper::new(Arc::new(api.0.clone()), strategies::exponential_cooldown());
    let account_identifier: Identifier = account_identifier.try_into()?;
    let api_identifier: ApiIdentifier = account_identifier.into();

    let results = api
        .get_inbox_ids(vec![api_identifier.clone()])
        .await
        .map_err(GenericError::from_error)?;

    Ok(results.get(&api_identifier).cloned())
}

#[derive(uniffi::Object)]
pub struct FfiSignatureRequest {
    inner: Arc<Mutex<SignatureRequest>>,
    scw_verifier: Arc<Box<dyn SmartContractSignatureVerifier>>,
}

#[derive(uniffi::Record, Clone)]
pub struct FfiPasskeySignature {
    public_key: Vec<u8>,
    signature: Vec<u8>,
    authenticator_data: Vec<u8>,
    client_data_json: Vec<u8>,
}

#[uniffi::export(async_runtime = "tokio")]
impl FfiSignatureRequest {
    // Signature that's signed by EOA wallet
    pub async fn add_ecdsa_signature(&self, signature_bytes: Vec<u8>) -> Result<(), GenericError> {
        let mut inner = self.inner.lock().await;
        inner
            .add_signature(
                UnverifiedSignature::new_recoverable_ecdsa(signature_bytes),
                &self.scw_verifier,
            )
            .await?;

        Ok(())
    }

    pub async fn add_passkey_signature(
        &self,
        signature: FfiPasskeySignature,
    ) -> Result<(), GenericError> {
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
    pub async fn add_scw_signature(
        &self,
        signature_bytes: Vec<u8>,
        address: String,
        chain_id: u64,
        block_number: Option<u64>,
    ) -> Result<(), GenericError> {
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

    pub async fn signature_text(&self) -> Result<String, GenericError> {
        Ok(self.inner.lock().await.signature_text())
    }

    /// missing signatures that are from `MemberKind::Address`
    pub async fn missing_address_signatures(&self) -> Result<Vec<String>, GenericError> {
        let inner = self.inner.lock().await;
        Ok(inner
            .missing_address_signatures()
            .iter()
            .map(|member| member.to_string())
            .collect())
    }
}

#[derive(uniffi::Object)]
pub struct FfiXmtpClient {
    inner_client: Arc<RustXmtpClient>,
    #[allow(dead_code)]
    worker: FfiSyncWorker,
    #[allow(dead_code)]
    account_identifier: FfiIdentifier,
}

#[uniffi::export(async_runtime = "tokio")]
impl FfiXmtpClient {
    pub fn api_statistics(&self) -> FfiApiStats {
        self.inner_client.api_stats().into()
    }

    pub fn api_identity_statistics(&self) -> FfiIdentityStats {
        self.inner_client.identity_api_stats().into()
    }

    pub fn api_aggregate_statistics(&self) -> String {
        let api = self.inner_client.api_stats();
        let identity = self.inner_client.identity_api_stats();
        let aggregate = AggregateStats { mls: api, identity };
        format!("{:?}", aggregate)
    }

    pub fn clear_all_statistics(&self) {
        self.inner_client.clear_stats()
    }

    pub fn inbox_id(&self) -> InboxId {
        self.inner_client.inbox_id().to_string()
    }

    pub fn conversations(&self) -> Arc<FfiConversations> {
        Arc::new(FfiConversations {
            inner_client: self.inner_client.clone(),
        })
    }

    pub fn conversation(&self, conversation_id: Vec<u8>) -> Result<FfiConversation, GenericError> {
        self.inner_client
            .stitched_group(&conversation_id)
            .map(Into::into)
            .map_err(Into::into)
    }

    pub fn dm_conversation(
        &self,
        target_inbox_id: String,
    ) -> Result<FfiConversation, GenericError> {
        let convo = self
            .inner_client
            .dm_group_from_target_inbox(target_inbox_id)?;
        Ok(convo.into())
    }

    pub fn message(&self, message_id: Vec<u8>) -> Result<FfiMessage, GenericError> {
        let message = self.inner_client.message(message_id)?;
        Ok(message.into())
    }

    pub fn enriched_message(&self, message_id: Vec<u8>) -> Result<FfiDecodedMessage, GenericError> {
        let message = self.inner_client.message_v2(message_id)?;
        Ok(message.into())
    }

    pub fn delete_message(&self, message_id: Vec<u8>) -> Result<u32, GenericError> {
        let deleted_count = self.inner_client.delete_message(message_id)?;
        Ok(deleted_count as u32)
    }

    pub async fn can_message(
        &self,
        account_identifiers: Vec<FfiIdentifier>,
    ) -> Result<HashMap<FfiIdentifier, bool>, GenericError> {
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

    pub fn installation_id(&self) -> Vec<u8> {
        self.inner_client.installation_public_key().to_vec()
    }

    pub fn release_db_connection(&self) -> Result<(), GenericError> {
        Ok(self.inner_client.release_db_connection()?)
    }

    pub async fn db_reconnect(&self) -> Result<(), GenericError> {
        Ok(self.inner_client.reconnect_db()?)
    }

    pub async fn find_inbox_id(
        &self,
        identifier: FfiIdentifier,
    ) -> Result<Option<String>, GenericError> {
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
    pub async fn inbox_state(
        &self,
        refresh_from_network: bool,
    ) -> Result<FfiInboxState, GenericError> {
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
    pub async fn get_key_package_statuses_for_installation_ids(
        &self,
        installation_ids: Vec<Vec<u8>>,
    ) -> Result<HashMap<Vec<u8>, FfiKeyPackageStatus>, GenericError> {
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
    pub async fn addresses_from_inbox_id(
        &self,
        refresh_from_network: bool,
        inbox_ids: Vec<String>,
    ) -> Result<Vec<FfiInboxState>, GenericError> {
        let state = self
            .inner_client
            .inbox_addresses(
                refresh_from_network,
                inbox_ids.iter().map(String::as_str).collect(),
            )
            .await?;
        Ok(state.into_iter().map(Into::into).collect())
    }

    pub async fn get_latest_inbox_state(
        &self,
        inbox_id: String,
    ) -> Result<FfiInboxState, GenericError> {
        let state = self
            .inner_client
            .identity_updates()
            .get_latest_association_state(&self.inner_client.context.db(), &inbox_id)
            .await?;
        Ok(state.into())
    }

    pub async fn set_consent_states(&self, records: Vec<FfiConsent>) -> Result<(), GenericError> {
        let inner = self.inner_client.as_ref();
        let stored_records: Vec<StoredConsentRecord> =
            records.into_iter().map(StoredConsentRecord::from).collect();

        inner.set_consent_states(&stored_records).await?;
        Ok(())
    }

    pub async fn get_consent_state(
        &self,
        entity_type: FfiConsentEntityType,
        entity: String,
    ) -> Result<FfiConsentState, GenericError> {
        let inner = self.inner_client.as_ref();
        let result = inner.get_consent_state(entity_type.into(), entity).await?;

        Ok(result.into())
    }

    /// A utility function to sign a piece of text with this installation's private key.
    pub fn sign_with_installation_key(&self, text: &str) -> Result<Vec<u8>, GenericError> {
        let inner = self.inner_client.as_ref();
        Ok(inner.context.sign_with_public_context(text)?)
    }

    /// A utility function to easily verify that a piece of text was signed by this installation.
    pub fn verify_signed_with_installation_key(
        &self,
        signature_text: &str,
        signature_bytes: Vec<u8>,
    ) -> Result<(), GenericError> {
        let inner = self.inner_client.as_ref();
        let public_key = inner.installation_public_key().to_vec();

        self.verify_signed_with_public_key(signature_text, signature_bytes, public_key)
    }

    /// A utility function to easily verify that a string has been signed by another libXmtp installation.
    /// Only works for verifying libXmtp public context signatures.
    pub fn verify_signed_with_public_key(
        &self,
        signature_text: &str,
        signature_bytes: Vec<u8>,
        public_key: Vec<u8>,
    ) -> Result<(), GenericError> {
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

    pub async fn sync_preferences(&self) -> Result<u64, GenericError> {
        let inner = self.inner_client.as_ref();
        let num_groups_synced = inner.sync_all_welcomes_and_history_sync_groups().await?;

        Ok(num_groups_synced as u64)
    }

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

    pub async fn register_identity(
        &self,
        signature_request: Arc<FfiSignatureRequest>,
    ) -> Result<(), GenericError> {
        let signature_request = signature_request.inner.lock().await;
        self.inner_client
            .register_identity(signature_request.clone())
            .await?;

        Ok(())
    }

    /// Manually trigger a device sync request to sync records from another active device on this account.
    pub async fn send_sync_request(&self) -> Result<(), GenericError> {
        self.inner_client
            .device_sync_client()
            .send_sync_request()
            .await?;
        Ok(())
    }

    /// Adds a wallet address to the existing client
    pub async fn add_identity(
        &self,
        new_identity: FfiIdentifier,
    ) -> Result<Arc<FfiSignatureRequest>, GenericError> {
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

    pub async fn apply_signature_request(
        &self,
        signature_request: Arc<FfiSignatureRequest>,
    ) -> Result<(), GenericError> {
        let signature_request = signature_request.inner.lock().await;
        self.inner_client
            .identity_updates()
            .apply_signature_request(signature_request.clone())
            .await?;

        Ok(())
    }

    /// Revokes or removes an identity from the existing client
    pub async fn revoke_identity(
        &self,
        identifier: FfiIdentifier,
    ) -> Result<Arc<FfiSignatureRequest>, GenericError> {
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
     */
    pub async fn revoke_all_other_installations(
        &self,
    ) -> Result<Arc<FfiSignatureRequest>, GenericError> {
        let installation_id = self.inner_client.installation_public_key();
        let inbox_state = self.inner_client.inbox_state(true).await?;
        let other_installation_ids = inbox_state
            .installation_ids()
            .into_iter()
            .filter(|id| id != installation_id)
            .collect();

        let signature_request = self
            .inner_client
            .identity_updates()
            .revoke_installations(other_installation_ids)
            .await?;

        Ok(Arc::new(FfiSignatureRequest {
            inner: Arc::new(tokio::sync::Mutex::new(signature_request)),
            scw_verifier: self.inner_client.scw_verifier().clone(),
        }))
    }

    /**
     * Revoke a list of installations
     */
    pub async fn revoke_installations(
        &self,
        installation_ids: Vec<Vec<u8>>,
    ) -> Result<Arc<FfiSignatureRequest>, GenericError> {
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
    pub async fn change_recovery_identifier(
        &self,
        new_recovery_identifier: FfiIdentifier,
    ) -> Result<Arc<FfiSignatureRequest>, GenericError> {
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

    /// Archive application elements to file for later restoration.
    pub async fn create_archive(
        &self,
        path: String,
        opts: FfiArchiveOptions,
        key: Vec<u8>,
    ) -> Result<(), GenericError> {
        let db = self.inner_client.context.db();
        let options: BackupOptions = opts.into();
        ArchiveExporter::export_to_file(options, db, path, &check_key(key)?)
            .await
            .map_err(DeviceSyncError::Archive)?;
        Ok(())
    }

    /// Import a previous archive
    pub async fn import_archive(&self, path: String, key: Vec<u8>) -> Result<(), GenericError> {
        let mut importer = ArchiveImporter::from_file(path, &check_key(key)?)
            .await
            .map_err(DeviceSyncError::Archive)?;
        insert_importer(&mut importer, &self.inner_client.context).await?;

        Ok(())
    }

    /// Load the metadata for an archive to see what it contains.
    /// Reads only the metadata without loading the entire file, so this function is quick.
    pub async fn archive_metadata(
        &self,
        path: String,
        key: Vec<u8>,
    ) -> Result<FfiBackupMetadata, GenericError> {
        let importer = ArchiveImporter::from_file(path, &check_key(key)?)
            .await
            .map_err(DeviceSyncError::Archive)?;
        Ok(importer.metadata.into())
    }

    /// Export an encrypted debug archive to a device sync server to inspect telemetry for debugging purposes.
    pub async fn upload_debug_archive(&self, server_url: String) -> Result<String, GenericError> {
        let db = self.inner_client.context.db();
        Ok(upload_debug_archive(db, Some(server_url)).await?)
    }
}

fn check_key(mut key: Vec<u8>) -> Result<Vec<u8>, GenericError> {
    if key.len() < 32 {
        return Err(GenericError::Generic {
            err: format!(
                "The encryption key must be at least {} bytes long.",
                ENC_KEY_SIZE
            ),
        });
    }
    key.truncate(ENC_KEY_SIZE);
    Ok(key)
}

#[derive(uniffi::Record)]
pub struct FfiBackupMetadata {
    backup_version: u16,
    elements: Vec<FfiBackupElementSelection>,
    exported_at_ns: i64,
    start_ns: Option<i64>,
    end_ns: Option<i64>,
}
impl From<BackupMetadata> for FfiBackupMetadata {
    fn from(value: BackupMetadata) -> Self {
        Self {
            backup_version: value.backup_version,
            elements: value
                .elements
                .into_iter()
                .filter_map(|selection| selection.try_into().ok())
                .collect(),
            start_ns: value.start_ns,
            end_ns: value.end_ns,
            exported_at_ns: value.exported_at_ns,
        }
    }
}

#[derive(uniffi::Record)]
pub struct FfiArchiveOptions {
    start_ns: Option<i64>,
    end_ns: Option<i64>,
    elements: Vec<FfiBackupElementSelection>,
}
impl From<FfiArchiveOptions> for BackupOptions {
    fn from(value: FfiArchiveOptions) -> Self {
        Self {
            start_ns: value.start_ns,
            end_ns: value.start_ns,
            elements: value
                .elements
                .into_iter()
                .map(|el| {
                    let element: BackupElementSelection = el.into();
                    element.into()
                })
                .collect(),
        }
    }
}

#[derive(uniffi::Enum)]
pub enum FfiBackupElementSelection {
    Messages,
    Consent,
}
impl From<FfiBackupElementSelection> for BackupElementSelection {
    fn from(value: FfiBackupElementSelection) -> Self {
        match value {
            FfiBackupElementSelection::Consent => Self::Consent,
            FfiBackupElementSelection::Messages => Self::Messages,
        }
    }
}

impl TryFrom<BackupElementSelection> for FfiBackupElementSelection {
    type Error = DeserializationError;
    fn try_from(value: BackupElementSelection) -> Result<Self, Self::Error> {
        let v = match value {
            BackupElementSelection::Consent => Self::Consent,
            BackupElementSelection::Messages => Self::Messages,
            _ => {
                return Err(DeserializationError::Unspecified(
                    "Backup Element Selection",
                ));
            }
        };
        Ok(v)
    }
}

impl From<HmacKey> for FfiHmacKey {
    fn from(value: HmacKey) -> Self {
        Self {
            epoch: value.epoch,
            key: value.key.to_vec(),
        }
    }
}

/// Signature kind used in identity operations
#[derive(uniffi::Enum, Clone, Debug, PartialEq)]
pub enum FfiSignatureKind {
    /// ERC-191 signature (Externally Owned Account/EOA)
    Erc191,
    /// ERC-1271 signature (Smart Contract Wallet/SCW)
    Erc1271,
    /// Installation key signature
    InstallationKey,
    /// Legacy delegated signature
    LegacyDelegated,
    /// P256 passkey signature
    P256,
}

impl From<xmtp_id::associations::SignatureKind> for FfiSignatureKind {
    fn from(kind: xmtp_id::associations::SignatureKind) -> Self {
        match kind {
            xmtp_id::associations::SignatureKind::Erc191 => FfiSignatureKind::Erc191,
            xmtp_id::associations::SignatureKind::Erc1271 => FfiSignatureKind::Erc1271,
            xmtp_id::associations::SignatureKind::InstallationKey => {
                FfiSignatureKind::InstallationKey
            }
            xmtp_id::associations::SignatureKind::LegacyDelegated => {
                FfiSignatureKind::LegacyDelegated
            }
            xmtp_id::associations::SignatureKind::P256 => FfiSignatureKind::P256,
        }
    }
}

#[derive(uniffi::Record)]
pub struct FfiInboxState {
    pub inbox_id: String,
    pub recovery_identity: FfiIdentifier,
    pub installations: Vec<FfiInstallation>,
    pub account_identities: Vec<FfiIdentifier>,
    pub creation_signature_kind: Option<FfiSignatureKind>,
}

#[derive(uniffi::Record)]
pub struct FfiHmacKey {
    key: Vec<u8>,
    epoch: i64,
}

#[derive(uniffi::Record)]
pub struct FfiInstallation {
    pub id: Vec<u8>,
    pub client_timestamp_ns: Option<u64>,
}

#[derive(uniffi::Record)]
pub struct FfiKeyPackageStatus {
    pub lifetime: Option<FfiLifetime>,
    pub validation_error: Option<String>,
}

#[derive(uniffi::Record)]
pub struct FfiLifetime {
    pub not_before: u64,
    pub not_after: u64,
}

impl From<VerifiedLifetime> for FfiLifetime {
    fn from(value: VerifiedLifetime) -> Self {
        Self {
            not_before: value.not_before,
            not_after: value.not_after,
        }
    }
}

impl From<VerifiedKeyPackageV2> for FfiKeyPackageStatus {
    fn from(value: VerifiedKeyPackageV2) -> Self {
        Self {
            lifetime: value.life_time().map(Into::into),
            validation_error: None,
        }
    }
}

impl From<AssociationState> for FfiInboxState {
    fn from(state: AssociationState) -> Self {
        Self {
            inbox_id: state.inbox_id().to_string(),
            recovery_identity: state.recovery_identifier().clone().into(),
            installations: state
                .members()
                .into_iter()
                .filter_map(|m| match m.identifier {
                    MemberIdentifier::Ethereum(_) => None,
                    MemberIdentifier::Passkey(_) => None,
                    MemberIdentifier::Installation(ident::Installation(id)) => {
                        Some(FfiInstallation {
                            id,
                            client_timestamp_ns: m.client_timestamp_ns,
                        })
                    }
                })
                .collect(),
            account_identities: state.identifiers().into_iter().map(Into::into).collect(),
            creation_signature_kind: None, // Will be populated by inbox_state method
        }
    }
}

#[derive(uniffi::Enum, Clone, Debug)]
pub enum FfiGroupQueryOrderBy {
    CreatedAt,
    LastActivity,
}

impl From<FfiGroupQueryOrderBy> for GroupQueryOrderBy {
    fn from(order_by: FfiGroupQueryOrderBy) -> Self {
        match order_by {
            FfiGroupQueryOrderBy::CreatedAt => GroupQueryOrderBy::CreatedAt,
            FfiGroupQueryOrderBy::LastActivity => GroupQueryOrderBy::LastActivity,
        }
    }
}

#[derive(uniffi::Record, Clone, Default)]
pub struct FfiSendMessageOpts {
    pub should_push: bool,
}

impl From<FfiSendMessageOpts> for xmtp_mls::groups::send_message_opts::SendMessageOpts {
    fn from(opts: FfiSendMessageOpts) -> Self {
        xmtp_mls::groups::send_message_opts::SendMessageOpts {
            should_push: opts.should_push,
        }
    }
}

#[derive(uniffi::Record, Default)]
pub struct FfiListConversationsOptions {
    pub created_after_ns: Option<i64>,
    pub created_before_ns: Option<i64>,
    pub last_activity_before_ns: Option<i64>,
    pub last_activity_after_ns: Option<i64>,
    pub order_by: Option<FfiGroupQueryOrderBy>,
    pub limit: Option<i64>,
    pub consent_states: Option<Vec<FfiConsentState>>,
    pub include_duplicate_dms: bool,
}

impl From<FfiListConversationsOptions> for GroupQueryArgs {
    fn from(opts: FfiListConversationsOptions) -> GroupQueryArgs {
        GroupQueryArgs {
            created_before_ns: opts.created_before_ns,
            created_after_ns: opts.created_after_ns,
            limit: opts.limit,
            consent_states: opts
                .consent_states
                .map(|vec| vec.into_iter().map(Into::into).collect()),
            include_duplicate_dms: opts.include_duplicate_dms,
            last_activity_before_ns: opts.last_activity_before_ns,
            last_activity_after_ns: opts.last_activity_after_ns,
            order_by: opts.order_by.map(Into::into),
            ..Default::default()
        }
    }
}

#[derive(uniffi::Object)]
pub struct FfiConversations {
    inner_client: Arc<RustXmtpClient>,
}

#[derive(uniffi::Enum, Clone, Debug)]
pub enum FfiGroupPermissionsOptions {
    Default,
    AdminOnly,
    CustomPolicy,
}

#[derive(uniffi::Enum, Debug)]
pub enum FfiPermissionUpdateType {
    AddMember,
    RemoveMember,
    AddAdmin,
    RemoveAdmin,
    UpdateMetadata,
}

impl From<&FfiPermissionUpdateType> for PermissionUpdateType {
    fn from(update_type: &FfiPermissionUpdateType) -> Self {
        match update_type {
            FfiPermissionUpdateType::AddMember => PermissionUpdateType::AddMember,
            FfiPermissionUpdateType::RemoveMember => PermissionUpdateType::RemoveMember,
            FfiPermissionUpdateType::AddAdmin => PermissionUpdateType::AddAdmin,
            FfiPermissionUpdateType::RemoveAdmin => PermissionUpdateType::RemoveAdmin,
            FfiPermissionUpdateType::UpdateMetadata => PermissionUpdateType::UpdateMetadata,
        }
    }
}

#[derive(uniffi::Enum, Clone, Debug, PartialEq, Eq)]
pub enum FfiPermissionPolicy {
    Allow,
    Deny,
    Admin,
    SuperAdmin,
    DoesNotExist,
    Other,
}

impl TryInto<PermissionPolicyOption> for FfiPermissionPolicy {
    type Error = GroupMutablePermissionsError;

    fn try_into(self) -> Result<PermissionPolicyOption, Self::Error> {
        match self {
            FfiPermissionPolicy::Allow => Ok(PermissionPolicyOption::Allow),
            FfiPermissionPolicy::Deny => Ok(PermissionPolicyOption::Deny),
            FfiPermissionPolicy::Admin => Ok(PermissionPolicyOption::AdminOnly),
            FfiPermissionPolicy::SuperAdmin => Ok(PermissionPolicyOption::SuperAdminOnly),
            _ => Err(GroupMutablePermissionsError::InvalidPermissionPolicyOption),
        }
    }
}

impl TryInto<MembershipPolicies> for FfiPermissionPolicy {
    type Error = GroupMutablePermissionsError;

    fn try_into(self) -> Result<MembershipPolicies, Self::Error> {
        match self {
            FfiPermissionPolicy::Allow => Ok(MembershipPolicies::allow()),
            FfiPermissionPolicy::Deny => Ok(MembershipPolicies::deny()),
            FfiPermissionPolicy::Admin => Ok(MembershipPolicies::allow_if_actor_admin()),
            FfiPermissionPolicy::SuperAdmin => Ok(MembershipPolicies::allow_if_actor_super_admin()),
            _ => Err(GroupMutablePermissionsError::InvalidPermissionPolicyOption),
        }
    }
}

impl TryInto<MetadataPolicies> for FfiPermissionPolicy {
    type Error = GroupMutablePermissionsError;

    fn try_into(self) -> Result<MetadataPolicies, Self::Error> {
        match self {
            FfiPermissionPolicy::Allow => Ok(MetadataPolicies::allow()),
            FfiPermissionPolicy::Deny => Ok(MetadataPolicies::deny()),
            FfiPermissionPolicy::Admin => Ok(MetadataPolicies::allow_if_actor_admin()),
            FfiPermissionPolicy::SuperAdmin => Ok(MetadataPolicies::allow_if_actor_super_admin()),
            _ => Err(GroupMutablePermissionsError::InvalidPermissionPolicyOption),
        }
    }
}

impl TryInto<PermissionsPolicies> for FfiPermissionPolicy {
    type Error = GroupMutablePermissionsError;

    fn try_into(self) -> Result<PermissionsPolicies, Self::Error> {
        match self {
            FfiPermissionPolicy::Deny => Ok(PermissionsPolicies::deny()),
            FfiPermissionPolicy::Admin => Ok(PermissionsPolicies::allow_if_actor_admin()),
            FfiPermissionPolicy::SuperAdmin => {
                Ok(PermissionsPolicies::allow_if_actor_super_admin())
            }
            _ => Err(GroupMutablePermissionsError::InvalidPermissionPolicyOption),
        }
    }
}

impl From<&MembershipPolicies> for FfiPermissionPolicy {
    fn from(policies: &MembershipPolicies) -> Self {
        if let MembershipPolicies::Standard(base_policy) = policies {
            match base_policy {
                BasePolicies::Allow => FfiPermissionPolicy::Allow,
                BasePolicies::Deny => FfiPermissionPolicy::Deny,
                BasePolicies::AllowSameMember => FfiPermissionPolicy::Other,
                BasePolicies::AllowIfAdminOrSuperAdmin => FfiPermissionPolicy::Admin,
                BasePolicies::AllowIfSuperAdmin => FfiPermissionPolicy::SuperAdmin,
            }
        } else {
            FfiPermissionPolicy::Other
        }
    }
}

impl From<&MetadataPolicies> for FfiPermissionPolicy {
    fn from(policies: &MetadataPolicies) -> Self {
        if let MetadataPolicies::Standard(base_policy) = policies {
            match base_policy {
                MetadataBasePolicies::Allow => FfiPermissionPolicy::Allow,
                MetadataBasePolicies::Deny => FfiPermissionPolicy::Deny,
                MetadataBasePolicies::AllowIfActorAdminOrSuperAdmin => FfiPermissionPolicy::Admin,
                MetadataBasePolicies::AllowIfActorSuperAdmin => FfiPermissionPolicy::SuperAdmin,
            }
        } else {
            FfiPermissionPolicy::Other
        }
    }
}

impl From<&PermissionsPolicies> for FfiPermissionPolicy {
    fn from(policies: &PermissionsPolicies) -> Self {
        if let PermissionsPolicies::Standard(base_policy) = policies {
            match base_policy {
                PermissionsBasePolicies::Deny => FfiPermissionPolicy::Deny,
                PermissionsBasePolicies::AllowIfActorAdminOrSuperAdmin => {
                    FfiPermissionPolicy::Admin
                }
                PermissionsBasePolicies::AllowIfActorSuperAdmin => FfiPermissionPolicy::SuperAdmin,
            }
        } else {
            FfiPermissionPolicy::Other
        }
    }
}

#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq)]
pub struct FfiPermissionPolicySet {
    pub add_member_policy: FfiPermissionPolicy,
    pub remove_member_policy: FfiPermissionPolicy,
    pub add_admin_policy: FfiPermissionPolicy,
    pub remove_admin_policy: FfiPermissionPolicy,
    pub update_group_name_policy: FfiPermissionPolicy,
    pub update_group_description_policy: FfiPermissionPolicy,
    pub update_group_image_url_square_policy: FfiPermissionPolicy,
    pub update_message_disappearing_policy: FfiPermissionPolicy,
}

impl From<PreconfiguredPolicies> for FfiGroupPermissionsOptions {
    fn from(policy: PreconfiguredPolicies) -> Self {
        match policy {
            PreconfiguredPolicies::Default => FfiGroupPermissionsOptions::Default,
            PreconfiguredPolicies::AdminsOnly => FfiGroupPermissionsOptions::AdminOnly,
        }
    }
}

impl TryFrom<FfiPermissionPolicySet> for PolicySet {
    type Error = GroupMutablePermissionsError;
    fn try_from(policy_set: FfiPermissionPolicySet) -> Result<Self, GroupMutablePermissionsError> {
        let mut metadata_permissions_map: HashMap<String, MetadataPolicies> = HashMap::new();
        metadata_permissions_map.insert(
            MetadataField::GroupName.to_string(),
            policy_set.update_group_name_policy.try_into()?,
        );
        metadata_permissions_map.insert(
            MetadataField::Description.to_string(),
            policy_set.update_group_description_policy.try_into()?,
        );
        metadata_permissions_map.insert(
            MetadataField::GroupImageUrlSquare.to_string(),
            policy_set.update_group_image_url_square_policy.try_into()?,
        );

        // MessageDisappearFromNS follows the same policy as MessageDisappearInNS
        metadata_permissions_map.insert(
            MetadataField::MessageDisappearFromNS.to_string(),
            policy_set
                .update_message_disappearing_policy
                .clone()
                .try_into()?,
        );
        metadata_permissions_map.insert(
            MetadataField::MessageDisappearInNS.to_string(),
            policy_set.update_message_disappearing_policy.try_into()?,
        );

        Ok(PolicySet {
            add_member_policy: policy_set.add_member_policy.try_into()?,
            remove_member_policy: policy_set.remove_member_policy.try_into()?,
            add_admin_policy: policy_set.add_admin_policy.try_into()?,
            remove_admin_policy: policy_set.remove_admin_policy.try_into()?,
            update_metadata_policy: metadata_permissions_map,
            update_permissions_policy: PermissionsPolicies::allow_if_actor_super_admin(),
        })
    }
}

#[derive(uniffi::Enum, Debug)]
pub enum FfiMetadataField {
    GroupName,
    Description,
    ImageUrlSquare,
}

impl From<&FfiMetadataField> for MetadataField {
    fn from(field: &FfiMetadataField) -> Self {
        match field {
            FfiMetadataField::GroupName => MetadataField::GroupName,
            FfiMetadataField::Description => MetadataField::Description,
            FfiMetadataField::ImageUrlSquare => MetadataField::GroupImageUrlSquare,
        }
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl FfiConversations {
    pub fn create_group_optimistic(
        &self,
        opts: FfiCreateGroupOptions,
    ) -> Result<Arc<FfiConversation>, GenericError> {
        log::info!("creating optimistic group");

        if let Some(FfiGroupPermissionsOptions::CustomPolicy) = opts.permissions {
            if opts.custom_permission_policy_set.is_none() {
                return Err(GenericError::Generic {
                    err: "CustomPolicy must include policy set".to_string(),
                });
            }
        } else if opts.custom_permission_policy_set.is_some() {
            return Err(GenericError::Generic {
                err: "Only CustomPolicy may specify a policy set".to_string(),
            });
        }

        let metadata_options = opts.clone().into_group_metadata_options();

        let group_permissions = match opts.permissions {
            Some(FfiGroupPermissionsOptions::Default) => {
                Some(xmtp_mls::groups::PreconfiguredPolicies::Default.to_policy_set())
            }
            Some(FfiGroupPermissionsOptions::AdminOnly) => {
                Some(xmtp_mls::groups::PreconfiguredPolicies::AdminsOnly.to_policy_set())
            }
            Some(FfiGroupPermissionsOptions::CustomPolicy) => {
                if let Some(policy_set) = opts.custom_permission_policy_set {
                    Some(policy_set.try_into()?)
                } else {
                    None
                }
            }
            _ => None,
        };

        let convo = self
            .inner_client
            .create_group(group_permissions, Some(metadata_options))?;

        Ok(Arc::new(convo.into()))
    }

    pub async fn create_group(
        &self,
        account_identities: Vec<FfiIdentifier>,
        opts: FfiCreateGroupOptions,
    ) -> Result<Arc<FfiConversation>, GenericError> {
        log::info!(
            "creating group with account addresses: {}",
            account_identities
                .iter()
                .map(|ident| format!("{ident}"))
                .collect::<Vec<_>>()
                .join(", ")
        );

        let convo = self.create_group_optimistic(opts)?;

        if !account_identities.is_empty() {
            convo.add_members(account_identities).await?;
        } else {
            convo.sync().await?;
        }

        Ok(convo)
    }

    pub async fn create_group_with_inbox_ids(
        &self,
        inbox_ids: Vec<String>,
        opts: FfiCreateGroupOptions,
    ) -> Result<Arc<FfiConversation>, GenericError> {
        log::info!(
            "creating group with account inbox ids: {}",
            inbox_ids.join(", ")
        );

        let convo = self.create_group_optimistic(opts)?;

        if !inbox_ids.is_empty() {
            convo.add_members_by_inbox_id(inbox_ids).await?;
        } else {
            convo.sync().await?;
        };

        Ok(convo)
    }

    pub async fn find_or_create_dm(
        &self,
        target_identity: FfiIdentifier,
        opts: FfiCreateDMOptions,
    ) -> Result<Arc<FfiConversation>, GenericError> {
        let target_identity = target_identity.try_into()?;
        log::info!("creating dm with target address: {target_identity:?}",);
        self.inner_client
            .find_or_create_dm(target_identity, Some(opts.into_dm_metadata_options()))
            .await
            .map(|g| Arc::new(g.into()))
            .map_err(Into::into)
    }

    pub async fn find_or_create_dm_by_inbox_id(
        &self,
        inbox_id: String,
        opts: FfiCreateDMOptions,
    ) -> Result<Arc<FfiConversation>, GenericError> {
        log::info!("creating dm with target inbox_id: {}", inbox_id);
        self.inner_client
            .find_or_create_dm_by_inbox_id(inbox_id, Some(opts.into_dm_metadata_options()))
            .await
            .map(|g| Arc::new(g.into()))
            .map_err(Into::into)
    }

    pub async fn process_streamed_welcome_message(
        &self,
        envelope_bytes: Vec<u8>,
    ) -> Result<Arc<FfiConversation>, GenericError> {
        self.inner_client
            .process_streamed_welcome_message(envelope_bytes)
            .await
            .map(|g| Arc::new(g.into()))
            .map_err(Into::into)
    }

    pub async fn sync(&self) -> Result<(), GenericError> {
        let inner = self.inner_client.as_ref();
        inner.sync_welcomes().await?;
        Ok(())
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn sync_all_conversations(
        &self,
        consent_states: Option<Vec<FfiConsentState>>,
    ) -> Result<u32, GenericError> {
        let inner = self.inner_client.as_ref();
        let consents: Option<Vec<ConsentState>> =
            consent_states.map(|states| states.into_iter().map(|state| state.into()).collect());
        let num_groups_synced: usize = inner.sync_all_welcomes_and_groups(consents).await?;
        // Convert usize to u32 for compatibility with Uniffi
        let num_groups_synced: u32 = num_groups_synced
            .try_into()
            .map_err(|_| GenericError::FailedToConvertToU32)?;

        Ok(num_groups_synced)
    }

    pub fn list(
        &self,
        opts: FfiListConversationsOptions,
    ) -> Result<Vec<Arc<FfiConversationListItem>>, GenericError> {
        let inner = self.inner_client.as_ref();
        let convo_list: Vec<Arc<FfiConversationListItem>> = inner
            .list_conversations(opts.into())?
            .into_iter()
            .map(|conversation_item| {
                Arc::new(FfiConversationListItem {
                    conversation: conversation_item.group.into(),
                    last_message: conversation_item
                        .last_message
                        .map(|stored_message| stored_message.into()),
                    is_commit_log_forked: conversation_item.is_commit_log_forked,
                })
            })
            .collect();

        Ok(convo_list)
    }

    pub fn list_groups(
        &self,
        opts: FfiListConversationsOptions,
    ) -> Result<Vec<Arc<FfiConversationListItem>>, GenericError> {
        let inner = self.inner_client.as_ref();
        let convo_list: Vec<Arc<FfiConversationListItem>> = inner
            .list_conversations(GroupQueryArgs {
                conversation_type: Some(ConversationType::Group),
                ..GroupQueryArgs::from(opts)
            })?
            .into_iter()
            .map(|conversation_item| {
                Arc::new(FfiConversationListItem {
                    conversation: conversation_item.group.into(),
                    last_message: conversation_item
                        .last_message
                        .map(|stored_message| stored_message.into()),
                    is_commit_log_forked: conversation_item.is_commit_log_forked,
                })
            })
            .collect();

        Ok(convo_list)
    }

    pub fn list_dms(
        &self,
        opts: FfiListConversationsOptions,
    ) -> Result<Vec<Arc<FfiConversationListItem>>, GenericError> {
        let inner = self.inner_client.as_ref();
        let convo_list: Vec<Arc<FfiConversationListItem>> = inner
            .list_conversations(GroupQueryArgs {
                conversation_type: Some(ConversationType::Dm),
                ..GroupQueryArgs::from(opts)
            })?
            .into_iter()
            .map(|conversation_item| {
                Arc::new(FfiConversationListItem {
                    conversation: conversation_item.group.into(),
                    last_message: conversation_item
                        .last_message
                        .map(|stored_message| stored_message.into()),
                    is_commit_log_forked: conversation_item.is_commit_log_forked,
                })
            })
            .collect();

        Ok(convo_list)
    }

    pub async fn stream_groups(
        &self,
        callback: Arc<dyn FfiConversationCallback>,
    ) -> FfiStreamCloser {
        let client = self.inner_client.clone();
        let close_cb = callback.clone();
        let handle = RustXmtpClient::stream_conversations_with_callback(
            client.clone(),
            Some(ConversationType::Group),
            move |convo| match convo {
                Ok(c) => callback.on_conversation(Arc::new(c.into())),
                Err(e) => callback.on_error(e.into()),
            },
            move || close_cb.on_close(),
            false,
        );

        FfiStreamCloser::new(handle)
    }

    pub async fn stream_dms(&self, callback: Arc<dyn FfiConversationCallback>) -> FfiStreamCloser {
        let client = self.inner_client.clone();
        let close_cb = callback.clone();
        let handle = RustXmtpClient::stream_conversations_with_callback(
            client.clone(),
            Some(ConversationType::Dm),
            move |convo| match convo {
                Ok(c) => callback.on_conversation(Arc::new(c.into())),
                Err(e) => callback.on_error(e.into()),
            },
            move || close_cb.on_close(),
            false,
        );

        FfiStreamCloser::new(handle)
    }

    pub async fn stream(&self, callback: Arc<dyn FfiConversationCallback>) -> FfiStreamCloser {
        let client = self.inner_client.clone();
        let close_cb = callback.clone();
        let handle = RustXmtpClient::stream_conversations_with_callback(
            client.clone(),
            None,
            move |convo| match convo {
                Ok(c) => callback.on_conversation(Arc::new(c.into())),
                Err(e) => callback.on_error(e.into()),
            },
            move || close_cb.on_close(),
            false,
        );

        FfiStreamCloser::new(handle)
    }

    pub async fn stream_all_group_messages(
        &self,
        message_callback: Arc<dyn FfiMessageCallback>,
        consent_states: Option<Vec<FfiConsentState>>,
    ) -> FfiStreamCloser {
        self.stream_messages(
            message_callback,
            Some(FfiConversationType::Group),
            consent_states,
        )
        .await
    }

    pub async fn stream_all_dm_messages(
        &self,
        message_callback: Arc<dyn FfiMessageCallback>,
        consent_states: Option<Vec<FfiConsentState>>,
    ) -> FfiStreamCloser {
        self.stream_messages(
            message_callback,
            Some(FfiConversationType::Dm),
            consent_states,
        )
        .await
    }

    pub async fn stream_all_messages(
        &self,
        message_callback: Arc<dyn FfiMessageCallback>,
        consent_states: Option<Vec<FfiConsentState>>,
    ) -> FfiStreamCloser {
        self.stream_messages(message_callback, None, consent_states)
            .await
    }

    async fn stream_messages(
        &self,
        message_callback: Arc<dyn FfiMessageCallback>,
        conversation_type: Option<FfiConversationType>,
        consent_states: Option<Vec<FfiConsentState>>,
    ) -> FfiStreamCloser {
        let consents: Option<Vec<ConsentState>> =
            consent_states.map(|states| states.into_iter().map(|state| state.into()).collect());
        let close_cb = message_callback.clone();
        let handle = RustXmtpClient::stream_all_messages_with_callback(
            self.inner_client.context.clone(),
            conversation_type.map(Into::into),
            consents,
            move |msg| match msg {
                Ok(m) => message_callback.on_message(m.into()),
                Err(e) => message_callback.on_error(e.into()),
            },
            move || close_cb.on_close(),
        );

        FfiStreamCloser::new(handle)
    }

    /// Get notified when there is a new consent update either locally or is synced from another device
    /// allowing the user to re-render the new state appropriately
    pub async fn stream_consent(&self, callback: Arc<dyn FfiConsentCallback>) -> FfiStreamCloser {
        let close_cb = callback.clone();
        let handle = RustXmtpClient::stream_consent_with_callback(
            self.inner_client.clone(),
            move |msg| match msg {
                Ok(m) => callback.on_consent_update(m.into_iter().map(Into::into).collect()),
                Err(e) => callback.on_error(e.into()),
            },
            move || close_cb.on_close(),
        );

        FfiStreamCloser::new(handle)
    }

    /// Get notified when a preference changes either locally or is synced from another device
    /// allowing the user to re-render the new state appropriately.
    pub async fn stream_preferences(
        &self,
        callback: Arc<dyn FfiPreferenceCallback>,
    ) -> FfiStreamCloser {
        let close_cb = callback.clone();
        let handle = RustXmtpClient::stream_preferences_with_callback(
            self.inner_client.clone(),
            move |msg| match msg {
                Ok(m) => callback.on_preference_update(
                    m.into_iter().filter_map(|v| v.try_into().ok()).collect(),
                ),
                Err(e) => callback.on_error(e.into()),
            },
            move || close_cb.on_close(),
        );

        FfiStreamCloser::new(handle)
    }

    pub fn get_hmac_keys(&self) -> Result<HashMap<Vec<u8>, Vec<FfiHmacKey>>, GenericError> {
        let inner = self.inner_client.as_ref();
        let conversations = inner.find_groups(GroupQueryArgs {
            include_duplicate_dms: true,
            ..GroupQueryArgs::default()
        })?;

        let mut hmac_map = HashMap::new();
        for conversation in conversations {
            let id = conversation.group_id.clone();
            let keys = conversation
                .hmac_keys(-1..=1)?
                .into_iter()
                .map(Into::into)
                .collect::<Vec<_>>();

            hmac_map.insert(id, keys);
        }

        Ok(hmac_map)
    }
}

#[cfg(test)]
impl FfiConversations {
    pub async fn get_sync_group(&self) -> Result<FfiConversation, GenericError> {
        let inner = self.inner_client.as_ref();
        let sync_group = inner.device_sync_client().get_sync_group().await?;
        Ok(sync_group.into())
    }
}

impl From<FfiConversationType> for ConversationType {
    fn from(value: FfiConversationType) -> Self {
        match value {
            FfiConversationType::Dm => ConversationType::Dm,
            FfiConversationType::Group => ConversationType::Group,
            FfiConversationType::Sync => ConversationType::Sync,
            FfiConversationType::Oneshot => ConversationType::Oneshot,
        }
    }
}

impl TryFrom<PreferenceUpdate> for FfiPreferenceUpdate {
    type Error = GenericError;
    fn try_from(value: PreferenceUpdate) -> Result<Self, Self::Error> {
        match value {
            PreferenceUpdate::Hmac { key, .. } => Ok(FfiPreferenceUpdate::HMAC { key }),
            // These are filtered out in the stream and should not be here
            // We're keeping preference update and consent streams separate right now.
            PreferenceUpdate::Consent(_) => Err(GenericError::Generic {
                err: "Consent updates should be filtered out.".to_string(),
            }),
        }
    }
}

#[derive(uniffi::Object, Clone)]
pub struct FfiConversation {
    inner: RustMlsGroup,
}

#[derive(uniffi::Object)]
pub struct FfiConversationListItem {
    conversation: FfiConversation,
    last_message: Option<FfiMessage>,
    is_commit_log_forked: Option<bool>,
}

#[uniffi::export]
impl FfiConversationListItem {
    pub fn conversation(&self) -> Arc<FfiConversation> {
        Arc::new(self.conversation.clone())
    }
    pub fn last_message(&self) -> Option<FfiMessage> {
        self.last_message.clone()
    }

    pub fn is_commit_log_forked(&self) -> Option<bool> {
        self.is_commit_log_forked
    }
}

#[derive(uniffi::Record, Debug)]
pub struct FfiUpdateGroupMembershipResult {
    added_members: HashMap<String, u64>,
    removed_members: Vec<String>,
    failed_installations: Vec<Vec<u8>>,
}

impl FfiUpdateGroupMembershipResult {
    fn new(
        added_members: HashMap<String, u64>,
        removed_members: Vec<String>,
        failed_installations: Vec<Vec<u8>>,
    ) -> Self {
        FfiUpdateGroupMembershipResult {
            added_members,
            removed_members,
            failed_installations,
        }
    }
}

impl From<UpdateGroupMembershipResult> for FfiUpdateGroupMembershipResult {
    fn from(value: UpdateGroupMembershipResult) -> Self {
        FfiUpdateGroupMembershipResult::new(
            value.added_members,
            value.removed_members,
            value.failed_installations,
        )
    }
}

/// Settings for disappearing messages in a conversation.
///
/// # Fields
///
/// * `from_ns` - The timestamp (in nanoseconds) from when messages should be tracked for deletion.
/// * `in_ns` - The duration (in nanoseconds) after which tracked messages will be deleted.
#[derive(uniffi::Record, Clone, Debug)]
pub struct FfiMessageDisappearingSettings {
    pub from_ns: i64,
    pub in_ns: i64,
}

impl FfiMessageDisappearingSettings {
    fn new(from_ns: i64, in_ns: i64) -> Self {
        Self { from_ns, in_ns }
    }
}

impl From<MessageDisappearingSettings> for FfiMessageDisappearingSettings {
    fn from(value: MessageDisappearingSettings) -> Self {
        FfiMessageDisappearingSettings::new(value.from_ns, value.in_ns)
    }
}

#[derive(uniffi::Record, Debug, Clone, Copy)]
pub struct FfiCursor {
    originator_id: u32,
    sequence_id: u64,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct FfiConversationDebugInfo {
    pub epoch: u64,
    pub maybe_forked: bool,
    pub fork_details: String,
    pub is_commit_log_forked: Option<bool>,
    pub local_commit_log: String,
    pub remote_commit_log: String,
    pub cursor: Vec<FfiCursor>,
}

impl From<Cursor> for FfiCursor {
    fn from(value: Cursor) -> Self {
        FfiCursor {
            sequence_id: value.sequence_id,
            originator_id: value.originator_id,
        }
    }
}

impl FfiConversationDebugInfo {
    fn new(
        epoch: u64,
        maybe_forked: bool,
        fork_details: String,
        is_commit_log_forked: Option<bool>,
        local_commit_log: String,
        remote_commit_log: String,
        cursor: Vec<Cursor>,
    ) -> Self {
        Self {
            epoch,
            maybe_forked,
            fork_details,
            is_commit_log_forked,
            local_commit_log,
            remote_commit_log,
            cursor: cursor.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<ConversationDebugInfo> for FfiConversationDebugInfo {
    fn from(value: ConversationDebugInfo) -> Self {
        FfiConversationDebugInfo::new(
            value.epoch,
            value.maybe_forked,
            value.fork_details,
            value.is_commit_log_forked,
            value.local_commit_log,
            value.remote_commit_log,
            value.cursor,
        )
    }
}

impl From<RustMlsGroup> for FfiConversation {
    fn from(mls_group: RustMlsGroup) -> FfiConversation {
        FfiConversation { inner: mls_group }
    }
}

impl From<StoredConsentRecord> for FfiConsent {
    fn from(consent: StoredConsentRecord) -> Self {
        FfiConsent {
            entity: consent.entity,
            entity_type: match consent.entity_type {
                ConsentType::ConversationId => FfiConsentEntityType::ConversationId,
                ConsentType::InboxId => FfiConsentEntityType::InboxId,
            },
            state: consent.state.into(),
        }
    }
}

#[derive(uniffi::Record)]
pub struct FfiConversationMember {
    pub inbox_id: String,
    pub account_identifiers: Vec<FfiIdentifier>,
    pub installation_ids: Vec<Vec<u8>>,
    pub permission_level: FfiPermissionLevel,
    pub consent_state: FfiConsentState,
}

#[derive(uniffi::Enum)]
pub enum FfiPermissionLevel {
    Member,
    Admin,
    SuperAdmin,
}

#[derive(uniffi::Enum, PartialEq, Debug)]
pub enum FfiConsentState {
    Unknown,
    Allowed,
    Denied,
}

impl From<ConsentState> for FfiConsentState {
    fn from(state: ConsentState) -> Self {
        match state {
            ConsentState::Unknown => FfiConsentState::Unknown,
            ConsentState::Allowed => FfiConsentState::Allowed,
            ConsentState::Denied => FfiConsentState::Denied,
        }
    }
}

impl From<FfiConsentState> for ConsentState {
    fn from(state: FfiConsentState) -> Self {
        match state {
            FfiConsentState::Unknown => ConsentState::Unknown,
            FfiConsentState::Allowed => ConsentState::Allowed,
            FfiConsentState::Denied => ConsentState::Denied,
        }
    }
}

#[derive(uniffi::Enum)]
pub enum FfiConsentEntityType {
    ConversationId,
    InboxId,
}

impl From<FfiConsentEntityType> for ConsentType {
    fn from(entity_type: FfiConsentEntityType) -> Self {
        match entity_type {
            FfiConsentEntityType::ConversationId => ConsentType::ConversationId,
            FfiConsentEntityType::InboxId => ConsentType::InboxId,
        }
    }
}

#[derive(uniffi::Enum, Clone)]
pub enum FfiDirection {
    Ascending,
    Descending,
}

impl From<FfiDirection> for SortDirection {
    fn from(direction: FfiDirection) -> Self {
        match direction {
            FfiDirection::Ascending => SortDirection::Ascending,
            FfiDirection::Descending => SortDirection::Descending,
        }
    }
}

impl From<FfiMessageDisappearingSettings> for MessageDisappearingSettings {
    fn from(settings: FfiMessageDisappearingSettings) -> Self {
        MessageDisappearingSettings::new(settings.from_ns, settings.in_ns)
    }
}

#[derive(uniffi::Record, Clone, Default)]
pub struct FfiListMessagesOptions {
    pub sent_before_ns: Option<i64>,
    pub sent_after_ns: Option<i64>,
    pub limit: Option<i64>,
    pub delivery_status: Option<FfiDeliveryStatus>,
    pub direction: Option<FfiDirection>,
    pub content_types: Option<Vec<FfiContentType>>,
    pub exclude_content_types: Option<Vec<FfiContentType>>,
    pub exclude_sender_inbox_ids: Option<Vec<String>>,
}

impl From<FfiListMessagesOptions> for MsgQueryArgs {
    fn from(opts: FfiListMessagesOptions) -> Self {
        MsgQueryArgs {
            kind: None,
            sent_before_ns: opts.sent_before_ns,
            sent_after_ns: opts.sent_after_ns,
            limit: opts.limit,
            delivery_status: opts.delivery_status.map(Into::into),
            direction: opts.direction.map(Into::into),
            content_types: opts
                .content_types
                .map(|types| types.into_iter().map(Into::into).collect()),
            exclude_content_types: opts
                .exclude_content_types
                .map(|types| types.into_iter().map(Into::into).collect()),
            exclude_sender_inbox_ids: opts.exclude_sender_inbox_ids,
        }
    }
}

#[derive(uniffi::Enum, Clone)]
pub enum FfiContentType {
    Unknown,
    Text,
    GroupMembershipChange,
    GroupUpdated,
    Reaction,
    ReadReceipt,
    Reply,
    Attachment,
    RemoteAttachment,
    TransactionReference,
}

impl From<FfiContentType> for ContentType {
    fn from(value: FfiContentType) -> Self {
        match value {
            FfiContentType::Unknown => ContentType::Unknown,
            FfiContentType::Text => ContentType::Text,
            FfiContentType::GroupMembershipChange => ContentType::GroupMembershipChange,
            FfiContentType::GroupUpdated => ContentType::GroupUpdated,
            FfiContentType::Reaction => ContentType::Reaction,
            FfiContentType::ReadReceipt => ContentType::ReadReceipt,
            FfiContentType::Reply => ContentType::Reply,
            FfiContentType::Attachment => ContentType::Attachment,
            FfiContentType::RemoteAttachment => ContentType::RemoteAttachment,
            FfiContentType::TransactionReference => ContentType::TransactionReference,
        }
    }
}

#[derive(uniffi::Record, Clone, Default)]
pub struct FfiCreateGroupOptions {
    pub permissions: Option<FfiGroupPermissionsOptions>,
    pub group_name: Option<String>,
    pub group_image_url_square: Option<String>,
    pub group_description: Option<String>,
    pub custom_permission_policy_set: Option<FfiPermissionPolicySet>,
    pub message_disappearing_settings: Option<FfiMessageDisappearingSettings>,
}

impl FfiCreateGroupOptions {
    pub fn into_group_metadata_options(self) -> GroupMetadataOptions {
        GroupMetadataOptions {
            name: self.group_name,
            image_url_square: self.group_image_url_square,
            description: self.group_description,
            message_disappearing_settings: self
                .message_disappearing_settings
                .map(|settings| settings.into()),
        }
    }
}

#[derive(uniffi::Record, Clone, Default)]
pub struct FfiCreateDMOptions {
    pub message_disappearing_settings: Option<FfiMessageDisappearingSettings>,
}

impl FfiCreateDMOptions {
    pub fn new(disappearing_settings: FfiMessageDisappearingSettings) -> Self {
        FfiCreateDMOptions {
            message_disappearing_settings: Some(disappearing_settings),
        }
    }
    pub fn into_dm_metadata_options(self) -> DMMetadataOptions {
        DMMetadataOptions {
            message_disappearing_settings: self
                .message_disappearing_settings
                .map(|settings| settings.into()),
        }
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl FfiConversation {
    pub async fn send(
        &self,
        content_bytes: Vec<u8>,
        opts: FfiSendMessageOpts,
    ) -> Result<Vec<u8>, GenericError> {
        let message_id = self
            .inner
            .send_message(content_bytes.as_slice(), opts.into())
            .await?;
        Ok(message_id)
    }

    pub(crate) async fn send_text(&self, text: &str) -> Result<Vec<u8>, GenericError> {
        let content = TextCodec::encode(text.to_string())
            .map_err(|e| GenericError::Generic { err: e.to_string() })?;
        self.send(
            encoded_content_to_bytes(content),
            FfiSendMessageOpts { should_push: true },
        )
        .await
    }

    /// send a message without immediately publishing to the delivery service.
    pub fn send_optimistic(
        &self,
        content_bytes: Vec<u8>,
        opts: FfiSendMessageOpts,
    ) -> Result<Vec<u8>, GenericError> {
        let id = self
            .inner
            .send_message_optimistic(content_bytes.as_slice(), opts.into())?;

        Ok(id)
    }

    /// Publish all unpublished messages
    pub async fn publish_messages(&self) -> Result<(), GenericError> {
        self.inner.publish_messages().await?;
        Ok(())
    }

    pub async fn sync(&self) -> Result<(), GenericError> {
        self.inner.sync().await?;

        Ok(())
    }

    pub async fn find_messages(
        &self,
        opts: FfiListMessagesOptions,
    ) -> Result<Vec<FfiMessage>, GenericError> {
        let messages: Vec<FfiMessage> = self
            .inner
            .find_messages(&opts.into())?
            .into_iter()
            .map(|msg| msg.into())
            .collect();

        Ok(messages)
    }

    pub fn count_messages(&self, opts: FfiListMessagesOptions) -> Result<i64, GenericError> {
        let count = self.inner.count_messages(&opts.into())?;

        Ok(count)
    }

    pub fn find_messages_with_reactions(
        &self,
        opts: FfiListMessagesOptions,
    ) -> Result<Vec<FfiMessageWithReactions>, GenericError> {
        let messages: Vec<FfiMessageWithReactions> = self
            .inner
            .find_messages_with_reactions(&opts.into())?
            .into_iter()
            .map(|msg| msg.into())
            .collect();
        Ok(messages)
    }

    pub fn find_enriched_messages(
        &self,
        opts: FfiListMessagesOptions,
    ) -> Result<Vec<Arc<FfiDecodedMessage>>, GenericError> {
        let messages: Vec<Arc<FfiDecodedMessage>> = self
            .inner
            .find_messages_v2(&opts.into())?
            .into_iter()
            .map(|msg| Arc::new(msg.into()))
            .collect();
        Ok(messages)
    }

    pub async fn process_streamed_conversation_message(
        &self,
        envelope_bytes: Vec<u8>,
    ) -> Result<FfiMessage, FfiSubscribeError> {
        let message = self
            .inner
            .process_streamed_group_message(envelope_bytes)
            .await?;
        let ffi_message = message.into();

        Ok(ffi_message)
    }

    pub async fn list_members(&self) -> Result<Vec<FfiConversationMember>, GenericError> {
        let members: Vec<FfiConversationMember> = self
            .inner
            .members()
            .await?
            .into_iter()
            .map(|member| FfiConversationMember {
                inbox_id: member.inbox_id,
                account_identifiers: member.account_identifiers.to_ffi(),
                installation_ids: member.installation_ids,
                permission_level: match member.permission_level {
                    PermissionLevel::Member => FfiPermissionLevel::Member,
                    PermissionLevel::Admin => FfiPermissionLevel::Admin,
                    PermissionLevel::SuperAdmin => FfiPermissionLevel::SuperAdmin,
                },
                consent_state: member.consent_state.into(),
            })
            .collect();

        Ok(members)
    }

    pub async fn add_members(
        &self,
        account_identifiers: Vec<FfiIdentifier>,
    ) -> Result<FfiUpdateGroupMembershipResult, GenericError> {
        let account_identifiers = account_identifiers.to_internal()?;
        log::info!(
            "adding members: {}",
            account_identifiers
                .iter()
                .map(|ident| format!("{ident}"))
                .collect::<Vec<_>>()
                .join(",")
        );

        self.inner
            .add_members(&account_identifiers)
            .await
            .map(FfiUpdateGroupMembershipResult::from)
            .map_err(Into::into)
    }

    pub async fn add_members_by_inbox_id(
        &self,
        inbox_ids: Vec<String>,
    ) -> Result<FfiUpdateGroupMembershipResult, GenericError> {
        log::info!("Adding members by inbox ID: {}", inbox_ids.join(", "));

        self.inner
            .add_members_by_inbox_id(&inbox_ids)
            .await
            .map(FfiUpdateGroupMembershipResult::from)
            .map_err(Into::into)
    }

    pub async fn remove_members(
        &self,
        account_identifiers: Vec<FfiIdentifier>,
    ) -> Result<(), GenericError> {
        self.inner
            .remove_members(&account_identifiers.to_internal()?)
            .await
            .map_err(Into::into)
    }

    pub async fn remove_members_by_inbox_id(
        &self,
        inbox_ids: Vec<String>,
    ) -> Result<(), GenericError> {
        let ids = inbox_ids.iter().map(AsRef::as_ref).collect::<Vec<&str>>();
        self.inner
            .remove_members_by_inbox_id(ids.as_slice())
            .await?;
        Ok(())
    }

    pub async fn update_group_name(&self, group_name: String) -> Result<(), GenericError> {
        self.inner.update_group_name(group_name).await?;
        Ok(())
    }

    pub fn group_name(&self) -> Result<String, GenericError> {
        let group_name = self.inner.group_name()?;
        Ok(group_name)
    }

    pub async fn update_group_image_url_square(
        &self,
        group_image_url_square: String,
    ) -> Result<(), GenericError> {
        self.inner
            .update_group_image_url_square(group_image_url_square)
            .await?;

        Ok(())
    }

    pub fn group_image_url_square(&self) -> Result<String, GenericError> {
        Ok(self.inner.group_image_url_square()?)
    }

    pub async fn update_group_description(
        &self,
        group_description: String,
    ) -> Result<(), GenericError> {
        self.inner
            .update_group_description(group_description)
            .await?;

        Ok(())
    }

    pub fn group_description(&self) -> Result<String, GenericError> {
        Ok(self.inner.group_description()?)
    }

    pub async fn update_conversation_message_disappearing_settings(
        &self,
        settings: FfiMessageDisappearingSettings,
    ) -> Result<(), GenericError> {
        self.inner
            .update_conversation_message_disappearing_settings(MessageDisappearingSettings::from(
                settings,
            ))
            .await?;

        Ok(())
    }

    pub async fn remove_conversation_message_disappearing_settings(
        &self,
    ) -> Result<(), GenericError> {
        self.inner
            .remove_conversation_message_disappearing_settings()
            .await?;

        Ok(())
    }

    pub fn conversation_message_disappearing_settings(
        &self,
    ) -> Result<Option<FfiMessageDisappearingSettings>, GenericError> {
        let settings = self.inner.disappearing_settings()?;

        match settings {
            Some(s) => Ok(Some(FfiMessageDisappearingSettings::from(s))),
            None => Ok(None),
        }
    }

    pub fn is_conversation_message_disappearing_enabled(&self) -> Result<bool, GenericError> {
        self.conversation_message_disappearing_settings()
            .map(|settings| {
                settings
                    .as_ref()
                    .is_some_and(|s| s.from_ns > 0 && s.in_ns > 0)
            })
    }

    pub fn admin_list(&self) -> Result<Vec<String>, GenericError> {
        self.inner.admin_list().map_err(Into::into)
    }

    pub fn super_admin_list(&self) -> Result<Vec<String>, GenericError> {
        self.inner.super_admin_list().map_err(Into::into)
    }

    pub fn is_admin(&self, inbox_id: &String) -> Result<bool, GenericError> {
        let admin_list = self.admin_list()?;
        Ok(admin_list.contains(inbox_id))
    }

    pub fn is_super_admin(&self, inbox_id: &String) -> Result<bool, GenericError> {
        let super_admin_list = self.super_admin_list()?;
        Ok(super_admin_list.contains(inbox_id))
    }

    pub async fn add_admin(&self, inbox_id: String) -> Result<(), GenericError> {
        self.inner
            .update_admin_list(UpdateAdminListType::Add, inbox_id)
            .await
            .map_err(Into::into)
    }

    pub async fn remove_admin(&self, inbox_id: String) -> Result<(), GenericError> {
        self.inner
            .update_admin_list(UpdateAdminListType::Remove, inbox_id)
            .await
            .map_err(Into::into)
    }

    pub async fn add_super_admin(&self, inbox_id: String) -> Result<(), GenericError> {
        self.inner
            .update_admin_list(UpdateAdminListType::AddSuper, inbox_id)
            .await
            .map_err(Into::into)
    }

    pub async fn remove_super_admin(&self, inbox_id: String) -> Result<(), GenericError> {
        self.inner
            .update_admin_list(UpdateAdminListType::RemoveSuper, inbox_id)
            .await
            .map_err(Into::into)
    }

    pub fn group_permissions(&self) -> Result<Arc<FfiGroupPermissions>, GenericError> {
        let permissions = self.inner.permissions()?;
        Ok(Arc::new(FfiGroupPermissions {
            inner: Arc::new(permissions),
        }))
    }

    pub async fn update_permission_policy(
        &self,
        permission_update_type: FfiPermissionUpdateType,
        permission_policy_option: FfiPermissionPolicy,
        metadata_field: Option<FfiMetadataField>,
    ) -> Result<(), GenericError> {
        self.inner
            .update_permission_policy(
                PermissionUpdateType::from(&permission_update_type),
                permission_policy_option.try_into()?,
                metadata_field.map(|field| MetadataField::from(&field)),
            )
            .await
            .map_err(Into::into)
    }

    pub async fn stream(&self, message_callback: Arc<dyn FfiMessageCallback>) -> FfiStreamCloser {
        let close_cb = message_callback.clone();
        let handle = MlsGroup::stream_with_callback(
            self.inner.context.clone(),
            self.id(),
            move |message| match message {
                Ok(m) => message_callback.on_message(m.into()),
                Err(e) => message_callback.on_error(e.into()),
            },
            move || close_cb.on_close(),
        );

        FfiStreamCloser::new(handle)
    }

    pub fn created_at_ns(&self) -> i64 {
        self.inner.created_at_ns
    }

    pub fn is_active(&self) -> Result<bool, GenericError> {
        self.inner.is_active().map_err(Into::into)
    }

    pub fn paused_for_version(&self) -> Result<Option<String>, GenericError> {
        self.inner.paused_for_version().map_err(Into::into)
    }

    pub fn consent_state(&self) -> Result<FfiConsentState, GenericError> {
        self.inner
            .consent_state()
            .map(Into::into)
            .map_err(Into::into)
    }

    pub fn update_consent_state(&self, state: FfiConsentState) -> Result<(), GenericError> {
        self.inner
            .update_consent_state(state.into())
            .map_err(Into::into)
    }

    pub fn added_by_inbox_id(&self) -> Result<String, GenericError> {
        self.inner.added_by_inbox_id().map_err(Into::into)
    }

    pub async fn group_metadata(&self) -> Result<Arc<FfiConversationMetadata>, GenericError> {
        let metadata = self.inner.metadata().await?;
        Ok(Arc::new(FfiConversationMetadata {
            inner: Arc::new(metadata),
        }))
    }

    pub fn dm_peer_inbox_id(&self) -> Option<String> {
        self.inner
            .dm_id
            .as_ref()
            .map(|dm_id| dm_id.other_inbox_id(self.inner.context.inbox_id()))
    }

    pub fn get_hmac_keys(&self) -> Result<HashMap<Vec<u8>, Vec<FfiHmacKey>>, GenericError> {
        let duplicate_dms = self.inner.find_duplicate_dms()?;

        let mut hmac_map = HashMap::new();
        for conversation in duplicate_dms {
            let id = conversation.group_id.clone();
            let keys = conversation
                .hmac_keys(-1..=1)?
                .into_iter()
                .map(Into::into)
                .collect::<Vec<_>>();

            hmac_map.insert(id, keys);
        }

        let keys = self
            .inner
            .hmac_keys(-1..=1)?
            .into_iter()
            .map(Into::into)
            .collect::<Vec<_>>();

        hmac_map.insert(self.id(), keys);

        Ok(hmac_map)
    }

    pub async fn conversation_debug_info(&self) -> Result<FfiConversationDebugInfo, GenericError> {
        let debug_info = self.inner.debug_info().await?;
        Ok(debug_info.into())
    }

    pub async fn find_duplicate_dms(&self) -> Result<Vec<Arc<FfiConversation>>, GenericError> {
        let dms = self.inner.find_duplicate_dms()?;

        let ffi_conversations: Vec<Arc<FfiConversation>> =
            dms.into_iter().map(|dm| Arc::new(dm.into())).collect();

        Ok(ffi_conversations)
    }

    pub fn get_last_read_times(&self) -> Result<HashMap<String, i64>, GenericError> {
        let latest_read_times = self.inner.get_last_read_times()?;
        Ok(latest_read_times)
    }
}

#[uniffi::export]
impl FfiConversation {
    pub fn id(&self) -> Vec<u8> {
        self.inner.group_id.clone()
    }

    pub fn conversation_type(&self) -> FfiConversationType {
        self.inner.conversation_type.into()
    }
}

#[derive(uniffi::Enum, PartialEq, Debug, Clone)]
pub enum FfiConversationMessageKind {
    Application,
    MembershipChange,
}

impl From<GroupMessageKind> for FfiConversationMessageKind {
    fn from(kind: GroupMessageKind) -> Self {
        match kind {
            GroupMessageKind::Application => FfiConversationMessageKind::Application,
            GroupMessageKind::MembershipChange => FfiConversationMessageKind::MembershipChange,
        }
    }
}

#[derive(uniffi::Enum, PartialEq, Debug, Clone)]
pub enum FfiConversationType {
    Group,
    Dm,
    Sync,
    Oneshot,
}

impl From<ConversationType> for FfiConversationType {
    fn from(kind: ConversationType) -> Self {
        match kind {
            ConversationType::Group => FfiConversationType::Group,
            ConversationType::Dm => FfiConversationType::Dm,
            ConversationType::Sync => FfiConversationType::Sync,
            ConversationType::Oneshot => FfiConversationType::Oneshot,
        }
    }
}

#[derive(uniffi::Record)]
pub struct FfiMessageWithReactions {
    pub message: FfiMessage,
    pub reactions: Vec<FfiMessage>,
}

impl From<StoredGroupMessageWithReactions> for FfiMessageWithReactions {
    fn from(msg_with_reactions: StoredGroupMessageWithReactions) -> Self {
        Self {
            message: msg_with_reactions.message.into(),
            reactions: msg_with_reactions
                .reactions
                .into_iter()
                .map(|reaction| reaction.into())
                .collect(),
        }
    }
}

#[uniffi::export]
pub fn encode_reaction(reaction: FfiReactionPayload) -> Result<Vec<u8>, GenericError> {
    // Convert FfiReaction to Reaction
    let reaction: ReactionV2 = reaction.into();

    // Use ReactionCodec to encode the reaction
    let encoded = ReactionCodec::encode(reaction)
        .map_err(|e| GenericError::Generic { err: e.to_string() })?;

    // Encode the EncodedContent to bytes
    let mut buf = Vec::new();
    encoded
        .encode(&mut buf)
        .map_err(|e| GenericError::Generic { err: e.to_string() })?;

    Ok(buf)
}

#[uniffi::export]
pub fn decode_reaction(bytes: Vec<u8>) -> Result<FfiReactionPayload, GenericError> {
    // Decode bytes into EncodedContent
    let encoded_content = EncodedContent::decode(bytes.as_slice())
        .map_err(|e| GenericError::Generic { err: e.to_string() })?;

    // Use ReactionCodec to decode into Reaction and convert to FfiReaction
    ReactionCodec::decode(encoded_content)
        .map(Into::into)
        .map_err(|e| GenericError::Generic { err: e.to_string() })
}

// RemoteAttachmentInfo and MultiRemoteAttachment FFI structures - using types from message module

#[uniffi::export]
pub fn encode_multi_remote_attachment(
    ffi_multi_remote_attachment: FfiMultiRemoteAttachment,
) -> Result<Vec<u8>, GenericError> {
    // Convert FfiMultiRemoteAttachment to MultiRemoteAttachment
    let multi_remote_attachment: MultiRemoteAttachment = ffi_multi_remote_attachment.into();

    // Use MultiRemoteAttachmentCodec to encode the reaction
    let encoded = MultiRemoteAttachmentCodec::encode(multi_remote_attachment)
        .map_err(|e| GenericError::Generic { err: e.to_string() })?;

    // Encode the EncodedContent to bytes
    let mut buf = Vec::new();
    encoded
        .encode(&mut buf)
        .map_err(|e| GenericError::Generic { err: e.to_string() })?;

    Ok(buf)
}

#[uniffi::export]
pub fn decode_multi_remote_attachment(
    bytes: Vec<u8>,
) -> Result<FfiMultiRemoteAttachment, GenericError> {
    // Decode bytes into EncodedContent
    let encoded_content = EncodedContent::decode(bytes.as_slice())
        .map_err(|e| GenericError::Generic { err: e.to_string() })?;

    // Use MultiRemoteAttachmentCodec to decode into MultiRemoteAttachment and convert to FfiMultiRemoteAttachment
    MultiRemoteAttachmentCodec::decode(encoded_content)
        .map(Into::into)
        .map_err(|e| GenericError::Generic { err: e.to_string() })
}

// TransactionReference FFI structures - using types from message module

#[uniffi::export]
pub fn encode_transaction_reference(
    reference: FfiTransactionReference,
) -> Result<Vec<u8>, GenericError> {
    let reference: TransactionReference = reference.into();

    let encoded = TransactionReferenceCodec::encode(reference)
        .map_err(|e| GenericError::Generic { err: e.to_string() })?;

    let mut buf = Vec::new();
    encoded
        .encode(&mut buf)
        .map_err(|e| GenericError::Generic { err: e.to_string() })?;

    Ok(buf)
}

#[uniffi::export]
pub fn decode_transaction_reference(
    bytes: Vec<u8>,
) -> Result<FfiTransactionReference, GenericError> {
    let encoded_content = EncodedContent::decode(bytes.as_slice())
        .map_err(|e| GenericError::Generic { err: e.to_string() })?;

    TransactionReferenceCodec::decode(encoded_content)
        .map(Into::into)
        .map_err(|e| GenericError::Generic { err: e.to_string() })
}

// Attachment FFI structures - using FfiAttachment from message module

#[uniffi::export]
pub fn encode_attachment(attachment: FfiAttachment) -> Result<Vec<u8>, GenericError> {
    let attachment: Attachment = attachment.into();

    let encoded = AttachmentCodec::encode(attachment)
        .map_err(|e| GenericError::Generic { err: e.to_string() })?;

    let mut buf = Vec::new();
    encoded
        .encode(&mut buf)
        .map_err(|e| GenericError::Generic { err: e.to_string() })?;

    Ok(buf)
}

#[uniffi::export]
pub fn decode_attachment(bytes: Vec<u8>) -> Result<FfiAttachment, GenericError> {
    let encoded_content = EncodedContent::decode(bytes.as_slice())
        .map_err(|e| GenericError::Generic { err: e.to_string() })?;

    AttachmentCodec::decode(encoded_content)
        .map(Into::into)
        .map_err(|e| GenericError::Generic { err: e.to_string() })
}

#[uniffi::export]
pub fn encode_reply(reply: FfiReply) -> Result<Vec<u8>, GenericError> {
    let reply: Reply = reply.into();

    let encoded =
        ReplyCodec::encode(reply).map_err(|e| GenericError::Generic { err: e.to_string() })?;

    let mut buf = Vec::new();
    encoded
        .encode(&mut buf)
        .map_err(|e| GenericError::Generic { err: e.to_string() })?;

    Ok(buf)
}

#[uniffi::export]
pub fn decode_reply(bytes: Vec<u8>) -> Result<FfiReply, GenericError> {
    let encoded_content = EncodedContent::decode(bytes.as_slice())
        .map_err(|e| GenericError::Generic { err: e.to_string() })?;

    ReplyCodec::decode(encoded_content)
        .map(Into::into)
        .map_err(|e| GenericError::Generic { err: e.to_string() })
}

// ReadReceipt FFI structures - using FfiReadReceipt from message module

#[uniffi::export]
pub fn encode_read_receipt(read_receipt: FfiReadReceipt) -> Result<Vec<u8>, GenericError> {
    let read_receipt: ReadReceipt = read_receipt.into();

    let encoded = ReadReceiptCodec::encode(read_receipt)
        .map_err(|e| GenericError::Generic { err: e.to_string() })?;

    let mut buf = Vec::new();
    encoded
        .encode(&mut buf)
        .map_err(|e| GenericError::Generic { err: e.to_string() })?;

    Ok(buf)
}

#[uniffi::export]
pub fn decode_read_receipt(bytes: Vec<u8>) -> Result<FfiReadReceipt, GenericError> {
    let encoded_content = EncodedContent::decode(bytes.as_slice())
        .map_err(|e| GenericError::Generic { err: e.to_string() })?;

    ReadReceiptCodec::decode(encoded_content)
        .map(Into::into)
        .map_err(|e| GenericError::Generic { err: e.to_string() })
}

// RemoteAttachment FFI structures - using FfiRemoteAttachment from message module

#[uniffi::export]
pub fn encode_remote_attachment(
    remote_attachment: FfiRemoteAttachment,
) -> Result<Vec<u8>, GenericError> {
    let remote_attachment: RemoteAttachment = remote_attachment.into();

    let encoded = RemoteAttachmentCodec::encode(remote_attachment)
        .map_err(|e| GenericError::Generic { err: e.to_string() })?;

    let mut buf = Vec::new();
    encoded
        .encode(&mut buf)
        .map_err(|e| GenericError::Generic { err: e.to_string() })?;

    Ok(buf)
}

#[uniffi::export]
pub fn decode_remote_attachment(bytes: Vec<u8>) -> Result<FfiRemoteAttachment, GenericError> {
    let encoded_content = EncodedContent::decode(bytes.as_slice())
        .map_err(|e| GenericError::Generic { err: e.to_string() })?;

    RemoteAttachmentCodec::decode(encoded_content)
        .map(Into::into)
        .map_err(|e| GenericError::Generic { err: e.to_string() })
}

#[derive(uniffi::Record, Clone)]
pub struct FfiMessage {
    pub id: Vec<u8>,
    pub sent_at_ns: i64,
    pub conversation_id: Vec<u8>,
    pub sender_inbox_id: String,
    pub content: Vec<u8>,
    pub kind: FfiConversationMessageKind,
    pub delivery_status: FfiDeliveryStatus,
    pub sequence_id: u64,
    pub originator_id: u32,
}

impl From<StoredGroupMessage> for FfiMessage {
    fn from(msg: StoredGroupMessage) -> Self {
        Self {
            id: msg.id,
            sent_at_ns: msg.sent_at_ns,
            conversation_id: msg.group_id,
            sender_inbox_id: msg.sender_inbox_id,
            content: msg.decrypted_message_bytes,
            kind: msg.kind.into(),
            delivery_status: msg.delivery_status.into(),
            sequence_id: msg.sequence_id as u64,
            originator_id: msg.originator_id as u32,
        }
    }
}

#[derive(uniffi::Record, Clone)]
pub struct FfiApiStats {
    pub upload_key_package: u64,
    pub fetch_key_package: u64,
    pub send_group_messages: u64,
    pub send_welcome_messages: u64,
    pub query_group_messages: u64,
    pub query_welcome_messages: u64,
    pub subscribe_messages: u64,
    pub subscribe_welcomes: u64,
}

impl From<ApiStats> for FfiApiStats {
    fn from(stats: ApiStats) -> Self {
        Self {
            upload_key_package: stats.upload_key_package.get_count() as u64,
            fetch_key_package: stats.fetch_key_package.get_count() as u64,
            send_group_messages: stats.send_group_messages.get_count() as u64,
            send_welcome_messages: stats.send_welcome_messages.get_count() as u64,
            query_group_messages: stats.query_group_messages.get_count() as u64,
            query_welcome_messages: stats.query_welcome_messages.get_count() as u64,
            subscribe_messages: stats.subscribe_messages.get_count() as u64,
            subscribe_welcomes: stats.subscribe_welcomes.get_count() as u64,
        }
    }
}

#[derive(uniffi::Record, Clone)]
pub struct FfiIdentityStats {
    pub publish_identity_update: u64,
    pub get_identity_updates_v2: u64,
    pub get_inbox_ids: u64,
    pub verify_smart_contract_wallet_signature: u64,
}

impl From<IdentityStats> for FfiIdentityStats {
    fn from(stats: IdentityStats) -> Self {
        Self {
            publish_identity_update: stats.publish_identity_update.get_count() as u64,
            get_identity_updates_v2: stats.get_identity_updates_v2.get_count() as u64,
            get_inbox_ids: stats.get_inbox_ids.get_count() as u64,
            verify_smart_contract_wallet_signature: stats
                .verify_smart_contract_wallet_signature
                .get_count() as u64,
        }
    }
}

#[derive(uniffi::Record)]
pub struct FfiConsent {
    pub entity_type: FfiConsentEntityType,
    pub state: FfiConsentState,
    pub entity: String,
}

impl From<FfiConsent> for StoredConsentRecord {
    fn from(consent: FfiConsent) -> Self {
        Self {
            entity_type: consent.entity_type.into(),
            state: consent.state.into(),
            entity: consent.entity,
            consented_at_ns: now_ns(),
        }
    }
}

type FfiHandle = Box<GenericStreamHandle<Result<(), SubscribeError>>>;

#[derive(uniffi::Object, Clone)]
pub struct FfiStreamCloser {
    stream_handle: Arc<Mutex<Option<FfiHandle>>>,
    // for convenience, does not require locking mutex.
    abort_handle: Arc<Box<dyn AbortHandle>>,
}

impl FfiStreamCloser {
    pub fn new(
        stream_handle: impl StreamHandle<StreamOutput = Result<(), SubscribeError>>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self {
            abort_handle: Arc::new(stream_handle.abort_handle()),
            stream_handle: Arc::new(Mutex::new(Some(Box::new(stream_handle)))),
        }
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl FfiStreamCloser {
    /// Signal the stream to end
    /// Does not wait for the stream to end.
    pub fn end(&self) {
        self.abort_handle.end();
    }

    /// End the stream and asynchronously wait for it to shutdown
    pub async fn end_and_wait(&self) -> Result<(), GenericError> {
        use GenericError::Generic;
        use xmtp_common::StreamHandleError::*;

        if self.abort_handle.is_finished() {
            return Ok(());
        }

        let mut stream_handle = self.stream_handle.lock().await;
        let stream_handle = stream_handle.take();
        if let Some(mut h) = stream_handle {
            match h.end_and_wait().await {
                Err(Cancelled) => Ok(()),
                Err(Panicked(msg)) => Err(Generic { err: msg }),
                Err(e) => Err(Generic {
                    err: format!("error joining task {}", e),
                }),
                Ok(t) => t.map_err(|e| Generic { err: e.to_string() }),
            }
        } else {
            log::warn!("subscription already closed");
            Ok(())
        }
    }

    pub fn is_closed(&self) -> bool {
        self.abort_handle.is_finished()
    }

    pub async fn wait_for_ready(&self) {
        let mut stream_handle = self.stream_handle.lock().await;
        if let Some(ref mut h) = *stream_handle {
            h.wait_for_ready().await;
        }
    }
}

#[uniffi::export(with_foreign)]
pub trait FfiMessageCallback: Send + Sync {
    fn on_message(&self, message: FfiMessage);
    fn on_error(&self, error: FfiSubscribeError);
    fn on_close(&self);
}

#[uniffi::export(with_foreign)]
pub trait FfiConversationCallback: Send + Sync {
    fn on_conversation(&self, conversation: Arc<FfiConversation>);
    fn on_error(&self, error: FfiSubscribeError);
    fn on_close(&self);
}

#[uniffi::export(with_foreign)]
pub trait FfiConsentCallback: Send + Sync {
    fn on_consent_update(&self, consent: Vec<FfiConsent>);
    fn on_error(&self, error: FfiSubscribeError);
    fn on_close(&self);
}

#[uniffi::export(with_foreign)]
pub trait FfiPreferenceCallback: Send + Sync {
    fn on_preference_update(&self, preference: Vec<FfiPreferenceUpdate>);
    fn on_error(&self, error: FfiSubscribeError);
    fn on_close(&self);
}

#[derive(uniffi::Enum, Debug)]
pub enum FfiPreferenceUpdate {
    HMAC { key: Vec<u8> },
}

#[derive(uniffi::Object)]
pub struct FfiConversationMetadata {
    inner: Arc<GroupMetadata>,
}

#[uniffi::export]
impl FfiConversationMetadata {
    pub fn creator_inbox_id(&self) -> String {
        self.inner.creator_inbox_id.clone()
    }

    pub fn conversation_type(&self) -> FfiConversationType {
        match self.inner.conversation_type {
            ConversationType::Group => FfiConversationType::Group,
            ConversationType::Dm => FfiConversationType::Dm,
            ConversationType::Sync => FfiConversationType::Sync,
            ConversationType::Oneshot => FfiConversationType::Oneshot,
        }
    }
}

#[derive(uniffi::Object)]
pub struct FfiGroupPermissions {
    inner: Arc<GroupMutablePermissions>,
}

#[uniffi::export]
impl FfiGroupPermissions {
    pub fn policy_type(&self) -> Result<FfiGroupPermissionsOptions, GenericError> {
        if let Ok(preconfigured_policy) = self.inner.preconfigured_policy() {
            Ok(preconfigured_policy.into())
        } else {
            Ok(FfiGroupPermissionsOptions::CustomPolicy)
        }
    }

    pub fn policy_set(&self) -> Result<FfiPermissionPolicySet, GenericError> {
        let policy_set = &self.inner.policies;
        let metadata_policy_map = &policy_set.update_metadata_policy;
        let get_policy = |field: &str| {
            metadata_policy_map
                .get(field)
                .map(FfiPermissionPolicy::from)
                .unwrap_or(FfiPermissionPolicy::DoesNotExist)
        };
        Ok(FfiPermissionPolicySet {
            add_member_policy: FfiPermissionPolicy::from(&policy_set.add_member_policy),
            remove_member_policy: FfiPermissionPolicy::from(&policy_set.remove_member_policy),
            add_admin_policy: FfiPermissionPolicy::from(&policy_set.add_admin_policy),
            remove_admin_policy: FfiPermissionPolicy::from(&policy_set.remove_admin_policy),
            update_group_name_policy: get_policy(MetadataField::GroupName.as_str()),
            update_group_description_policy: get_policy(MetadataField::Description.as_str()),
            update_group_image_url_square_policy: get_policy(
                MetadataField::GroupImageUrlSquare.as_str(),
            ),
            update_message_disappearing_policy: get_policy(
                MetadataField::MessageDisappearInNS.as_str(),
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        FfiConsentCallback, FfiMessage, FfiMessageCallback, FfiPreferenceCallback,
        FfiPreferenceUpdate, FfiXmtpClient, create_client,
    };
    use crate::{
        FfiAttachment, FfiConsent, FfiConsentEntityType, FfiConsentState, FfiContentType,
        FfiConversation, FfiConversationCallback, FfiConversationMessageKind, FfiConversationType,
        FfiCreateDMOptions, FfiCreateGroupOptions, FfiDecodedMessageBody, FfiDecodedMessageContent,
        FfiDirection, FfiGroupMessageKind, FfiGroupPermissionsOptions, FfiGroupQueryOrderBy,
        FfiListConversationsOptions, FfiListMessagesOptions, FfiMessageDisappearingSettings,
        FfiMessageWithReactions, FfiMetadataField, FfiMultiRemoteAttachment, FfiPasskeySignature,
        FfiPermissionPolicy, FfiPermissionPolicySet, FfiPermissionUpdateType, FfiReactionAction,
        FfiReactionPayload, FfiReactionSchema, FfiReadReceipt, FfiRemoteAttachment, FfiReply,
        FfiSendMessageOpts, FfiSignatureKind, FfiSubscribeError, FfiTransactionReference,
        GenericError, apply_signature_request, connect_to_backend, decode_attachment,
        decode_multi_remote_attachment, decode_reaction, decode_read_receipt,
        decode_remote_attachment, decode_reply, decode_transaction_reference, encode_attachment,
        encode_multi_remote_attachment, encode_reaction, encode_read_receipt,
        encode_remote_attachment, encode_reply, encode_transaction_reference,
        get_inbox_id_for_identifier,
        identity::{FfiIdentifier, FfiIdentifierKind},
        inbox_owner::FfiInboxOwner,
        inbox_state_from_inbox_ids, is_connected,
        message::{FfiEncodedContent, FfiRemoteAttachmentInfo, FfiTransactionMetadata},
        mls::{
            inbox_owner::FfiWalletInboxOwner,
            test_utils::{LocalBuilder, LocalTester, connect_to_backend_test},
        },
        revoke_installations,
        worker::FfiSyncWorkerMode,
    };
    use alloy::signers::local::PrivateKeySigner;
    use futures::future::join_all;
    use log::{Instrument, info_span};
    use parking_lot::Mutex;
    use prost::Message;
    use std::{
        collections::HashMap,
        sync::{
            Arc,
            atomic::{AtomicU32, Ordering},
        },
        time::Duration,
    };
    use tokio::{
        sync::{Notify, futures::OwnedNotified},
        time::error::Elapsed,
    };
    use xmtp_api::ApiClientWrapper;
    use xmtp_common::tmp_path;
    use xmtp_common::{time::now_ns, wait_for_ge};
    use xmtp_common::{wait_for_eq, wait_for_ok};
    use xmtp_configuration::MAX_INSTALLATIONS_PER_INBOX;
    use xmtp_content_types::{
        ContentCodec, attachment::AttachmentCodec, bytes_to_encoded_content,
        encoded_content_to_bytes, group_updated::GroupUpdatedCodec,
        membership_change::GroupMembershipChangeCodec, reaction::ReactionCodec,
        read_receipt::ReadReceiptCodec, remote_attachment::RemoteAttachmentCodec,
        reply::ReplyCodec, text::TextCodec, transaction_reference::TransactionReferenceCodec,
    };
    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_db::EncryptionKey;
    use xmtp_db::MlsProviderExt;
    use xmtp_db::XmtpMlsStorageProvider;
    use xmtp_db::prelude::*;
    use xmtp_id::associations::{
        MemberIdentifier, test_utils::WalletTestExt, unverified::UnverifiedSignature,
    };
    use xmtp_mls::{
        InboxOwner,
        groups::{GroupError, device_sync::worker::SyncMetric},
        utils::{PasskeyUser, Tester},
    };
    use xmtp_proto::xmtp::mls::message_contents::{
        ContentTypeId, EncodedContent,
        content_types::{ReactionAction, ReactionSchema, ReactionV2},
    };

    const HISTORY_SYNC_URL: &str = "http://localhost:5558";

    struct RustStreamCallback {
        num_messages: AtomicU32,
        messages: Mutex<Vec<FfiMessage>>,
        conversations: Mutex<Vec<Arc<FfiConversation>>>,
        consent_updates: Mutex<Vec<FfiConsent>>,
        preference_updates: Mutex<Vec<FfiPreferenceUpdate>>,
        notify: Arc<Notify>,
        inbox_id: Option<String>,
        installation_id: Option<String>,
    }

    impl Default for RustStreamCallback {
        fn default() -> Self {
            RustStreamCallback {
                num_messages: Default::default(),
                messages: Default::default(),
                conversations: Default::default(),
                consent_updates: Default::default(),
                preference_updates: Default::default(),
                notify: Arc::new(Notify::new()),
                inbox_id: None,
                installation_id: None,
            }
        }
    }

    impl RustStreamCallback {
        pub fn message_count(&self) -> u32 {
            self.num_messages.load(Ordering::SeqCst)
        }

        pub fn consent_updates_count(&self) -> usize {
            self.consent_updates.lock().len()
        }

        pub fn enable_notifications(&self) -> OwnedNotified {
            self.notify.clone().notified_owned()
        }

        pub async fn wait_for_delivery(&self, timeout_secs: Option<u64>) -> Result<(), Elapsed> {
            tokio::time::timeout(
                std::time::Duration::from_secs(timeout_secs.unwrap_or(60)),
                async { self.notify.notified().await },
            )
            .await?;
            Ok(())
        }

        pub fn from_client(client: &FfiXmtpClient) -> Self {
            RustStreamCallback {
                inbox_id: Some(client.inner_client.inbox_id().to_string()),
                installation_id: Some(hex::encode(client.inner_client.installation_public_key())),
                ..Default::default()
            }
        }
    }

    impl FfiMessageCallback for RustStreamCallback {
        fn on_message(&self, message: FfiMessage) {
            let mut messages = self.messages.lock();
            log::info!(
                inbox_id = self.inbox_id,
                installation_id = self.installation_id,
                "ON MESSAGE Received\n-------- \n{}\n----------",
                String::from_utf8_lossy(&message.content)
            );
            messages.push(message);
            let _ = self.num_messages.fetch_add(1, Ordering::SeqCst);
            self.notify.notify_one();
        }

        fn on_error(&self, error: FfiSubscribeError) {
            log::error!("{}", error)
        }

        fn on_close(&self) {
            log::error!("closed");
        }
    }

    impl FfiConversationCallback for RustStreamCallback {
        fn on_conversation(&self, group: Arc<super::FfiConversation>) {
            log::debug!(
                inbox_id = self.inbox_id,
                installation_id = self.installation_id,
                "received conversation"
            );
            let _ = self.num_messages.fetch_add(1, Ordering::SeqCst);
            let mut convos = self.conversations.lock();
            convos.push(group);
            self.notify.notify_one();
        }

        fn on_error(&self, error: FfiSubscribeError) {
            log::error!("{}", error)
        }

        fn on_close(&self) {
            log::error!("closed");
        }
    }

    impl FfiConsentCallback for RustStreamCallback {
        fn on_consent_update(&self, mut consent: Vec<FfiConsent>) {
            log::debug!(
                inbox_id = self.inbox_id,
                installation_id = self.installation_id,
                "received consent update"
            );
            let mut consent_updates = self.consent_updates.lock();
            consent_updates.append(&mut consent);
            self.notify.notify_one();
        }

        fn on_error(&self, error: FfiSubscribeError) {
            log::error!("{}", error)
        }

        fn on_close(&self) {
            log::error!("closed");
        }
    }

    impl FfiPreferenceCallback for RustStreamCallback {
        fn on_preference_update(&self, mut preference: Vec<super::FfiPreferenceUpdate>) {
            log::debug!(
                inbox_id = self.inbox_id,
                installation_id = self.installation_id,
                "\n\n=======================received consent update==============\n\n"
            );
            self.preference_updates.lock().append(&mut preference);
            self.notify.notify_one();
        }

        fn on_error(&self, error: FfiSubscribeError) {
            log::error!("{}", error)
        }

        fn on_close(&self) {
            log::error!("closed");
        }
    }

    fn static_enc_key() -> EncryptionKey {
        [2u8; 32]
    }

    async fn register_client_with_wallet(wallet: &FfiWalletInboxOwner, client: &FfiXmtpClient) {
        register_client_with_wallet_no_panic(wallet, client)
            .await
            .unwrap()
    }

    async fn register_client_with_wallet_no_panic(
        wallet: &FfiWalletInboxOwner,
        client: &FfiXmtpClient,
    ) -> Result<(), GenericError> {
        let signature_request = client.signature_request().unwrap();

        signature_request
            .add_ecdsa_signature(
                wallet
                    .sign(signature_request.signature_text().await.unwrap())
                    .unwrap(),
            )
            .await?;

        client.register_identity(signature_request).await?;

        Ok(())
    }

    /// Create a new test client with a given wallet.
    async fn new_test_client_with_wallet(wallet: PrivateKeySigner) -> Arc<FfiXmtpClient> {
        new_test_client_with_wallet_and_history_sync_url(
            wallet,
            None,
            Some(FfiSyncWorkerMode::Disabled),
        )
        .await
    }

    async fn new_test_client_with_wallet_and_history_sync_url(
        wallet: PrivateKeySigner,
        history_sync_url: Option<String>,
        sync_worker_mode: Option<FfiSyncWorkerMode>,
    ) -> Arc<FfiXmtpClient> {
        let ffi_inbox_owner = FfiWalletInboxOwner::with_wallet(wallet);
        let ident = ffi_inbox_owner.identifier();

        let nonce = 1;
        let inbox_id = ident.inbox_id(nonce).unwrap();

        let client = create_client(
            connect_to_backend_test().await,
            connect_to_backend_test().await,
            Some(tmp_path()),
            Some([0u8; 32].to_vec()),
            &inbox_id,
            ident,
            nonce,
            None,
            history_sync_url,
            sync_worker_mode,
            None,
            None,
        )
        .await
        .unwrap();

        let conn = client.inner_client.context.db();
        conn.register_triggers();

        register_client_with_wallet(&ffi_inbox_owner, &client).await;

        client
    }

    async fn new_test_client_no_panic(
        wallet: PrivateKeySigner,
        sync_server_url: Option<String>,
    ) -> Result<Arc<FfiXmtpClient>, GenericError> {
        let ffi_inbox_owner = FfiWalletInboxOwner::with_wallet(wallet);
        let ident = ffi_inbox_owner.identifier();
        let nonce = 1;
        let inbox_id = ident.inbox_id(nonce).unwrap();

        let client = create_client(
            connect_to_backend_test().await,
            connect_to_backend_test().await,
            Some(tmp_path()),
            Some(xmtp_db::EncryptedMessageStore::<()>::generate_enc_key().into()),
            &inbox_id,
            ident,
            nonce,
            None,
            sync_server_url,
            None,
            None,
            None,
        )
        .await?;

        let conn = client.inner_client.context.db();
        conn.register_triggers();

        register_client_with_wallet_no_panic(&ffi_inbox_owner, &client).await?;

        Ok(client)
    }

    async fn new_test_client() -> Arc<FfiXmtpClient> {
        let wallet = PrivateKeySigner::random();
        new_test_client_with_wallet(wallet).await
    }

    impl FfiConversation {
        #[cfg(test)]
        async fn update_installations(&self) -> Result<(), GroupError> {
            self.inner.update_installations().await?;
            Ok(())
        }
    }

    #[tokio::test]
    async fn get_inbox_id() {
        let client = new_test_client().await;
        let ident = &client.account_identifier;
        let real_inbox_id = client.inbox_id();

        let api = connect_to_backend_test().await;
        let from_network = get_inbox_id_for_identifier(api, ident.clone())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(real_inbox_id, from_network);
    }

    #[tokio::test]
    #[ignore]
    async fn test_legacy_identity() {
        let ident = FfiIdentifier {
            identifier: "0x0bD00B21aF9a2D538103c3AAf95Cb507f8AF1B28".to_lowercase(),
            identifier_kind: FfiIdentifierKind::Ethereum,
        };
        let legacy_keys = hex::decode("0880bdb7a8b3f6ede81712220a20ad528ea38ce005268c4fb13832cfed13c2b2219a378e9099e48a38a30d66ef991a96010a4c08aaa8e6f5f9311a430a41047fd90688ca39237c2899281cdf2756f9648f93767f91c0e0f74aed7e3d3a8425e9eaa9fa161341c64aa1c782d004ff37ffedc887549ead4a40f18d1179df9dff124612440a403c2cb2338fb98bfe5f6850af11f6a7e97a04350fc9d37877060f8d18e8f66de31c77b3504c93cf6a47017ea700a48625c4159e3f7e75b52ff4ea23bc13db77371001").unwrap();
        let nonce = 0;

        let inbox_id = ident.inbox_id(nonce).unwrap();

        let client = create_client(
            connect_to_backend_test().await,
            connect_to_backend_test().await,
            Some(tmp_path()),
            None,
            &inbox_id,
            ident,
            nonce,
            Some(legacy_keys),
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

        assert!(client.signature_request().is_none());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_create_client_with_storage() {
        let ffi_inbox_owner = FfiWalletInboxOwner::new();
        let ident = ffi_inbox_owner.identifier();
        let nonce = 1;
        let inbox_id = ident.inbox_id(nonce).unwrap();

        let path = tmp_path();

        let client_a = create_client(
            connect_to_backend_test().await,
            connect_to_backend_test().await,
            Some(path.clone()),
            None,
            &inbox_id,
            ffi_inbox_owner.identifier(),
            nonce,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();
        register_client_with_wallet(&ffi_inbox_owner, &client_a).await;

        let installation_pub_key = client_a.inner_client.installation_public_key().to_vec();
        drop(client_a);

        let client_b = create_client(
            connect_to_backend_test().await,
            connect_to_backend_test().await,
            Some(path),
            None,
            &inbox_id,
            ffi_inbox_owner.identifier(),
            nonce,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

        let other_installation_pub_key = client_b.inner_client.installation_public_key().to_vec();
        drop(client_b);

        assert!(
            installation_pub_key == other_installation_pub_key,
            "did not use same installation ID"
        )
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_create_client_with_key() {
        let ffi_inbox_owner = FfiWalletInboxOwner::new();
        let nonce = 1;
        let ident = ffi_inbox_owner.identifier();
        let inbox_id = ident.inbox_id(nonce).unwrap();

        let path = tmp_path();

        let key = static_enc_key().to_vec();

        let client_a = create_client(
            connect_to_backend_test().await,
            connect_to_backend_test().await,
            Some(path.clone()),
            Some(key),
            &inbox_id,
            ffi_inbox_owner.identifier(),
            nonce,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

        drop(client_a);

        let mut other_key = static_enc_key();
        other_key[31] = 1;

        let result_errored = create_client(
            connect_to_backend_test().await,
            connect_to_backend_test().await,
            Some(path),
            Some(other_key.to_vec()),
            &inbox_id,
            ffi_inbox_owner.identifier(),
            nonce,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .is_err();

        assert!(result_errored, "did not error on wrong encryption key")
    }

    trait SignWithWallet {
        async fn add_wallet_signature(&self, wallet: &PrivateKeySigner);
    }

    use super::FfiSignatureRequest;
    impl SignWithWallet for FfiSignatureRequest {
        async fn add_wallet_signature(&self, wallet: &PrivateKeySigner) {
            let signature_text = self.inner.lock().await.signature_text();

            self.inner
                .lock()
                .await
                .add_signature(wallet.sign(&signature_text).unwrap(), &self.scw_verifier)
                .await
                .unwrap();
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn create_client_does_not_hit_network() {
        let ffi_inbox_owner = FfiWalletInboxOwner::new();
        let nonce = 1;
        let ident = ffi_inbox_owner.identifier();
        let inbox_id = ident.inbox_id(nonce).unwrap();
        let path = tmp_path();
        let key = static_enc_key().to_vec();

        let connection = connect_to_backend_test().await;
        let client = create_client(
            connection.clone(),
            connect_to_backend_test().await,
            Some(path.clone()),
            Some(key.clone()),
            &inbox_id,
            ffi_inbox_owner.identifier(),
            nonce,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

        let signature_request = client.signature_request().unwrap().clone();
        register_client_with_wallet(&ffi_inbox_owner, &client).await;

        signature_request
            .add_wallet_signature(&ffi_inbox_owner.wallet)
            .await;

        tokio::time::sleep(Duration::from_secs(2)).await;

        let aggregate_str = client.api_aggregate_statistics();
        println!("Aggregate Stats Create:\n{}", aggregate_str);

        let api_stats = client.api_statistics();
        assert_eq!(api_stats.upload_key_package, 1);
        assert_eq!(api_stats.fetch_key_package, 0);

        let identity_stats = client.api_identity_statistics();
        assert_eq!(identity_stats.publish_identity_update, 1);
        assert_eq!(identity_stats.get_identity_updates_v2, 2);
        assert_eq!(identity_stats.get_inbox_ids, 1);
        assert_eq!(identity_stats.verify_smart_contract_wallet_signature, 0);

        client.clear_all_statistics();

        let build = create_client(
            connection.clone(),
            connect_to_backend_test().await,
            Some(path.clone()),
            Some(key.clone()),
            &inbox_id,
            ffi_inbox_owner.identifier(),
            nonce,
            None,
            None,
            None,
            Some(true),
            None,
        )
        .await
        .unwrap();
        tokio::time::sleep(Duration::from_secs(2)).await;

        let aggregate_str = build.api_aggregate_statistics();
        println!("Aggregate Stats Build:\n{}", aggregate_str);

        let api_stats = build.api_statistics();
        assert_eq!(api_stats.upload_key_package, 0);
        assert_eq!(api_stats.fetch_key_package, 0);

        let identity_stats = build.api_identity_statistics();
        assert_eq!(identity_stats.publish_identity_update, 0);
        assert_eq!(identity_stats.get_identity_updates_v2, 0);
        assert_eq!(identity_stats.get_inbox_ids, 0);
        assert_eq!(identity_stats.verify_smart_contract_wallet_signature, 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn ffi_api_stats_exposed_correctly() {
        let tester = Tester::new().await;
        let client: &FfiXmtpClient = &tester.client;

        let bo = Tester::new().await;
        let _conversation = client
            .conversations()
            .create_group(
                vec![bo.account_identifier.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();

        let _ = client
            .conversations()
            .list(FfiListConversationsOptions::default());

        let api_stats = client.api_statistics();
        tracing::info!(
            "api_stats.send_group_messages {}",
            api_stats.send_group_messages
        );
        assert!(api_stats.send_group_messages == 1);
        assert!(api_stats.send_welcome_messages == 1);

        let identity_stats = client.api_identity_statistics();
        assert_eq!(identity_stats.publish_identity_update, 1);
        assert!(identity_stats.get_inbox_ids >= 1);

        let aggregate_str = client.api_aggregate_statistics();
        println!("Aggregate Stats:\n{}", aggregate_str);

        assert!(aggregate_str.contains("UploadKeyPackage"));
        assert!(aggregate_str.contains("PublishIdentityUpdate"));

        client.clear_all_statistics();

        let api_stats = client.api_statistics();
        assert!(api_stats.send_group_messages == 0);
        assert!(api_stats.send_welcome_messages == 0);

        let identity_stats = client.api_identity_statistics();
        assert_eq!(identity_stats.publish_identity_update, 0);
        assert!(identity_stats.get_inbox_ids == 0);

        let aggregate_str = client.api_aggregate_statistics();
        println!("Aggregate Stats:\n{}", aggregate_str);

        let _conversation2 = client
            .conversations()
            .create_group(
                vec![bo.account_identifier.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();

        let api_stats = client.api_statistics();
        assert!(api_stats.send_group_messages == 1);
        assert!(api_stats.send_welcome_messages == 1);

        let identity_stats = client.api_identity_statistics();
        assert_eq!(identity_stats.publish_identity_update, 0);
        assert!(identity_stats.get_inbox_ids == 1);

        let aggregate_str = client.api_aggregate_statistics();
        println!("Aggregate Stats:\n{}", aggregate_str);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn radio_silence() {
        let alex = Tester::builder()
            .sync_worker()
            .sync_server()
            .stream()
            .build()
            .await;

        let convo_callback = Arc::new(RustStreamCallback::default());
        let _convo_stream_handle = alex.conversations().stream_groups(convo_callback).await;

        let worker = alex.client.inner_client.context.sync_metrics().unwrap();

        let stats = alex.inner_client.api_stats();
        let ident_stats = alex.inner_client.identity_api_stats();

        // One identity update pushed. Zero interaction with groups.
        assert_eq!(ident_stats.publish_identity_update.get_count(), 1);
        assert_eq!(stats.send_welcome_messages.get_count(), 0);
        assert_eq!(stats.send_group_messages.get_count(), 1);

        let bo = Tester::new().await;
        let conversation = alex
            .conversations()
            .create_group(
                vec![bo.account_identifier.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();
        conversation
            .send(b"Hello there".to_vec(), FfiSendMessageOpts::default())
            .await
            .unwrap();
        worker
            .register_interest(SyncMetric::ConsentSent, 1)
            .wait()
            .await
            .unwrap();

        // One identity update pushed. Zero interaction with groups.
        assert_eq!(ident_stats.publish_identity_update.get_count(), 1);
        assert_eq!(ident_stats.get_inbox_ids.get_count(), 2);
        assert_eq!(stats.send_welcome_messages.get_count(), 1);
        let group_message_count = stats.send_group_messages.get_count();

        // Sleep for 2 seconds and make sure nothing else has sent
        tokio::time::sleep(Duration::from_secs(5)).await;

        // One identity update pushed. Zero interaction with groups.
        assert_eq!(ident_stats.publish_identity_update.get_count(), 1);
        assert_eq!(ident_stats.get_inbox_ids.get_count(), 2);
        assert_eq!(stats.send_welcome_messages.get_count(), 1);
        assert_eq!(stats.send_group_messages.get_count(), group_message_count);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_can_add_wallet_to_inbox() {
        // Setup the initial first client
        let ffi_inbox_owner = FfiWalletInboxOwner::new();
        let ident = ffi_inbox_owner.identifier();
        let nonce = 1;
        let inbox_id = ident.inbox_id(nonce).unwrap();

        let path = tmp_path();
        let key = static_enc_key().to_vec();
        let client = create_client(
            connect_to_backend_test().await,
            connect_to_backend_test().await,
            Some(path.clone()),
            Some(key),
            &inbox_id,
            ffi_inbox_owner.identifier(),
            nonce,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

        let signature_request = client.signature_request().unwrap().clone();
        register_client_with_wallet(&ffi_inbox_owner, &client).await;

        signature_request
            .add_wallet_signature(&ffi_inbox_owner.wallet)
            .await;

        let conn = client.inner_client.context.store().db();
        let state = client
            .inner_client
            .identity_updates()
            .get_latest_association_state(&conn, &inbox_id)
            .await
            .expect("could not get state");

        assert_eq!(state.members().len(), 2);

        // Now, add the second wallet to the client
        let wallet_to_add = generate_local_wallet();
        let new_account_address = wallet_to_add.identifier();
        println!("second address: {}", new_account_address);

        let signature_request = client
            .add_identity(new_account_address.into())
            .await
            .expect("could not add wallet");

        signature_request.add_wallet_signature(&wallet_to_add).await;

        client
            .apply_signature_request(signature_request)
            .await
            .unwrap();

        let updated_state = client
            .inner_client
            .identity_updates()
            .get_latest_association_state(&conn, &inbox_id)
            .await
            .expect("could not get state");

        assert_eq!(updated_state.members().len(), 3);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 10)]
    async fn rapidfire_duplicate_create() {
        let wallet = generate_local_wallet();
        let mut futs = vec![];
        for _ in 0..10 {
            futs.push(new_test_client_no_panic(wallet.clone(), None));
        }

        let results = join_all(futs).await;

        let mut num_okay = 0;
        for result in results {
            if result.is_ok() {
                num_okay += 1;
            }
        }

        // Only one client should get to sign up
        assert_eq!(num_okay, 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_associate_passkey() {
        let alex = new_test_client().await;
        let passkey = PasskeyUser::new().await;

        let sig_request = alex
            .add_identity(passkey.identifier().into())
            .await
            .unwrap();
        let challenge = sig_request.signature_text().await.unwrap();
        let UnverifiedSignature::Passkey(sig) = passkey.sign(&challenge).unwrap() else {
            unreachable!("Should always be a passkey.")
        };
        sig_request
            .add_passkey_signature(FfiPasskeySignature {
                public_key: sig.public_key,
                signature: sig.signature,
                authenticator_data: sig.authenticator_data,
                client_data_json: sig.client_data_json,
            })
            .await
            .unwrap();

        alex.apply_signature_request(sig_request).await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_can_revoke_wallet() {
        // Setup the initial first client
        let ffi_inbox_owner = FfiWalletInboxOwner::new();
        let nonce = 1;
        let ident = ffi_inbox_owner.identifier();
        let inbox_id = ident.inbox_id(nonce).unwrap();
        let path = tmp_path();
        let key = static_enc_key().to_vec();

        let client = create_client(
            connect_to_backend_test().await,
            connect_to_backend_test().await,
            Some(path.clone()),
            Some(key),
            &inbox_id,
            ffi_inbox_owner.identifier(),
            nonce,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

        let signature_request = client.signature_request().unwrap().clone();
        register_client_with_wallet(&ffi_inbox_owner, &client).await;

        signature_request
            .add_wallet_signature(&ffi_inbox_owner.wallet)
            .await;

        let conn = client.inner_client.context.store().db();
        let state = client
            .inner_client
            .identity_updates()
            .get_latest_association_state(&conn, &inbox_id)
            .await
            .expect("could not get state");

        assert_eq!(state.members().len(), 2);

        // Now, add the second wallet to the client

        let wallet_to_add = generate_local_wallet();
        let new_account_address = wallet_to_add.identifier();
        println!("second address: {}", new_account_address);

        let signature_request = client
            .add_identity(new_account_address.into())
            .await
            .expect("could not add wallet");

        signature_request.add_wallet_signature(&wallet_to_add).await;

        client
            .apply_signature_request(signature_request.clone())
            .await
            .unwrap();

        let updated_state = client
            .inner_client
            .identity_updates()
            .get_latest_association_state(&conn, &inbox_id)
            .await
            .expect("could not get state");

        assert_eq!(updated_state.members().len(), 3);

        // Now, revoke the second wallet
        let signature_request = client
            .revoke_identity(wallet_to_add.identifier().into())
            .await
            .expect("could not revoke wallet");

        signature_request
            .add_wallet_signature(&ffi_inbox_owner.wallet)
            .await;

        client
            .apply_signature_request(signature_request)
            .await
            .unwrap();

        let revoked_state = client
            .inner_client
            .identity_updates()
            .get_latest_association_state(&conn, &inbox_id)
            .await
            .expect("could not get state");

        assert_eq!(revoked_state.members().len(), 2);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_invalid_external_signature() {
        let inbox_owner = FfiWalletInboxOwner::new();
        let ident = inbox_owner.identifier();
        let nonce = 1;
        let inbox_id = ident.inbox_id(nonce).unwrap();
        let path = tmp_path();

        let client = create_client(
            connect_to_backend_test().await,
            connect_to_backend_test().await,
            Some(path.clone()),
            None, // encryption_key
            &inbox_id,
            inbox_owner.identifier(),
            nonce,
            None, // v2_signed_private_key_proto
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

        let signature_request = client.signature_request().unwrap();
        assert!(client.register_identity(signature_request).await.is_err());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_can_message() {
        let amal = FfiWalletInboxOwner::new();
        let amal_ident = amal.identifier();
        let nonce = 1;
        let amal_inbox_id = amal_ident.inbox_id(nonce).unwrap();

        let bola = FfiWalletInboxOwner::new();
        let bola_ident = bola.identifier();
        let bola_inbox_id = bola_ident.inbox_id(nonce).unwrap();
        let path = tmp_path();

        let client_amal = create_client(
            connect_to_backend_test().await,
            connect_to_backend_test().await,
            Some(path.clone()),
            None,
            &amal_inbox_id,
            amal.identifier(),
            nonce,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

        // let coda = new_passkey_cred().await;
        // Check if can message a passkey identifier
        // TODO: enable when xmtp-node-go is updated
        // let can_msg = client_amal
        // .can_message(vec![coda.client.account_identifier.clone()])
        // .await
        // .unwrap();
        // let can_msg = *can_msg
        // .get(&coda.client.account_identifier)
        // .unwrap_or(&false);
        // assert!(can_msg);

        let can_message_result = client_amal
            .can_message(vec![bola.identifier()])
            .await
            .unwrap();

        assert!(
            can_message_result
                .get(&bola.identifier())
                .map(|&value| !value)
                .unwrap_or(false),
            "Expected the can_message result to be false for the address"
        );

        let client_bola = create_client(
            connect_to_backend_test().await,
            connect_to_backend_test().await,
            Some(path.clone()),
            None,
            &bola_inbox_id,
            bola.identifier(),
            nonce,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();
        register_client_with_wallet(&bola, &client_bola).await;

        let can_message_result2 = client_amal
            .can_message(vec![bola.identifier()])
            .await
            .unwrap();

        assert!(
            can_message_result2
                .get(&bola.identifier())
                .copied()
                .unwrap_or(false),
            "Expected the can_message result to be true for the address"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_create_group_with_members() {
        let amal = Tester::new().await;
        let bola = Tester::new().await;

        let group = amal
            .conversations()
            .create_group(
                vec![bola.account_identifier.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();

        let members = group.list_members().await.unwrap();
        assert_eq!(members.len(), 2);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_create_group_with_metadata() {
        let amal = new_test_client().await;
        let bola = new_test_client().await;

        let conversation_message_disappearing_settings =
            FfiMessageDisappearingSettings::new(10, 100);

        let group = amal
            .conversations()
            .create_group(
                vec![bola.account_identifier.clone()],
                FfiCreateGroupOptions {
                    permissions: Some(FfiGroupPermissionsOptions::AdminOnly),
                    group_name: Some("Group Name".to_string()),
                    group_image_url_square: Some("url".to_string()),
                    group_description: Some("group description".to_string()),
                    custom_permission_policy_set: None,
                    message_disappearing_settings: Some(
                        conversation_message_disappearing_settings.clone(),
                    ),
                },
            )
            .await
            .unwrap();

        let members = group.list_members().await.unwrap();
        assert_eq!(members.len(), 2);
        assert_eq!(group.group_name().unwrap(), "Group Name");
        assert_eq!(group.group_image_url_square().unwrap(), "url");
        assert_eq!(group.group_description().unwrap(), "group description");
        assert_eq!(
            group
                .conversation_message_disappearing_settings()
                .unwrap()
                .unwrap()
                .from_ns,
            conversation_message_disappearing_settings.clone().from_ns
        );
        assert_eq!(
            group
                .conversation_message_disappearing_settings()
                .unwrap()
                .unwrap()
                .in_ns,
            conversation_message_disappearing_settings.in_ns
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_revoke_installation_for_two_users_and_group_modification() {
        // Step 1: Create two installations
        let alix_wallet = PrivateKeySigner::random();
        let bola_wallet = PrivateKeySigner::random();
        let alix_client_1 = new_test_client_with_wallet(alix_wallet.clone()).await;
        let alix_client_2 = new_test_client_with_wallet(alix_wallet.clone()).await;
        let bola_client_1 = new_test_client_with_wallet(bola_wallet.clone()).await;

        // Ensure both clients are properly initialized
        let alix_client_1_state = alix_client_1.inbox_state(true).await.unwrap();
        let alix_client_2_state = alix_client_2.inbox_state(true).await.unwrap();
        let bola_client_1_state = bola_client_1.inbox_state(true).await.unwrap();
        assert_eq!(alix_client_1_state.installations.len(), 2);
        assert_eq!(alix_client_2_state.installations.len(), 2);
        assert_eq!(bola_client_1_state.installations.len(), 1);

        // Step 2: Create a group
        let group = alix_client_1
            .conversations()
            .create_group(
                vec![bola_client_1.account_identifier.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();

        // No ordering guarantee on members list
        let group_members = group.list_members().await.unwrap();
        assert_eq!(group_members.len(), 2);

        // identify which member is alix
        let alix_member = group_members
            .iter()
            .find(|m| m.inbox_id == alix_client_1.inbox_id())
            .unwrap();
        assert_eq!(alix_member.installation_ids.len(), 2);

        // Step 3: Revoke one installation
        let revoke_request = alix_client_1
            .revoke_installations(vec![alix_client_2.installation_id()])
            .await
            .unwrap();
        revoke_request.add_wallet_signature(&alix_wallet).await;
        alix_client_1
            .apply_signature_request(revoke_request)
            .await
            .unwrap();

        // Validate revocation
        let client_1_state_after_revoke = alix_client_1.inbox_state(true).await.unwrap();
        let _client_2_state_after_revoke = alix_client_2.inbox_state(true).await.unwrap();

        let alix_conversation_1 = alix_client_1.conversations();
        alix_conversation_1
            .sync_all_conversations(None)
            .await
            .unwrap();
        let alix_conversation_2 = alix_client_2.conversations();
        alix_conversation_2
            .sync_all_conversations(None)
            .await
            .unwrap();
        let bola_conversation_1 = bola_client_1.conversations();
        bola_conversation_1
            .sync_all_conversations(None)
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        assert_eq!(client_1_state_after_revoke.installations.len(), 1);

        // Re-fetch group members
        let group_members = group.list_members().await.unwrap();
        let alix_member = group_members
            .iter()
            .find(|m| m.inbox_id == alix_client_1.inbox_id())
            .unwrap();
        assert_eq!(alix_member.installation_ids.len(), 1);

        let alix_2_groups = alix_conversation_2
            .list(FfiListConversationsOptions::default())
            .unwrap();

        assert!(
            alix_2_groups
                .first()
                .unwrap()
                .conversation
                .update_group_name("test 2".to_string())
                .await
                .is_err()
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_revoke_installation_for_one_user_and_group_modification() {
        // Step 1: Create two installations
        let alix_wallet = PrivateKeySigner::random();
        let alix_client_1 = new_test_client_with_wallet(alix_wallet.clone()).await;
        let alix_client_2 = new_test_client_with_wallet(alix_wallet.clone()).await;

        // Ensure both clients are properly initialized
        let alix_client_1_state = alix_client_1.inbox_state(true).await.unwrap();
        let alix_client_2_state = alix_client_2.inbox_state(true).await.unwrap();
        assert_eq!(alix_client_1_state.installations.len(), 2);
        assert_eq!(alix_client_2_state.installations.len(), 2);

        // Step 2: Create a group
        let group = alix_client_1
            .conversations()
            .create_group(vec![], FfiCreateGroupOptions::default())
            .await
            .unwrap();

        // No ordering guarantee on members list
        let group_members = group.list_members().await.unwrap();
        assert_eq!(group_members.len(), 1);

        // identify which member is alix
        let alix_member = group_members
            .iter()
            .find(|m| m.inbox_id == alix_client_1.inbox_id())
            .unwrap();
        assert_eq!(alix_member.installation_ids.len(), 2);

        // Step 3: Revoke one installation
        let revoke_request = alix_client_1
            .revoke_installations(vec![alix_client_2.installation_id()])
            .await
            .unwrap();
        revoke_request.add_wallet_signature(&alix_wallet).await;
        alix_client_1
            .apply_signature_request(revoke_request)
            .await
            .unwrap();

        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        // Validate revocation
        let client_1_state_after_revoke = alix_client_1.inbox_state(true).await.unwrap();
        let _client_2_state_after_revoke = alix_client_2.inbox_state(true).await.unwrap();

        let alix_conversation_1 = alix_client_1.conversations();
        alix_conversation_1
            .sync_all_conversations(None)
            .await
            .unwrap();

        let alix_conversation_2 = alix_client_2.conversations();
        alix_conversation_2
            .sync_all_conversations(None)
            .await
            .unwrap();
        assert_eq!(client_1_state_after_revoke.installations.len(), 1);

        // Re-fetch group members
        let group_members = group.list_members().await.unwrap();
        let alix_member = group_members
            .iter()
            .find(|m| m.inbox_id == alix_client_1.inbox_id())
            .unwrap();
        assert_eq!(alix_member.installation_ids.len(), 1);

        let alix_2_groups = alix_conversation_2
            .list(FfiListConversationsOptions::default())
            .unwrap();

        assert!(
            alix_2_groups
                .first()
                .unwrap()
                .conversation
                .update_group_name("test 2".to_string())
                .await
                .is_err()
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_can_stream_group_messages_for_updates() {
        let alix = Tester::new().await;
        let bo = Tester::new().await;

        // Stream all group messages
        let message_callbacks = Arc::new(RustStreamCallback::default());
        let stream_messages = bo
            .conversations()
            .stream_all_messages(message_callbacks.clone(), None)
            .await;
        stream_messages.wait_for_ready().await;

        // Create group and send first message
        let alix_group = alix
            .conversations()
            .create_group(
                vec![bo.account_identifier.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();

        alix_group
            .update_group_name("Old Name".to_string())
            .await
            .unwrap();
        message_callbacks.wait_for_delivery(None).await.unwrap();

        let bo_groups = bo
            .conversations()
            .list(FfiListConversationsOptions::default())
            .unwrap();
        let bo_group = &bo_groups[0];
        bo_group.conversation.sync().await.unwrap();

        // alix published + processed group creation and name update
        assert_eq!(alix.client.inner_client.context.db().intents_published(), 2);
        assert_eq!(alix.client.inner_client.context.db().intents_processed(), 2);

        bo_group
            .conversation
            .update_group_name("Old Name2".to_string())
            .await
            .unwrap();
        message_callbacks.wait_for_delivery(None).await.unwrap();
        assert_eq!(bo.client.inner_client.context.db().intents_published(), 1);

        alix_group
            .send(b"Hello there".to_vec(), FfiSendMessageOpts::default())
            .await
            .unwrap();
        message_callbacks.wait_for_delivery(None).await.unwrap();
        assert_eq!(alix.client.inner_client.context.db().intents_published(), 3);

        let dm = bo
            .conversations()
            .find_or_create_dm(
                alix.account_identifier.clone(),
                FfiCreateDMOptions::default(),
            )
            .await
            .unwrap();
        message_callbacks.wait_for_delivery(None).await.unwrap();
        dm.send(b"Hello again".to_vec(), FfiSendMessageOpts::default())
            .await
            .unwrap();
        assert_eq!(bo.client.inner_client.context.db().intents_published(), 3);
        message_callbacks.wait_for_delivery(None).await.unwrap();

        // Uncomment the following lines to add more group name updates
        bo_group
            .conversation
            .update_group_name("Old Name3".to_string())
            .await
            .unwrap();
        message_callbacks.wait_for_delivery(None).await.unwrap();
        assert_eq!(bo.client.inner_client.context.db().intents_published(), 4);

        wait_for_eq(|| async { message_callbacks.message_count() }, 6)
            .await
            .unwrap();

        stream_messages.end_and_wait().await.unwrap();
        assert!(stream_messages.is_closed());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_conversation_debug_info_returns_correct_values() {
        // Step 1: Setup test client Alix and bo
        let alix = new_test_client().await;
        let bo = new_test_client().await;

        // Step 2: Create a group and add messages
        let alix_conversations = alix.conversations();

        // Create a group
        let group = alix_conversations
            .create_group(
                vec![bo.account_identifier.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();

        let debug_info = group.inner.debug_info().await.unwrap();
        // Ensure the group is included
        assert_eq!(debug_info.epoch, 1, "Group epoch should be 1");
        assert!(!debug_info.maybe_forked, "Group is not marked as forked");
        assert!(
            debug_info.fork_details.is_empty(),
            "Group has no fork details"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_list_conversations_last_message() {
        // Step 1: Setup test client Alix and bo
        let alix = new_test_client().await;
        let bo = new_test_client().await;

        // Step 2: Create a group and add messages
        let alix_conversations = alix.conversations();

        // Create a group
        let group = alix_conversations
            .create_group(
                vec![bo.account_identifier.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();

        // Add messages to the group
        let text_message_1 = TextCodec::encode("Text message for Group 1".to_string()).unwrap();
        group
            .send(
                encoded_content_to_bytes(text_message_1),
                FfiSendMessageOpts::default(),
            )
            .await
            .unwrap();
        let text_message_2 = TextCodec::encode("Text message for Group 2".to_string()).unwrap();
        group
            .send(
                encoded_content_to_bytes(text_message_2),
                FfiSendMessageOpts::default(),
            )
            .await
            .unwrap();

        // Step 3: Synchronize conversations
        alix_conversations
            .sync_all_conversations(None)
            .await
            .unwrap();

        // Step 4: List conversations and verify
        let conversations = alix_conversations
            .list(FfiListConversationsOptions::default())
            .unwrap();

        // Ensure the group is included
        assert_eq!(conversations.len(), 1, "Alix should have exactly 1 group");

        let last_message = conversations[0].last_message.as_ref().unwrap();
        assert_eq!(
            TextCodec::decode(bytes_to_encoded_content(last_message.content.clone())).unwrap(),
            "Text message for Group 2".to_string(),
            "Last message content should be the most recent"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_list_conversations_no_messages() {
        // Step 1: Setup test clients Alix and Bo
        let alix = new_test_client().await;
        let bo = new_test_client().await;

        let alix_conversations = alix.conversations();

        // Step 2: Create a group with Bo but do not send messages
        alix_conversations
            .create_group(
                vec![bo.account_identifier.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();

        // Step 3: Synchronize conversations
        alix_conversations
            .sync_all_conversations(None)
            .await
            .unwrap();

        // Step 4: List conversations and verify
        let conversations = alix_conversations
            .list(FfiListConversationsOptions::default())
            .unwrap();

        // Ensure the group is included
        assert_eq!(conversations.len(), 1, "Alix should have exactly 1 group");

        // Verify that the last_message is None
        assert!(
            conversations[0].last_message.is_none(),
            "Last message should be None since no messages were sent"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_conversation_list_filters_readable_messages() {
        // Step 1: Setup test client
        let client = new_test_client().await;
        let conversations_api = client.conversations();

        // Step 2: Create 9 groups
        let mut groups = Vec::with_capacity(9);
        for _ in 0..9 {
            let group = conversations_api
                .create_group(vec![], FfiCreateGroupOptions::default())
                .await
                .unwrap();
            groups.push(group);
        }

        // Step 3: Each group gets a message sent in it by type following the pattern:
        //   group[0] -> TextCodec                    (readable)
        //   group[1] -> ReactionCodec                (readable)
        //   group[2] -> AttachmentCodec              (readable)
        //   group[3] -> RemoteAttachmentCodec        (readable)
        //   group[4] -> ReplyCodec                   (readable)
        //   group[5] -> TransactionReferenceCodec    (readable)
        //   group[6] -> GroupUpdatedCodec            (not readable)
        //   group[7] -> GroupMembershipUpdatedCodec  (not readable)
        //   group[8] -> ReadReceiptCodec             (not readable)

        // group[0] sends TextCodec message
        let text_message = TextCodec::encode("Text message for Group 1".to_string()).unwrap();
        groups[0]
            .send(
                encoded_content_to_bytes(text_message),
                FfiSendMessageOpts::default(),
            )
            .await
            .unwrap();

        // group[1] sends ReactionCodec message
        let reaction_content_type_id = ContentTypeId {
            authority_id: "".to_string(),
            type_id: ReactionCodec::TYPE_ID.to_string(),
            version_major: 0,
            version_minor: 0,
        };
        let reaction_encoded_content = EncodedContent {
            r#type: Some(reaction_content_type_id),
            content: "reaction content".as_bytes().to_vec(),
            parameters: HashMap::new(),
            fallback: None,
            compression: None,
        };
        groups[1]
            .send(
                encoded_content_to_bytes(reaction_encoded_content),
                FfiSendMessageOpts::default(),
            )
            .await
            .unwrap();

        // group[2] sends AttachmentCodec message
        let attachment_content_type_id = ContentTypeId {
            authority_id: "".to_string(),
            type_id: AttachmentCodec::TYPE_ID.to_string(),
            version_major: 0,
            version_minor: 0,
        };
        let attachment_encoded_content = EncodedContent {
            r#type: Some(attachment_content_type_id),
            content: "attachment content".as_bytes().to_vec(),
            parameters: HashMap::new(),
            fallback: None,
            compression: None,
        };
        groups[2]
            .send(
                encoded_content_to_bytes(attachment_encoded_content),
                FfiSendMessageOpts::default(),
            )
            .await
            .unwrap();

        // group[3] sends RemoteAttachmentCodec message
        let remote_attachment_content_type_id = ContentTypeId {
            authority_id: "".to_string(),
            type_id: RemoteAttachmentCodec::TYPE_ID.to_string(),
            version_major: 0,
            version_minor: 0,
        };
        let remote_attachment_encoded_content = EncodedContent {
            r#type: Some(remote_attachment_content_type_id),
            content: "remote attachment content".as_bytes().to_vec(),
            parameters: HashMap::new(),
            fallback: None,
            compression: None,
        };
        groups[3]
            .send(
                encoded_content_to_bytes(remote_attachment_encoded_content),
                FfiSendMessageOpts::default(),
            )
            .await
            .unwrap();

        // group[4] sends ReplyCodec message
        let reply_content_type_id = ContentTypeId {
            authority_id: "".to_string(),
            type_id: ReplyCodec::TYPE_ID.to_string(),
            version_major: 0,
            version_minor: 0,
        };
        let reply_encoded_content = EncodedContent {
            r#type: Some(reply_content_type_id),
            content: "reply content".as_bytes().to_vec(),
            parameters: HashMap::new(),
            fallback: None,
            compression: None,
        };
        groups[4]
            .send(
                encoded_content_to_bytes(reply_encoded_content),
                FfiSendMessageOpts::default(),
            )
            .await
            .unwrap();

        // group[5] sends TransactionReferenceCodec message
        let transaction_reference_content_type_id = ContentTypeId {
            authority_id: "".to_string(),
            type_id: TransactionReferenceCodec::TYPE_ID.to_string(),
            version_major: 0,
            version_minor: 0,
        };
        let transaction_reference_encoded_content = EncodedContent {
            r#type: Some(transaction_reference_content_type_id),
            content: "transaction reference".as_bytes().to_vec(),
            parameters: HashMap::new(),
            fallback: None,
            compression: None,
        };
        groups[5]
            .send(
                encoded_content_to_bytes(transaction_reference_encoded_content),
                FfiSendMessageOpts::default(),
            )
            .await
            .unwrap();

        // group[6] sends GroupUpdatedCodec message
        let group_updated_content_type_id = ContentTypeId {
            authority_id: "".to_string(),
            type_id: GroupUpdatedCodec::TYPE_ID.to_string(),
            version_major: 0,
            version_minor: 0,
        };
        let group_updated_encoded_content = EncodedContent {
            r#type: Some(group_updated_content_type_id),
            content: "group updated content".as_bytes().to_vec(),
            parameters: HashMap::new(),
            fallback: None,
            compression: None,
        };
        groups[6]
            .send(
                encoded_content_to_bytes(group_updated_encoded_content),
                FfiSendMessageOpts::default(),
            )
            .await
            .unwrap();

        // group[7] sends GroupMembershipUpdatedCodec message
        let group_membership_updated_content_type_id = ContentTypeId {
            authority_id: "".to_string(),
            type_id: GroupMembershipChangeCodec::TYPE_ID.to_string(),
            version_major: 0,
            version_minor: 0,
        };
        let group_membership_updated_encoded_content = EncodedContent {
            r#type: Some(group_membership_updated_content_type_id),
            content: "group membership updated".as_bytes().to_vec(),
            parameters: HashMap::new(),
            fallback: None,
            compression: None,
        };
        groups[7]
            .send(
                encoded_content_to_bytes(group_membership_updated_encoded_content),
                FfiSendMessageOpts::default(),
            )
            .await
            .unwrap();

        // group[8] sends ReadReceiptCodec message
        let read_receipt_content_type_id = ContentTypeId {
            authority_id: "".to_string(),
            type_id: ReadReceiptCodec::TYPE_ID.to_string(),
            version_major: 0,
            version_minor: 0,
        };
        let read_receipt_encoded_content = EncodedContent {
            r#type: Some(read_receipt_content_type_id),
            content: "read receipt content".as_bytes().to_vec(),
            parameters: HashMap::new(),
            fallback: None,
            compression: None,
        };
        groups[8]
            .send(
                encoded_content_to_bytes(read_receipt_encoded_content),
                FfiSendMessageOpts::default(),
            )
            .await
            .unwrap();

        // Step 4: Synchronize all conversations
        conversations_api
            .sync_all_conversations(None)
            .await
            .unwrap();

        // Step 5: Fetch the list of conversations
        let conversations = conversations_api
            .list(FfiListConversationsOptions::default())
            .unwrap();

        // Step 6: Verify the order of conversations by last readable message sent (or recently created if no readable message)
        // The order should be: 5, 4, 3, 2, 1, 0, 8, 7, 6
        assert_eq!(
            conversations.len(),
            9,
            "There should be exactly 9 conversations"
        );

        assert_eq!(
            conversations[0].conversation.inner.group_id, groups[5].inner.group_id,
            "Group 6 should be the first conversation"
        );
        assert_eq!(
            conversations[1].conversation.inner.group_id, groups[4].inner.group_id,
            "Group 5 should be the second conversation"
        );
        assert_eq!(
            conversations[2].conversation.inner.group_id, groups[3].inner.group_id,
            "Group 4 should be the third conversation"
        );
        assert_eq!(
            conversations[3].conversation.inner.group_id, groups[2].inner.group_id,
            "Group 3 should be the fourth conversation"
        );
        assert_eq!(
            conversations[4].conversation.inner.group_id, groups[1].inner.group_id,
            "Group 2 should be the fifth conversation"
        );
        assert_eq!(
            conversations[5].conversation.inner.group_id, groups[0].inner.group_id,
            "Group 1 should be the sixth conversation"
        );
        assert_eq!(
            conversations[6].conversation.inner.group_id, groups[8].inner.group_id,
            "Group 9 should be the seventh conversation"
        );
        assert_eq!(
            conversations[7].conversation.inner.group_id, groups[7].inner.group_id,
            "Group 8 should be the eighth conversation"
        );
        assert_eq!(
            conversations[8].conversation.inner.group_id, groups[6].inner.group_id,
            "Group 7 should be the ninth conversation"
        );

        // Step 7: Verify that for conversations 0 through 5, last_message is Some
        // Index of group[0] in conversations -> 5
        for i in 0..=5 {
            assert!(
                conversations[5 - i].last_message.is_some(),
                "Group {} should have a last message",
                i + 1
            );
        }

        // Step 8: Verify that for conversations 6, 7, 8, last_message is None
        #[allow(clippy::needless_range_loop)]
        for i in 6..=8 {
            assert!(
                conversations[i].last_message.is_none(),
                "Group {} should have no last message",
                i + 1
            );
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_can_sync_all_groups() {
        let alix = new_test_client().await;
        let bo = new_test_client().await;

        for _i in 0..30 {
            alix.conversations()
                .create_group(
                    vec![bo.account_identifier.clone()],
                    FfiCreateGroupOptions::default(),
                )
                .await
                .unwrap();
        }

        bo.conversations()
            .sync_all_conversations(None)
            .await
            .unwrap();
        let alix_groups = alix
            .conversations()
            .list(FfiListConversationsOptions::default())
            .unwrap();

        if cfg!(feature = "d14n") {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }

        let alix_group1 = alix_groups[0].clone();
        let alix_group5 = alix_groups[5].clone();
        let bo_group1 = bo.conversation(alix_group1.conversation.id()).unwrap();
        let bo_group5 = bo.conversation(alix_group5.conversation.id()).unwrap();

        alix_group1
            .conversation
            .send("alix1".as_bytes().to_vec(), FfiSendMessageOpts::default())
            .await
            .unwrap();
        alix_group5
            .conversation
            .send("alix1".as_bytes().to_vec(), FfiSendMessageOpts::default())
            .await
            .unwrap();

        let bo_messages1 = bo_group1
            .find_messages(FfiListMessagesOptions::default())
            .await
            .unwrap();
        let bo_messages5 = bo_group5
            .find_messages(FfiListMessagesOptions::default())
            .await
            .unwrap();
        assert_eq!(bo_messages1.len(), 1);
        assert_eq!(bo_messages5.len(), 1);

        bo.conversations()
            .sync_all_conversations(None)
            .await
            .unwrap();

        let bo_messages1 = bo_group1
            .find_messages(FfiListMessagesOptions::default())
            .await
            .unwrap();
        let bo_messages5 = bo_group5
            .find_messages(FfiListMessagesOptions::default())
            .await
            .unwrap();
        assert_eq!(bo_messages1.len(), 2);
        assert_eq!(bo_messages5.len(), 2);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_can_sync_all_groups_active_only() {
        let alix = new_test_client().await;
        let bo = new_test_client().await;

        // Create 30 groups with alix and bo and sync them
        for _i in 0..30 {
            alix.conversations()
                .create_group(
                    vec![bo.account_identifier.clone()],
                    FfiCreateGroupOptions::default(),
                )
                .await
                .unwrap();
        }
        bo.conversations().sync().await.unwrap();
        let num_groups_synced_1: u32 = bo
            .conversations()
            .sync_all_conversations(None)
            .await
            .unwrap();
        assert_eq!(num_groups_synced_1, 30);

        // Remove bo from all groups and sync
        for group in alix
            .conversations()
            .list(FfiListConversationsOptions::default())
            .unwrap()
        {
            group
                .conversation
                .remove_members(vec![bo.account_identifier.clone()])
                .await
                .unwrap();
        }

        // First sync after removal needs to process all groups and set them to inactive
        let num_groups_synced_2: u32 = bo
            .conversations()
            .sync_all_conversations(None)
            .await
            .unwrap();
        assert_eq!(num_groups_synced_2, 30);

        // Second sync after removal will not process inactive groups
        let num_groups_synced_3: u32 = bo
            .conversations()
            .sync_all_conversations(None)
            .await
            .unwrap();
        assert_eq!(num_groups_synced_3, 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_can_send_message_when_out_of_sync() {
        let alix = new_test_client().await;
        let bo = new_test_client().await;
        let caro = new_test_client().await;
        let davon = new_test_client().await;
        let eri = new_test_client().await;
        let frankie = new_test_client().await;

        let alix_group = alix
            .conversations()
            .create_group(
                vec![bo.account_identifier.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();

        bo.conversations().sync().await.unwrap();
        let bo_group = bo.conversation(alix_group.id()).unwrap();

        bo_group
            .send("bo1".as_bytes().to_vec(), FfiSendMessageOpts::default())
            .await
            .unwrap();
        // Temporary workaround for OpenMLS issue - make sure Alix's epoch is up-to-date
        // https://github.com/xmtp/libxmtp/issues/1116
        alix_group.sync().await.unwrap();
        alix_group
            .send("alix1".as_bytes().to_vec(), FfiSendMessageOpts::default())
            .await
            .unwrap();

        // Move the group forward by 3 epochs (as Alix's max_past_epochs is
        // configured to 3) without Bo syncing
        alix_group
            .add_members(vec![
                caro.account_identifier.clone(),
                davon.account_identifier.clone(),
            ])
            .await
            .unwrap();
        alix_group
            .remove_members(vec![
                caro.account_identifier.clone(),
                davon.account_identifier.clone(),
            ])
            .await
            .unwrap();
        alix_group
            .add_members(vec![
                eri.account_identifier.clone(),
                frankie.account_identifier.clone(),
            ])
            .await
            .unwrap();

        // Bo sends messages to Alix while 3 epochs behind
        bo_group
            .send("bo3".as_bytes().to_vec(), FfiSendMessageOpts::default())
            .await
            .unwrap();
        alix_group
            .send("alix3".as_bytes().to_vec(), FfiSendMessageOpts::default())
            .await
            .unwrap();
        bo_group
            .send("bo4".as_bytes().to_vec(), FfiSendMessageOpts::default())
            .await
            .unwrap();
        bo_group
            .send("bo5".as_bytes().to_vec(), FfiSendMessageOpts::default())
            .await
            .unwrap();

        alix_group.sync().await.unwrap();
        let alix_messages = alix_group
            .find_messages(FfiListMessagesOptions::default())
            .await
            .unwrap();

        bo_group.sync().await.unwrap();
        let bo_messages = bo_group
            .find_messages(FfiListMessagesOptions::default())
            .await
            .unwrap();
        assert_eq!(bo_messages.len(), 10);
        assert_eq!(alix_messages.len(), 10);

        assert_eq!(
            bo_messages[bo_messages.len() - 1].id,
            alix_messages[alix_messages.len() - 1].id
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_create_new_installation_without_breaking_group() {
        let wallet1 = PrivateKeySigner::random();
        let wallet2 = PrivateKeySigner::random();

        // Create clients
        let client1 = new_test_client_with_wallet(wallet1).await;
        let client2 = new_test_client_with_wallet(wallet2.clone()).await;
        // Create a new group with client1 including wallet2

        let group = client1
            .conversations()
            .create_group(
                vec![client2.account_identifier.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();

        // Sync groups
        client1.conversations().sync().await.unwrap();
        client2.conversations().sync().await.unwrap();

        // Find groups for both clients
        let client1_group = client1.conversation(group.id()).unwrap();
        let client2_group = client2.conversation(group.id()).unwrap();

        // Sync both groups
        client1_group.sync().await.unwrap();
        client2_group.sync().await.unwrap();

        // Assert both clients see 2 members
        let client1_members = client1_group.list_members().await.unwrap();
        assert_eq!(client1_members.len(), 2);

        let client2_members = client2_group.list_members().await.unwrap();
        assert_eq!(client2_members.len(), 2);

        // Drop and delete local database for client2
        client2.release_db_connection().unwrap();

        // Recreate client2 (new installation)
        let client2 = new_test_client_with_wallet(wallet2).await;

        client1_group.update_installations().await.unwrap();

        // Send a message that will break the group
        client1_group
            .send(
                "This message will break the group".as_bytes().to_vec(),
                FfiSendMessageOpts::default(),
            )
            .await
            .unwrap();

        // Assert client1 still sees 2 members
        let client1_members = client1_group.list_members().await.unwrap();
        assert_eq!(client1_members.len(), 2);

        client2.conversations().sync().await.unwrap();
        let client2_group = client2.conversation(group.id()).unwrap();
        let client2_members = client2_group.list_members().await.unwrap();
        assert_eq!(client2_members.len(), 2);
    }

    // ... existing code ...

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_create_new_installation_can_see_dm() {
        // Create two wallets
        let wallet1 = PrivateKeySigner::random();
        let wallet2 = PrivateKeySigner::random();

        // Create initial clients
        let client1 = new_test_client_with_wallet(wallet1.clone()).await;
        let client2 = new_test_client_with_wallet(wallet2).await;

        // Create DM from client1 to client2
        let dm_group = client1
            .conversations()
            .find_or_create_dm(
                client2.account_identifier.clone(),
                FfiCreateDMOptions::default(),
            )
            .await
            .unwrap();

        // Sync both clients
        client1.conversations().sync().await.unwrap();
        client2.conversations().sync().await.unwrap();

        // Verify both clients can see the DM
        let client1_groups = client1
            .conversations()
            .list_dms(FfiListConversationsOptions::default())
            .unwrap();
        let client2_groups = client2
            .conversations()
            .list_dms(FfiListConversationsOptions::default())
            .unwrap();
        assert_eq!(client1_groups.len(), 1, "Client1 should see 1 conversation");
        assert_eq!(client2_groups.len(), 1, "Client2 should see 1 conversation");

        // Create a second client1 with same wallet
        let client1_second = new_test_client_with_wallet(wallet1).await;

        // Verify client1_second starts with no conversations
        let initial_conversations = client1_second
            .conversations()
            .list(FfiListConversationsOptions::default())
            .unwrap();
        assert_eq!(
            initial_conversations.len(),
            0,
            "New client should start with no conversations"
        );

        // Send message from client1 to client2
        dm_group
            .send(
                "Hello from client1".as_bytes().to_vec(),
                FfiSendMessageOpts::default(),
            )
            .await
            .unwrap();

        // Sync all clients
        client1.conversations().sync().await.unwrap();
        // client2.conversations().sync().await.unwrap();

        tracing::info!(
            "ABOUT TO SYNC CLIENT 1 SECOND: {}",
            client1_second.inbox_id().to_string()
        );
        client1_second.conversations().sync().await.unwrap();

        // Verify second client1 can see the DM
        let client1_second_groups = client1_second
            .conversations()
            .list_dms(FfiListConversationsOptions::default())
            .unwrap();
        assert_eq!(
            client1_second_groups.len(),
            1,
            "Second client1 should see 1 conversation"
        );
        assert_eq!(
            client1_second_groups[0].conversation.id(),
            dm_group.id(),
            "Second client1's conversation should match original DM"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_create_new_installations_does_not_fork_group() {
        let bo_wallet = PrivateKeySigner::random();

        // Create clients
        let alix = new_test_client().await;
        let bo = new_test_client_with_wallet(bo_wallet.clone()).await;
        let caro = new_test_client().await;

        // Alix begins a stream for all messages
        let message_callbacks = Arc::new(RustStreamCallback::from_client(&alix));
        let stream_messages = alix
            .conversations()
            .stream_all_messages(message_callbacks.clone(), None)
            .await;
        stream_messages.wait_for_ready().await;

        // Alix creates a group with Bo and Caro
        let group = alix
            .conversations()
            .create_group(
                vec![
                    bo.account_identifier.clone(),
                    caro.account_identifier.clone(),
                ],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();

        // Alix and Caro Sync groups
        alix.conversations().sync().await.unwrap();
        bo.conversations().sync().await.unwrap();
        caro.conversations().sync().await.unwrap();

        // Alix and Caro find the group
        let alix_group = alix.conversation(group.id()).unwrap();
        let bo_group = bo.conversation(group.id()).unwrap();
        let caro_group = caro.conversation(group.id()).unwrap();

        alix_group.update_installations().await.unwrap();
        log::info!("Alix sending first message");
        // Alix sends a message in the group
        alix_group
            .send(
                "First message".as_bytes().to_vec(),
                FfiSendMessageOpts::default(),
            )
            .await
            .unwrap();

        log::info!("Caro sending second message");
        caro_group.update_installations().await.unwrap();
        // Caro sends a message in the group
        caro_group
            .send(
                "Second message".as_bytes().to_vec(),
                FfiSendMessageOpts::default(),
            )
            .await
            .unwrap();

        // Wait for 2 seconds to make sure message does not get streamed to Bo's new installation
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        // Bo logs back in with a new installation
        let bo2 = new_test_client_with_wallet(bo_wallet).await;

        // Bo begins a stream for all messages
        let bo2_message_callbacks = Arc::new(RustStreamCallback::from_client(&bo2));
        let bo2_stream_messages = bo2
            .conversations()
            .stream_all_messages(bo2_message_callbacks.clone(), None)
            .await;
        bo2_stream_messages.wait_for_ready().await;

        alix_group.update_installations().await.unwrap();

        log::info!("Alix sending third message after Bo's second installation added");
        // Alix sends a message to the group
        alix_group
            .send(
                "Third message".as_bytes().to_vec(),
                FfiSendMessageOpts::default(),
            )
            .await
            .unwrap();

        // New installation of bo finds the group
        bo2.conversations().sync().await.unwrap();
        let bo2_group = bo2.conversation(group.id()).unwrap();

        log::info!("Bo sending fourth message");
        // Bo sends a message to the group
        bo2_group.update_installations().await.unwrap();
        bo2_group
            .send(
                "Fourth message".as_bytes().to_vec(),
                FfiSendMessageOpts::default(),
            )
            .await
            .unwrap();

        log::info!("Caro sending fifth message");
        // Caro sends a message in the group
        caro_group.update_installations().await.unwrap();
        // Temporary workaround for OpenMLS issue - make sure Caro's epoch is up-to-date
        // https://github.com/xmtp/libxmtp/issues/1116
        caro_group.sync().await.unwrap();
        caro_group
            .send(
                "Fifth message".as_bytes().to_vec(),
                FfiSendMessageOpts::default(),
            )
            .await
            .unwrap();

        log::info!("Syncing alix");
        alix_group.sync().await.unwrap();
        log::info!("Syncing bo 1");
        bo_group.sync().await.unwrap();
        log::info!("Syncing bo 2");
        bo2_group.sync().await.unwrap();
        log::info!("Syncing caro");
        caro_group.sync().await.unwrap();

        // Get the message count for all the clients
        let caro_messages = caro_group
            .find_messages(FfiListMessagesOptions::default())
            .await
            .unwrap();
        let alix_messages = alix_group
            .find_messages(FfiListMessagesOptions::default())
            .await
            .unwrap();
        let bo_messages = bo_group
            .find_messages(FfiListMessagesOptions::default())
            .await
            .unwrap();
        let bo2_messages = bo2_group
            .find_messages(FfiListMessagesOptions::default())
            .await
            .unwrap();

        assert_eq!(caro_messages.len(), 6);
        assert_eq!(alix_messages.len(), 6);
        assert_eq!(bo_messages.len(), 6);
        // Bo 2 only sees three messages since it joined after the first 2 were sent + plus the groupUpdatedCodec
        assert_eq!(bo2_messages.len(), 4);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_can_send_messages_when_epochs_behind() {
        let alix = new_test_client().await;
        let bo = new_test_client().await;

        let alix_group = alix
            .conversations()
            .create_group(
                vec![bo.account_identifier.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();

        bo.conversations().sync().await.unwrap();

        let bo_group = bo.conversation(alix_group.id()).unwrap();

        // Move forward 4 epochs
        alix_group
            .update_group_description("change 1".to_string())
            .await
            .unwrap();
        alix_group
            .update_group_description("change 2".to_string())
            .await
            .unwrap();
        alix_group
            .update_group_description("change 3".to_string())
            .await
            .unwrap();
        alix_group
            .update_group_description("change 4".to_string())
            .await
            .unwrap();

        bo_group
            .send(
                "bo message 1".as_bytes().to_vec(),
                FfiSendMessageOpts::default(),
            )
            .await
            .unwrap();

        alix_group.sync().await.unwrap();
        bo_group.sync().await.unwrap();

        let alix_messages = alix_group
            .find_messages(FfiListMessagesOptions::default())
            .await
            .unwrap();
        let bo_messages = bo_group
            .find_messages(FfiListMessagesOptions::default())
            .await
            .unwrap();

        let alix_can_see_bo_message = alix_messages
            .iter()
            .any(|message| message.content == "bo message 1".as_bytes());
        assert!(
            alix_can_see_bo_message,
            "\"bo message 1\" not found in alix's messages"
        );

        let bo_can_see_bo_message = bo_messages
            .iter()
            .any(|message| message.content == "bo message 1".as_bytes());
        assert!(
            bo_can_see_bo_message,
            "\"bo message 1\" not found in bo's messages"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_can_add_members_when_out_of_sync() {
        let alix = new_test_client().await;
        let bo = new_test_client().await;
        let caro = new_test_client().await;
        let davon = new_test_client().await;
        let eri = new_test_client().await;
        let frankie = new_test_client().await;

        let alix_group = alix
            .conversations()
            .create_group(
                vec![bo.account_identifier.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();

        bo.conversations().sync().await.unwrap();
        let bo_group = bo.conversation(alix_group.id()).unwrap();

        bo_group
            .send("bo1".as_bytes().to_vec(), FfiSendMessageOpts::default())
            .await
            .unwrap();
        alix_group
            .send("alix1".as_bytes().to_vec(), FfiSendMessageOpts::default())
            .await
            .unwrap();

        // Move the group forward by 3 epochs (as Alix's max_past_epochs is
        // configured to 3) without Bo syncing
        alix_group
            .add_members(vec![
                caro.account_identifier.clone(),
                davon.account_identifier.clone(),
            ])
            .await
            .unwrap();
        alix_group
            .remove_members(vec![
                caro.account_identifier.clone(),
                davon.account_identifier.clone(),
            ])
            .await
            .unwrap();
        alix_group
            .add_members(vec![eri.account_identifier.clone()])
            .await
            .unwrap();

        // Bo adds a member while 3 epochs behind
        bo_group
            .add_members(vec![frankie.account_identifier.clone()])
            .await
            .unwrap();

        bo_group.sync().await.unwrap();
        let bo_members = bo_group.list_members().await.unwrap();
        assert_eq!(bo_members.len(), 4);

        alix_group.sync().await.unwrap();
        let alix_members = alix_group.list_members().await.unwrap();
        assert_eq!(alix_members.len(), 4);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_removed_members_no_longer_update() {
        let alix = new_test_client().await;
        let bo = new_test_client().await;

        let alix_group = alix
            .conversations()
            .create_group(
                vec![bo.account_identifier.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();

        bo.conversations().sync().await.unwrap();
        let bo_group = bo.conversation(alix_group.id()).unwrap();

        alix_group.sync().await.unwrap();
        let alix_members = alix_group.list_members().await.unwrap();
        assert_eq!(alix_members.len(), 2);

        bo_group.sync().await.unwrap();
        let bo_members = bo_group.list_members().await.unwrap();
        assert_eq!(bo_members.len(), 2);

        let bo_messages = bo_group
            .find_messages(FfiListMessagesOptions::default())
            .await
            .unwrap();
        assert_eq!(bo_messages.len(), 1);

        alix_group
            .remove_members(vec![bo.account_identifier.clone()])
            .await
            .unwrap();

        alix_group
            .send("hello".as_bytes().to_vec(), FfiSendMessageOpts::default())
            .await
            .unwrap();

        bo_group.sync().await.unwrap();
        assert!(!bo_group.is_active().unwrap());

        let bo_messages = bo_group
            .find_messages(FfiListMessagesOptions::default())
            .await
            .unwrap();
        assert_eq!(
            bo_messages.first().unwrap().kind,
            FfiConversationMessageKind::MembershipChange
        );
        assert_eq!(bo_messages.len(), 2);

        let bo_members = bo_group.list_members().await.unwrap();
        assert_eq!(bo_members.len(), 1);

        alix_group.sync().await.unwrap();
        let alix_members = alix_group.list_members().await.unwrap();
        assert_eq!(alix_members.len(), 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_can_stream_and_update_name_without_forking_group() {
        let alix = new_test_client().await;
        let bo = new_test_client().await;

        // Stream all group messages
        let message_callbacks = Arc::new(RustStreamCallback::default());
        let stream_messages = bo
            .conversations()
            .stream_all_messages(message_callbacks.clone(), None)
            .await;
        stream_messages.wait_for_ready().await;

        let first_msg_check = 2;
        let second_msg_check = 5;

        // Create group and send first message
        let alix_group = alix
            .conversations()
            .create_group(
                vec![bo.account_identifier.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();
        if cfg!(feature = "d14n") {
            // give time for d14n to catch up
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
        alix_group
            .update_group_name("hello".to_string())
            .await
            .unwrap();
        message_callbacks.wait_for_delivery(None).await.unwrap();
        alix_group
            .send("hello1".as_bytes().to_vec(), FfiSendMessageOpts::default())
            .await
            .unwrap();
        message_callbacks.wait_for_delivery(None).await.unwrap();

        let bo_groups = bo
            .conversations()
            .list(FfiListConversationsOptions::default())
            .unwrap();
        assert_eq!(bo_groups.len(), 1);
        let bo_group = bo_groups[0].clone();
        bo_group.conversation.sync().await.unwrap();

        let bo_messages1 = bo_group
            .conversation
            .find_messages(FfiListMessagesOptions::default())
            .await
            .unwrap();
        assert_eq!(bo_messages1.len(), first_msg_check + 1);

        bo_group
            .conversation
            .send("hello2".as_bytes().to_vec(), FfiSendMessageOpts::default())
            .await
            .unwrap();
        message_callbacks.wait_for_delivery(None).await.unwrap();
        bo_group
            .conversation
            .send("hello3".as_bytes().to_vec(), FfiSendMessageOpts::default())
            .await
            .unwrap();
        message_callbacks.wait_for_delivery(None).await.unwrap();

        alix_group.sync().await.unwrap();

        let alix_messages = alix_group
            .find_messages(FfiListMessagesOptions::default())
            .await
            .unwrap();
        assert_eq!(alix_messages.len(), second_msg_check);

        alix_group
            .send("hello4".as_bytes().to_vec(), FfiSendMessageOpts::default())
            .await
            .unwrap();
        message_callbacks.wait_for_delivery(None).await.unwrap();
        bo_group.conversation.sync().await.unwrap();

        let bo_messages2 = bo_group
            .conversation
            .find_messages(FfiListMessagesOptions::default())
            .await
            .unwrap();
        assert_eq!(bo_messages2.len(), second_msg_check + 1);
        assert_eq!(message_callbacks.message_count(), second_msg_check as u32);

        stream_messages.end_and_wait().await.unwrap();
        assert!(stream_messages.is_closed());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_conversation_streaming() {
        let amal = new_test_client().await;
        let bola = new_test_client().await;

        let stream_callback = Arc::new(RustStreamCallback::default());

        let stream = bola.conversations().stream(stream_callback.clone()).await;

        amal.conversations()
            .create_group(
                vec![bola.account_identifier.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();

        stream_callback.wait_for_delivery(None).await.unwrap();

        assert_eq!(stream_callback.message_count(), 1);
        // Create another group and add bola
        amal.conversations()
            .create_group(
                vec![bola.account_identifier.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();
        stream_callback.wait_for_delivery(None).await.unwrap();

        assert_eq!(stream_callback.message_count(), 2);

        stream.end_and_wait().await.unwrap();
        assert!(stream.is_closed());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_stream_all_messages() {
        let alix = new_test_client().await;
        let bo = new_test_client().await;
        let caro = new_test_client().await;

        let alix_group = alix
            .conversations()
            .create_group(
                vec![caro.account_identifier.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();

        let stream_callback = Arc::new(RustStreamCallback::default());

        let stream = caro
            .conversations()
            .stream_all_messages(stream_callback.clone(), None)
            .await;
        stream.wait_for_ready().await;

        alix_group
            .send("first".as_bytes().to_vec(), FfiSendMessageOpts::default())
            .await
            .unwrap();
        stream_callback.wait_for_delivery(None).await.unwrap();

        let bo_group = bo
            .conversations()
            .create_group(
                vec![caro.account_identifier.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();
        let _ = caro.inner_client.sync_welcomes().await.unwrap();

        bo_group
            .send("second".as_bytes().to_vec(), FfiSendMessageOpts::default())
            .await
            .unwrap();
        stream_callback.wait_for_delivery(None).await.unwrap();
        alix_group
            .send("third".as_bytes().to_vec(), FfiSendMessageOpts::default())
            .await
            .unwrap();
        stream_callback.wait_for_delivery(None).await.unwrap();
        bo_group
            .send("fourth".as_bytes().to_vec(), FfiSendMessageOpts::default())
            .await
            .unwrap();
        stream_callback.wait_for_delivery(None).await.unwrap();

        assert_eq!(stream_callback.message_count(), 4);
        stream.end_and_wait().await.unwrap();
        assert!(stream.is_closed());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_message_streaming() {
        let amal = new_test_client().await;
        let bola = new_test_client().await;

        let amal_group: Arc<FfiConversation> = amal
            .conversations()
            .create_group(
                vec![bola.account_identifier.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();

        bola.inner_client.sync_welcomes().await.unwrap();
        let bola_group = bola.conversation(amal_group.id()).unwrap();

        let stream_callback = Arc::new(RustStreamCallback::default());
        let stream_closer = bola_group.stream(stream_callback.clone()).await;

        stream_closer.wait_for_ready().await;

        amal_group
            .send("hello".as_bytes().to_vec(), FfiSendMessageOpts::default())
            .await
            .unwrap();
        stream_callback.wait_for_delivery(None).await.unwrap();

        amal_group
            .send("goodbye".as_bytes().to_vec(), FfiSendMessageOpts::default())
            .await
            .unwrap();
        stream_callback.wait_for_delivery(None).await.unwrap();

        assert_eq!(stream_callback.message_count(), 2);
        stream_closer.end_and_wait().await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_message_streaming_when_removed_then_added() {
        let amal = new_test_client().await;
        let bola = new_test_client().await;
        log::info!(
            "Created Inbox IDs {} and {}",
            amal.inbox_id(),
            bola.inbox_id()
        );

        let amal_group = amal
            .conversations()
            .create_group(
                vec![bola.account_identifier.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();

        let stream_callback = Arc::new(RustStreamCallback::default());
        let stream_closer = bola
            .conversations()
            .stream_all_messages(stream_callback.clone(), None)
            .await;
        stream_closer.wait_for_ready().await;

        amal_group
            .send(b"hello1".to_vec(), FfiSendMessageOpts::default())
            .await
            .unwrap();
        stream_callback.wait_for_delivery(None).await.unwrap();
        amal_group
            .send(b"hello2".to_vec(), FfiSendMessageOpts::default())
            .await
            .unwrap();
        stream_callback.wait_for_delivery(None).await.unwrap();

        assert_eq!(stream_callback.message_count(), 2);
        assert!(!stream_closer.is_closed());

        amal_group
            .remove_members_by_inbox_id(vec![bola.inbox_id().clone()])
            .await
            .unwrap();
        stream_callback.wait_for_delivery(None).await.unwrap();
        assert_eq!(stream_callback.message_count(), 3); // Member removal transcript message
        //
        amal_group
            .send(b"hello3".to_vec(), FfiSendMessageOpts::default())
            .await
            .unwrap();
        //TODO: could verify with a log message
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        assert_eq!(stream_callback.message_count(), 3); // Don't receive messages while removed
        assert!(!stream_closer.is_closed());

        amal_group
            .add_members(vec![bola.account_identifier.clone()])
            .await
            .unwrap();

        // TODO: could check for LOG message with a Eviction error on receive
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        assert_eq!(stream_callback.message_count(), 3); // Don't receive transcript messages while removed

        amal_group
            .send("hello4".as_bytes().to_vec(), FfiSendMessageOpts::default())
            .await
            .unwrap();
        stream_callback.wait_for_delivery(None).await.unwrap();
        assert_eq!(stream_callback.message_count(), 4); // Receiving messages again
        assert!(!stream_closer.is_closed());

        stream_closer.end_and_wait().await.unwrap();
        assert!(stream_closer.is_closed());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_group_who_added_me() {
        // Create Clients
        let amal = new_test_client().await;
        let bola = new_test_client().await;

        // Amal creates a group and adds Bola to the group
        amal.conversations()
            .create_group(
                vec![bola.account_identifier.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();

        // Bola syncs groups - this will decrypt the Welcome, identify who added Bola
        // and then store that value on the group and insert into the database
        let bola_conversations = bola.conversations();
        let _ = bola_conversations.sync().await;

        // Bola gets the group id. This will be needed to fetch the group from
        // the database.
        let bola_groups = bola_conversations
            .list(FfiListConversationsOptions::default())
            .unwrap();

        let bola_group = bola_groups.first().unwrap();

        // Check Bola's group for the added_by_inbox_id of the inviter
        let added_by_inbox_id = bola_group.conversation.added_by_inbox_id().unwrap();

        // // Verify the welcome host_credential is equal to Amal's
        assert_eq!(
            amal.inbox_id(),
            added_by_inbox_id,
            "The Inviter and added_by_address do not match!"
        );
    }

    // TODO: Test current fails 50% of the time with db locking messages
    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_stream_groups_gets_callback_when_streaming_messages() {
        let alix = new_test_client().await;
        let bo = new_test_client().await;

        // Stream all group messages
        let message_callback = Arc::new(RustStreamCallback::default());
        let group_callback = Arc::new(RustStreamCallback::default());
        let stream_groups = bo.conversations().stream(group_callback.clone()).await;

        let stream_messages = bo
            .conversations()
            .stream_all_messages(message_callback.clone(), None)
            .await;
        stream_messages.wait_for_ready().await;

        // Create group and send first message
        let alix_group = alix
            .conversations()
            .create_group(
                vec![bo.account_identifier.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();
        group_callback.wait_for_delivery(None).await.unwrap();

        alix_group
            .send("hello1".as_bytes().to_vec(), FfiSendMessageOpts::default())
            .await
            .unwrap();
        message_callback.wait_for_delivery(None).await.unwrap();

        assert_eq!(group_callback.message_count(), 1);
        assert_eq!(message_callback.message_count(), 1);

        stream_messages.end_and_wait().await.unwrap();
        assert!(stream_messages.is_closed());

        stream_groups.end_and_wait().await.unwrap();
        assert!(stream_groups.is_closed());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_group_permissions_show_expected_values() {
        let alix = new_test_client().await;
        let bo = new_test_client().await;
        // Create admin_only group
        let admin_only_options = FfiCreateGroupOptions {
            permissions: Some(FfiGroupPermissionsOptions::AdminOnly),
            ..Default::default()
        };
        let alix_group_admin_only = alix
            .conversations()
            .create_group(vec![bo.account_identifier.clone()], admin_only_options)
            .await
            .unwrap();

        // Verify we can read the expected permissions
        let alix_permission_policy_set = alix_group_admin_only
            .group_permissions()
            .unwrap()
            .policy_set()
            .unwrap();
        let expected_permission_policy_set = FfiPermissionPolicySet {
            add_member_policy: FfiPermissionPolicy::Admin,
            remove_member_policy: FfiPermissionPolicy::Admin,
            add_admin_policy: FfiPermissionPolicy::SuperAdmin,
            remove_admin_policy: FfiPermissionPolicy::SuperAdmin,
            update_group_name_policy: FfiPermissionPolicy::Admin,
            update_group_description_policy: FfiPermissionPolicy::Admin,
            update_group_image_url_square_policy: FfiPermissionPolicy::Admin,
            update_message_disappearing_policy: FfiPermissionPolicy::Admin,
        };
        assert_eq!(alix_permission_policy_set, expected_permission_policy_set);

        // Create all_members group
        let all_members_options = FfiCreateGroupOptions {
            permissions: Some(FfiGroupPermissionsOptions::Default),
            ..Default::default()
        };
        let alix_group_all_members = alix
            .conversations()
            .create_group(vec![bo.account_identifier.clone()], all_members_options)
            .await
            .unwrap();

        // Verify we can read the expected permissions
        let alix_permission_policy_set = alix_group_all_members
            .group_permissions()
            .unwrap()
            .policy_set()
            .unwrap();
        let expected_permission_policy_set = FfiPermissionPolicySet {
            add_member_policy: FfiPermissionPolicy::Allow,
            remove_member_policy: FfiPermissionPolicy::Admin,
            add_admin_policy: FfiPermissionPolicy::SuperAdmin,
            remove_admin_policy: FfiPermissionPolicy::SuperAdmin,
            update_group_name_policy: FfiPermissionPolicy::Allow,
            update_group_description_policy: FfiPermissionPolicy::Allow,
            update_group_image_url_square_policy: FfiPermissionPolicy::Allow,
            update_message_disappearing_policy: FfiPermissionPolicy::Admin,
        };
        assert_eq!(alix_permission_policy_set, expected_permission_policy_set);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_dm_permissions_show_expected_values() {
        let alix = new_test_client().await;
        let bo = new_test_client().await;

        let alix_group_admin_only = alix
            .conversations()
            .find_or_create_dm(bo.account_identifier.clone(), FfiCreateDMOptions::default())
            .await
            .unwrap();

        // Verify we can read the expected permissions
        let alix_permission_policy_set = alix_group_admin_only
            .group_permissions()
            .unwrap()
            .policy_set()
            .unwrap();
        let expected_permission_policy_set = FfiPermissionPolicySet {
            add_member_policy: FfiPermissionPolicy::Deny,
            remove_member_policy: FfiPermissionPolicy::Deny,
            add_admin_policy: FfiPermissionPolicy::Deny,
            remove_admin_policy: FfiPermissionPolicy::Deny,
            update_group_name_policy: FfiPermissionPolicy::Allow,
            update_group_description_policy: FfiPermissionPolicy::Allow,
            update_group_image_url_square_policy: FfiPermissionPolicy::Allow,
            update_message_disappearing_policy: FfiPermissionPolicy::Allow,
        };
        assert_eq!(alix_permission_policy_set, expected_permission_policy_set);

        // Create all_members group
        let all_members_options = FfiCreateGroupOptions {
            permissions: Some(FfiGroupPermissionsOptions::Default),
            ..Default::default()
        };
        let alix_group_all_members = alix
            .conversations()
            .create_group(vec![bo.account_identifier.clone()], all_members_options)
            .await
            .unwrap();

        // Verify we can read the expected permissions
        let alix_permission_policy_set = alix_group_all_members
            .group_permissions()
            .unwrap()
            .policy_set()
            .unwrap();
        let expected_permission_policy_set = FfiPermissionPolicySet {
            add_member_policy: FfiPermissionPolicy::Allow,
            remove_member_policy: FfiPermissionPolicy::Admin,
            add_admin_policy: FfiPermissionPolicy::SuperAdmin,
            remove_admin_policy: FfiPermissionPolicy::SuperAdmin,
            update_group_name_policy: FfiPermissionPolicy::Allow,
            update_group_description_policy: FfiPermissionPolicy::Allow,
            update_group_image_url_square_policy: FfiPermissionPolicy::Allow,
            update_message_disappearing_policy: FfiPermissionPolicy::Admin,
        };
        assert_eq!(alix_permission_policy_set, expected_permission_policy_set);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_permissions_updates() {
        let alix = new_test_client().await;
        let bola = new_test_client().await;

        let admin_only_options = FfiCreateGroupOptions {
            permissions: Some(FfiGroupPermissionsOptions::AdminOnly),
            ..Default::default()
        };
        let alix_group = alix
            .conversations()
            .create_group(vec![bola.account_identifier.clone()], admin_only_options)
            .await
            .unwrap();

        let alix_group_permissions = alix_group
            .group_permissions()
            .unwrap()
            .policy_set()
            .unwrap();
        let expected_permission_policy_set = FfiPermissionPolicySet {
            add_member_policy: FfiPermissionPolicy::Admin,
            remove_member_policy: FfiPermissionPolicy::Admin,
            add_admin_policy: FfiPermissionPolicy::SuperAdmin,
            remove_admin_policy: FfiPermissionPolicy::SuperAdmin,
            update_group_name_policy: FfiPermissionPolicy::Admin,
            update_group_description_policy: FfiPermissionPolicy::Admin,
            update_group_image_url_square_policy: FfiPermissionPolicy::Admin,
            update_message_disappearing_policy: FfiPermissionPolicy::Admin,
        };
        assert_eq!(alix_group_permissions, expected_permission_policy_set);

        // Let's update the group so that the image url can be updated by anyone
        alix_group
            .update_permission_policy(
                FfiPermissionUpdateType::UpdateMetadata,
                FfiPermissionPolicy::Allow,
                Some(FfiMetadataField::ImageUrlSquare),
            )
            .await
            .unwrap();
        alix_group.sync().await.unwrap();
        let alix_group_permissions = alix_group
            .group_permissions()
            .unwrap()
            .policy_set()
            .unwrap();
        let new_expected_permission_policy_set = FfiPermissionPolicySet {
            add_member_policy: FfiPermissionPolicy::Admin,
            remove_member_policy: FfiPermissionPolicy::Admin,
            add_admin_policy: FfiPermissionPolicy::SuperAdmin,
            remove_admin_policy: FfiPermissionPolicy::SuperAdmin,
            update_group_name_policy: FfiPermissionPolicy::Admin,
            update_group_description_policy: FfiPermissionPolicy::Admin,
            update_group_image_url_square_policy: FfiPermissionPolicy::Allow,
            update_message_disappearing_policy: FfiPermissionPolicy::Admin,
        };
        assert_eq!(alix_group_permissions, new_expected_permission_policy_set);

        // Verify that bo can not update the group name
        let bola_conversations = bola.conversations();
        let _ = bola_conversations.sync().await;
        let bola_groups = bola_conversations
            .list(FfiListConversationsOptions::default())
            .unwrap();

        let bola_group = bola_groups.first().unwrap();
        bola_group
            .conversation
            .update_group_name("new_name".to_string())
            .await
            .unwrap_err();

        // Verify that bo CAN update the image url
        bola_group
            .conversation
            .update_group_image_url_square("https://example.com/image.png".to_string())
            .await
            .unwrap();

        // Verify we can read the correct values from the group
        bola_group.conversation.sync().await.unwrap();
        alix_group.sync().await.unwrap();
        assert_eq!(
            bola_group.conversation.group_image_url_square().unwrap(),
            "https://example.com/image.png"
        );
        assert_eq!(bola_group.conversation.group_name().unwrap(), "");
        assert_eq!(
            alix_group.group_image_url_square().unwrap(),
            "https://example.com/image.png"
        );
        assert_eq!(alix_group.group_name().unwrap(), "");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_disappearing_messages_deletion() {
        let alix = new_test_client().await;
        let alix_provider = alix.inner_client.context.mls_provider();
        let bola = new_test_client().await;
        let bola_provider = bola.inner_client.context.mls_provider();

        // Step 1: Create a group
        let alix_group = alix
            .conversations()
            .create_group(
                vec![bola.account_identifier.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();

        // Step 2: Send a message and sync
        alix_group
            .send(
                "Msg 1 from group".as_bytes().to_vec(),
                FfiSendMessageOpts::default(),
            )
            .await
            .unwrap();
        alix_group.sync().await.unwrap();

        // Step 3: Verify initial messages
        let mut alix_messages = alix_group
            .find_messages(FfiListMessagesOptions::default())
            .await
            .unwrap();
        assert_eq!(alix_messages.len(), 2);

        // Step 4: Set disappearing settings to 5ns after the latest message
        let latest_message_sent_at_ns = alix_messages.last().unwrap().sent_at_ns;
        let disappearing_settings =
            FfiMessageDisappearingSettings::new(latest_message_sent_at_ns, 5);
        alix_group
            .update_conversation_message_disappearing_settings(disappearing_settings.clone())
            .await
            .unwrap();
        alix_group.sync().await.unwrap();

        // Verify the settings were applied
        let group_from_db = alix
            .inner_client
            .context
            .db()
            .find_group(&alix_group.id())
            .unwrap();
        assert_eq!(
            group_from_db
                .clone()
                .unwrap()
                .message_disappear_from_ns
                .unwrap(),
            disappearing_settings.from_ns
        );
        assert_eq!(
            group_from_db.unwrap().message_disappear_in_ns.unwrap(),
            disappearing_settings.in_ns
        );
        assert!(
            alix_group
                .is_conversation_message_disappearing_enabled()
                .unwrap()
        );

        bola.conversations()
            .sync_all_conversations(None)
            .await
            .unwrap();

        let bola_group_from_db = bola_provider
            .key_store()
            .db()
            .find_group(&alix_group.id())
            .unwrap();
        assert_eq!(
            bola_group_from_db
                .clone()
                .unwrap()
                .message_disappear_from_ns
                .unwrap(),
            disappearing_settings.from_ns
        );
        assert_eq!(
            bola_group_from_db.unwrap().message_disappear_in_ns.unwrap(),
            disappearing_settings.in_ns
        );
        assert!(
            alix_group
                .is_conversation_message_disappearing_enabled()
                .unwrap()
        );

        // Step 5: Send additional messages
        for msg in &["Msg 2 from group", "Msg 3 from group", "Msg 4 from group"] {
            alix_group
                .send(msg.as_bytes().to_vec(), FfiSendMessageOpts::default())
                .await
                .unwrap();
        }
        alix_group.sync().await.unwrap();

        // Step 6: Verify total message count before cleanup
        alix_messages = alix_group
            .find_messages(FfiListMessagesOptions::default())
            .await
            .unwrap();
        let msg_counts_before_cleanup = alix_messages.len();

        // Wait for cleanup to complete
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        // Step 8: Disable disappearing messages
        alix_group
            .remove_conversation_message_disappearing_settings()
            .await
            .unwrap();
        alix_group.sync().await.unwrap();

        // Verify disappearing settings are disabled
        let group_from_db = alix_provider
            .key_store()
            .db()
            .find_group(&alix_group.id())
            .unwrap();
        assert_eq!(
            group_from_db
                .clone()
                .unwrap()
                .message_disappear_from_ns
                .unwrap(),
            0
        );
        assert!(
            !alix_group
                .is_conversation_message_disappearing_enabled()
                .unwrap()
        );

        assert_eq!(group_from_db.unwrap().message_disappear_in_ns.unwrap(), 0);

        // Step 9: Send another message
        alix_group
            .send(
                "Msg 5 from group".as_bytes().to_vec(),
                FfiSendMessageOpts::default(),
            )
            .await
            .unwrap();

        // Step 10: Verify messages after cleanup
        alix_messages = alix_group
            .find_messages(FfiListMessagesOptions::default())
            .await
            .unwrap();
        assert_eq!(msg_counts_before_cleanup, alix_messages.len());
        // 3 messages got deleted, then two messages got added for metadataUpdate and one normal messaged added later
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_disappearing_messages_with_0_from_ns_settings() {
        let alix = new_test_client().await;
        let alix_provider = alix.inner_client.context.mls_provider();
        let bola = new_test_client().await;
        let bola_provider = bola.inner_client.context.mls_provider();

        // Step 1: Create a group
        let alix_group = alix
            .conversations()
            .create_group(
                vec![bola.account_identifier.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();

        // Step 2: Send a message and sync
        alix_group
            .send(
                "Msg 1 from group".as_bytes().to_vec(),
                FfiSendMessageOpts::default(),
            )
            .await
            .unwrap();
        alix_group.sync().await.unwrap();

        // Step 3: Verify initial messages
        let mut alix_messages = alix_group
            .find_messages(FfiListMessagesOptions::default())
            .await
            .unwrap();
        assert_eq!(alix_messages.len(), 2);

        // Step 4: Set disappearing settings to 5ns after the latest message and from ns 0
        let disappearing_settings = FfiMessageDisappearingSettings::new(0, 5);
        alix_group
            .update_conversation_message_disappearing_settings(disappearing_settings.clone())
            .await
            .unwrap();
        alix_group.sync().await.unwrap();

        // Verify the settings were applied and the settings is not enabled
        let group_from_db = alix
            .inner_client
            .context
            .db()
            .find_group(&alix_group.id())
            .unwrap();
        assert_eq!(
            group_from_db
                .clone()
                .unwrap()
                .message_disappear_from_ns
                .unwrap(),
            disappearing_settings.from_ns
        );
        assert_eq!(
            group_from_db.unwrap().message_disappear_in_ns.unwrap(),
            disappearing_settings.in_ns
        );
        assert!(
            !alix_group
                .is_conversation_message_disappearing_enabled()
                .unwrap()
        );

        bola.conversations()
            .sync_all_conversations(None)
            .await
            .unwrap();

        let bola_group_from_db = bola_provider
            .key_store()
            .db()
            .find_group(&alix_group.id())
            .unwrap();
        assert_eq!(
            bola_group_from_db
                .clone()
                .unwrap()
                .message_disappear_from_ns
                .unwrap(),
            disappearing_settings.from_ns
        );
        assert_eq!(
            bola_group_from_db.unwrap().message_disappear_in_ns.unwrap(),
            disappearing_settings.in_ns
        );
        assert!(
            !alix_group
                .is_conversation_message_disappearing_enabled()
                .unwrap()
        );

        // Step 5: Send additional messages
        for msg in &["Msg 2 from group", "Msg 3 from group", "Msg 4 from group"] {
            alix_group
                .send(msg.as_bytes().to_vec(), FfiSendMessageOpts::default())
                .await
                .unwrap();
        }
        alix_group.sync().await.unwrap();

        // Step 6: Verify total message count before cleanup
        alix_messages = alix_group
            .find_messages(FfiListMessagesOptions::default())
            .await
            .unwrap();
        let msg_counts_before_cleanup = alix_messages.len();

        // Wait for cleanup to complete
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        // Step 8: Disable disappearing messages
        alix_group
            .remove_conversation_message_disappearing_settings()
            .await
            .unwrap();
        alix_group.sync().await.unwrap();

        // Verify disappearing settings are disabled
        let group_from_db = alix_provider
            .key_store()
            .db()
            .find_group(&alix_group.id())
            .unwrap();
        assert_eq!(
            group_from_db
                .clone()
                .unwrap()
                .message_disappear_from_ns
                .unwrap(),
            0
        );
        assert!(
            !alix_group
                .is_conversation_message_disappearing_enabled()
                .unwrap()
        );

        assert_eq!(group_from_db.unwrap().message_disappear_in_ns.unwrap(), 0);

        // Step 9: Send another message
        alix_group
            .send(
                "Msg 5 from group".as_bytes().to_vec(),
                FfiSendMessageOpts::default(),
            )
            .await
            .unwrap();

        // Step 10: Verify messages after cleanup
        alix_messages = alix_group
            .find_messages(FfiListMessagesOptions::default())
            .await
            .unwrap();
        // messages before cleanup + 1 message added for metadataUpdate + 1 message added for 1 normal message
        assert_eq!(msg_counts_before_cleanup + 2, alix_messages.len());
        // 3 messages got deleted, then two messages got added for metadataUpdate and one normal messaged added later
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_set_disappearing_messages_when_creating_group() {
        let alix = new_test_client().await;
        let alix_provider = alix.inner_client.context.mls_provider();
        let bola = new_test_client().await;
        let disappearing_settings = FfiMessageDisappearingSettings::new(now_ns(), 2_000_000_000);
        // Step 1: Create a group
        let alix_group = alix
            .conversations()
            .create_group(
                vec![bola.account_identifier.clone()],
                FfiCreateGroupOptions {
                    permissions: Some(FfiGroupPermissionsOptions::AdminOnly),
                    group_name: Some("Group Name".to_string()),
                    group_image_url_square: Some("url".to_string()),
                    group_description: Some("group description".to_string()),
                    custom_permission_policy_set: None,
                    message_disappearing_settings: Some(disappearing_settings.clone()),
                },
            )
            .await
            .unwrap();

        // Step 2: Send a message and sync
        alix_group
            .send(
                "Msg 1 from group".as_bytes().to_vec(),
                FfiSendMessageOpts::default(),
            )
            .await
            .unwrap();
        alix_group.sync().await.unwrap();

        // Step 3: Verify initial messages
        let alix_messages = alix_group
            .find_messages(FfiListMessagesOptions::default())
            .await
            .unwrap();
        assert_eq!(alix_messages.len(), 2);
        let group_from_db = alix_provider
            .key_store()
            .db()
            .find_group(&alix_group.id())
            .unwrap();
        assert_eq!(
            group_from_db
                .clone()
                .unwrap()
                .message_disappear_from_ns
                .unwrap(),
            disappearing_settings.from_ns
        );
        assert!(
            alix_group
                .is_conversation_message_disappearing_enabled()
                .unwrap()
        );
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        let alix_messages = alix_group
            .find_messages(FfiListMessagesOptions::default())
            .await
            .unwrap();
        assert_eq!(alix_messages.len(), 1);
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_set_disappearing_messages_when_creating_dm() {
        let alix = new_test_client().await;
        let alix_provider = alix.inner_client.context.mls_provider();
        let bola = new_test_client().await;
        let disappearing_settings = FfiMessageDisappearingSettings::new(now_ns(), 2_000_000_000);
        // Step 1: Create a group
        let alix_group = alix
            .conversations()
            .find_or_create_dm(
                bola.account_identifier.clone(),
                FfiCreateDMOptions::new(disappearing_settings.clone()),
            )
            .await
            .unwrap();

        // Step 2: Send a message and sync
        alix_group
            .send(
                "Msg 1 from group".as_bytes().to_vec(),
                FfiSendMessageOpts::default(),
            )
            .await
            .unwrap();
        alix_group.sync().await.unwrap();

        // Step 3: Verify initial messages
        let alix_messages = alix_group
            .find_messages(FfiListMessagesOptions::default())
            .await
            .unwrap();

        assert_eq!(alix_messages.len(), 2);
        let group_from_db = alix_provider
            .key_store()
            .db()
            .find_group(&alix_group.id())
            .unwrap();
        assert_eq!(
            group_from_db
                .clone()
                .unwrap()
                .message_disappear_from_ns
                .unwrap(),
            disappearing_settings.from_ns
        );
        assert!(
            alix_group
                .is_conversation_message_disappearing_enabled()
                .unwrap()
        );
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        let alix_messages = alix_group
            .find_messages(FfiListMessagesOptions::default())
            .await
            .unwrap();
        assert_eq!(alix_messages.len(), 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_group_creation_custom_permissions() {
        let alix = Tester::new().await;
        let bola = Tester::new().await;

        let custom_permissions = FfiPermissionPolicySet {
            add_admin_policy: FfiPermissionPolicy::Admin,
            remove_admin_policy: FfiPermissionPolicy::Admin,
            update_group_name_policy: FfiPermissionPolicy::Admin,
            update_group_description_policy: FfiPermissionPolicy::Allow,
            update_group_image_url_square_policy: FfiPermissionPolicy::Admin,
            add_member_policy: FfiPermissionPolicy::Allow,
            remove_member_policy: FfiPermissionPolicy::Deny,
            update_message_disappearing_policy: FfiPermissionPolicy::Admin,
        };

        let create_group_options = FfiCreateGroupOptions {
            permissions: Some(FfiGroupPermissionsOptions::CustomPolicy),
            group_name: Some("Test Group".to_string()),
            group_image_url_square: Some("https://example.com/image.png".to_string()),
            group_description: Some("A test group".to_string()),
            custom_permission_policy_set: Some(custom_permissions),
            message_disappearing_settings: None,
        };

        let alix_group = alix
            .conversations()
            .create_group(vec![bola.account_identifier.clone()], create_group_options)
            .await
            .unwrap();

        // Verify the group was created with the correct permissions
        let group_permissions_policy_set = alix_group
            .group_permissions()
            .unwrap()
            .policy_set()
            .unwrap();
        assert_eq!(
            group_permissions_policy_set.add_admin_policy,
            FfiPermissionPolicy::Admin
        );
        assert_eq!(
            group_permissions_policy_set.remove_admin_policy,
            FfiPermissionPolicy::Admin
        );
        assert_eq!(
            group_permissions_policy_set.update_group_name_policy,
            FfiPermissionPolicy::Admin
        );
        assert_eq!(
            group_permissions_policy_set.update_group_description_policy,
            FfiPermissionPolicy::Allow
        );
        assert_eq!(
            group_permissions_policy_set.update_group_image_url_square_policy,
            FfiPermissionPolicy::Admin
        );

        assert_eq!(
            group_permissions_policy_set.update_message_disappearing_policy,
            FfiPermissionPolicy::Admin
        );
        assert_eq!(
            group_permissions_policy_set.add_member_policy,
            FfiPermissionPolicy::Allow
        );
        assert_eq!(
            group_permissions_policy_set.remove_member_policy,
            FfiPermissionPolicy::Deny
        );

        // Verify that Bola can not update the group name
        let bola_conversations = bola.conversations();
        let _ = bola_conversations.sync().await;
        let bola_groups = bola_conversations
            .list(FfiListConversationsOptions::default())
            .unwrap();

        let bola_group = bola_groups.first().unwrap();
        bola_group
            .conversation
            .update_group_name("new_name".to_string())
            .await
            .unwrap_err();
        let result = bola_group
            .conversation
            .update_group_name("New Group Name".to_string())
            .await;
        assert!(result.is_err());

        // Verify that Alix can update the group name
        let result = alix_group
            .update_group_name("New Group Name".to_string())
            .await;
        assert!(result.is_ok());

        // Verify that Bola can update the group description
        let result = bola_group
            .conversation
            .update_group_description("New Description".to_string())
            .await;
        assert!(result.is_ok());

        // Verify that Alix can not remove bola even though they are a super admin
        let result = alix_group
            .remove_members(vec![bola.account_identifier.clone()])
            .await;
        assert!(result.is_err());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_group_creation_custom_permissions_fails_when_invalid() {
        let alix = Tester::new().await;
        let bola = Tester::new().await;

        // Add / Remove Admin must be Admin or Super Admin or Deny
        let custom_permissions_invalid_1 = FfiPermissionPolicySet {
            add_admin_policy: FfiPermissionPolicy::Allow,
            remove_admin_policy: FfiPermissionPolicy::Admin,
            update_group_name_policy: FfiPermissionPolicy::Admin,
            update_group_description_policy: FfiPermissionPolicy::Allow,
            update_group_image_url_square_policy: FfiPermissionPolicy::Admin,
            add_member_policy: FfiPermissionPolicy::Allow,
            remove_member_policy: FfiPermissionPolicy::Deny,
            update_message_disappearing_policy: FfiPermissionPolicy::Admin,
        };

        let custom_permissions_valid = FfiPermissionPolicySet {
            add_admin_policy: FfiPermissionPolicy::Admin,
            remove_admin_policy: FfiPermissionPolicy::Admin,
            update_group_name_policy: FfiPermissionPolicy::Admin,
            update_group_description_policy: FfiPermissionPolicy::Allow,
            update_group_image_url_square_policy: FfiPermissionPolicy::Admin,
            add_member_policy: FfiPermissionPolicy::Allow,
            remove_member_policy: FfiPermissionPolicy::Deny,
            update_message_disappearing_policy: FfiPermissionPolicy::Admin,
        };

        let create_group_options_invalid_1 = FfiCreateGroupOptions {
            permissions: Some(FfiGroupPermissionsOptions::CustomPolicy),
            group_name: Some("Test Group".to_string()),
            group_image_url_square: Some("https://example.com/image.png".to_string()),
            group_description: Some("A test group".to_string()),
            custom_permission_policy_set: Some(custom_permissions_invalid_1),
            message_disappearing_settings: None,
        };

        let results_1 = alix
            .conversations()
            .create_group(
                vec![bola.account_identifier.clone()],
                create_group_options_invalid_1,
            )
            .await;

        assert!(results_1.is_err());

        let create_group_options_invalid_2 = FfiCreateGroupOptions {
            permissions: Some(FfiGroupPermissionsOptions::Default),
            group_name: Some("Test Group".to_string()),
            group_image_url_square: Some("https://example.com/image.png".to_string()),
            group_description: Some("A test group".to_string()),
            custom_permission_policy_set: Some(custom_permissions_valid.clone()),
            message_disappearing_settings: None,
        };

        let results_2 = alix
            .conversations()
            .create_group(
                vec![bola.account_identifier.clone()],
                create_group_options_invalid_2,
            )
            .await;

        assert!(results_2.is_err());

        let create_group_options_invalid_3 = FfiCreateGroupOptions {
            permissions: None,
            group_name: Some("Test Group".to_string()),
            group_image_url_square: Some("https://example.com/image.png".to_string()),
            group_description: Some("A test group".to_string()),
            custom_permission_policy_set: Some(custom_permissions_valid.clone()),
            message_disappearing_settings: None,
        };

        let results_3 = alix
            .conversations()
            .create_group(
                vec![bola.account_identifier.clone()],
                create_group_options_invalid_3,
            )
            .await;

        assert!(results_3.is_err());

        let create_group_options_valid = FfiCreateGroupOptions {
            permissions: Some(FfiGroupPermissionsOptions::CustomPolicy),
            group_name: Some("Test Group".to_string()),
            group_image_url_square: Some("https://example.com/image.png".to_string()),
            group_description: Some("A test group".to_string()),
            custom_permission_policy_set: Some(custom_permissions_valid),
            message_disappearing_settings: None,
        };

        let results_4 = alix
            .conversations()
            .create_group(
                vec![bola.account_identifier.clone()],
                create_group_options_valid,
            )
            .await;

        assert!(results_4.is_ok());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_sign_and_verify() {
        let signature_text = "Hello there.";

        let client = new_test_client().await;
        let signature_bytes = client.sign_with_installation_key(signature_text).unwrap();

        // check if verification works
        let result =
            client.verify_signed_with_installation_key(signature_text, signature_bytes.clone());
        assert!(result.is_ok());

        // different text should result in an error.
        let result = client.verify_signed_with_installation_key("Hello here.", signature_bytes);
        assert!(result.is_err());

        // different bytes should result in an error
        let signature_bytes = vec![0; 64];
        let result = client.verify_signed_with_installation_key(signature_text, signature_bytes);
        assert!(result.is_err());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_revoke_all_installations() {
        let wallet = PrivateKeySigner::random();
        let client_1 = new_test_client_with_wallet(wallet.clone()).await;
        let client_2 = new_test_client_with_wallet(wallet.clone()).await;

        let client_1_state = client_1.inbox_state(true).await.unwrap();
        let client_2_state = client_2.inbox_state(true).await.unwrap();
        assert_eq!(client_1_state.installations.len(), 2);
        assert_eq!(client_2_state.installations.len(), 2);

        let signature_request = client_1.revoke_all_other_installations().await.unwrap();
        signature_request.add_wallet_signature(&wallet).await;
        client_1
            .apply_signature_request(signature_request)
            .await
            .unwrap();

        let client_1_state_after_revoke = client_1.inbox_state(true).await.unwrap();
        let client_2_state_after_revoke = client_2.inbox_state(true).await.unwrap();
        assert_eq!(client_1_state_after_revoke.installations.len(), 1);
        assert_eq!(client_2_state_after_revoke.installations.len(), 1);
        assert_eq!(
            client_1_state_after_revoke
                .installations
                .first()
                .unwrap()
                .id,
            client_1.installation_id()
        );
        assert_eq!(
            client_2_state_after_revoke
                .installations
                .first()
                .unwrap()
                .id,
            client_1.installation_id()
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_revoke_installations() {
        let wallet = PrivateKeySigner::random();
        let client_1 = new_test_client_with_wallet(wallet.clone()).await;
        let client_2 = new_test_client_with_wallet(wallet.clone()).await;

        let client_1_state = client_1.inbox_state(true).await.unwrap();
        let client_2_state = client_2.inbox_state(true).await.unwrap();
        assert_eq!(client_1_state.installations.len(), 2);
        assert_eq!(client_2_state.installations.len(), 2);

        let signature_request = client_1
            .revoke_installations(vec![client_2.installation_id()])
            .await
            .unwrap();
        signature_request.add_wallet_signature(&wallet).await;
        client_1
            .apply_signature_request(signature_request)
            .await
            .unwrap();

        let client_1_state_after_revoke = client_1.inbox_state(true).await.unwrap();
        let client_2_state_after_revoke = client_2.inbox_state(true).await.unwrap();
        assert_eq!(client_1_state_after_revoke.installations.len(), 1);
        assert_eq!(client_2_state_after_revoke.installations.len(), 1);
        assert_eq!(
            client_1_state_after_revoke
                .installations
                .first()
                .unwrap()
                .id,
            client_1.installation_id()
        );
        assert_eq!(
            client_2_state_after_revoke
                .installations
                .first()
                .unwrap()
                .id,
            client_1.installation_id()
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_dms_sync_but_do_not_list() {
        let alix = Tester::new().await;
        let bola = Tester::new().await;

        let alix_conversations = alix.conversations();
        let bola_conversations = bola.conversations();

        let _alix_dm = alix_conversations
            .find_or_create_dm(
                bola.account_identifier.clone(),
                FfiCreateDMOptions::default(),
            )
            .await
            .unwrap();
        let alix_num_sync = alix_conversations
            .sync_all_conversations(None)
            .await
            .unwrap();
        bola_conversations.sync().await.unwrap();
        let bola_num_sync = bola_conversations
            .sync_all_conversations(None)
            .await
            .unwrap();
        assert_eq!(alix_num_sync, 1);
        assert_eq!(bola_num_sync, 1);

        let alix_groups = alix_conversations
            .list_groups(FfiListConversationsOptions::default())
            .unwrap();
        assert_eq!(alix_groups.len(), 0);

        let bola_groups = bola_conversations
            .list_groups(FfiListConversationsOptions::default())
            .unwrap();
        assert_eq!(bola_groups.len(), 0);

        let alix_dms = alix_conversations
            .list_dms(FfiListConversationsOptions::default())
            .unwrap();
        assert_eq!(alix_dms.len(), 1);

        let bola_dms = bola_conversations
            .list_dms(FfiListConversationsOptions::default())
            .unwrap();
        assert_eq!(bola_dms.len(), 1);

        let alix_conversations = alix_conversations
            .list(FfiListConversationsOptions::default())
            .unwrap();
        assert_eq!(alix_conversations.len(), 1);

        let bola_conversations = bola_conversations
            .list(FfiListConversationsOptions::default())
            .unwrap();
        assert_eq!(bola_conversations.len(), 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_dm_stream_correct_type() {
        let amal = Tester::new().await;
        let bola = Tester::new().await;

        let stream_callback = Arc::new(RustStreamCallback::default());
        amal.conversations()
            .stream_dms(stream_callback.clone())
            .await;
        amal.conversations()
            .find_or_create_dm(
                bola.account_identifier.clone(),
                FfiCreateDMOptions::default(),
            )
            .await
            .unwrap();
        stream_callback.wait_for_delivery(None).await.unwrap();
        assert_eq!(stream_callback.message_count(), 1);

        let convo_list = stream_callback.conversations.lock();
        assert_eq!(convo_list.len(), 1);
        assert_eq!(convo_list[0].conversation_type(), FfiConversationType::Dm);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_dm_streaming() {
        let alix = Tester::new().await;
        let bo = Tester::new().await;
        let caro = Tester::new().await;

        // Stream all conversations
        let stream_callback = Arc::new(RustStreamCallback::default());
        let stream = bo.conversations().stream(stream_callback.clone()).await;

        alix.conversations()
            .create_group(
                vec![bo.account_identifier.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();

        stream_callback.wait_for_delivery(None).await.unwrap();

        assert_eq!(stream_callback.message_count(), 1);
        alix.conversations()
            .find_or_create_dm(bo.account_identifier.clone(), FfiCreateDMOptions::default())
            .await
            .unwrap();
        stream_callback.wait_for_delivery(None).await.unwrap();

        assert_eq!(stream_callback.message_count(), 2);

        stream.end_and_wait().await.unwrap();
        assert!(stream.is_closed());

        // Stream just groups
        // Sync bo first to avoid any spillover from the last stream
        bo.conversations().sync().await.unwrap();
        let stream_callback = Arc::new(RustStreamCallback::default());
        let stream = bo
            .conversations()
            .stream_groups(stream_callback.clone())
            .await;

        alix.conversations()
            .create_group(
                vec![bo.account_identifier.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();

        stream_callback.wait_for_delivery(Some(2)).await.unwrap();
        assert_eq!(stream_callback.message_count(), 1);

        alix.conversations()
            .find_or_create_dm(bo.account_identifier.clone(), FfiCreateDMOptions::default())
            .await
            .unwrap();
        let result = stream_callback.wait_for_delivery(Some(1)).await;
        assert!(result.is_err(), "Stream unexpectedly received a DM");
        assert_eq!(stream_callback.message_count(), 1);

        stream.end_and_wait().await.unwrap();
        assert!(stream.is_closed());

        // Stream just dms
        // Sync bo before opening the stream
        bo.conversations().sync().await.unwrap();
        let stream_callback = Arc::new(RustStreamCallback::default());
        let stream = bo.conversations().stream_dms(stream_callback.clone()).await;
        caro.conversations()
            .find_or_create_dm(bo.account_identifier.clone(), FfiCreateDMOptions::default())
            .await
            .unwrap();
        stream_callback.wait_for_delivery(Some(2)).await.unwrap();
        assert_eq!(stream_callback.message_count(), 1);

        alix.conversations()
            .create_group(
                vec![bo.account_identifier.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();

        let result = stream_callback.wait_for_delivery(Some(2)).await;
        assert!(result.is_err(), "Stream unexpectedly received a Group");
        assert_eq!(stream_callback.message_count(), 1);

        stream.end_and_wait().await.unwrap();
        assert!(stream.is_closed());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_stream_all_dm_messages() {
        let alix = Tester::new().await;
        let bo = Tester::new().await;
        let alix_dm = alix
            .conversations()
            .find_or_create_dm(bo.account_identifier.clone(), FfiCreateDMOptions::default())
            .await
            .unwrap();

        let alix_group = alix
            .conversations()
            .create_group(
                vec![bo.account_identifier.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();

        // Stream all conversations
        let stream_callback = Arc::new(RustStreamCallback::default());
        let stream = bo
            .conversations()
            .stream_all_messages(
                stream_callback.clone(),
                Some(vec![FfiConsentState::Allowed, FfiConsentState::Unknown]),
            )
            .await;
        stream.wait_for_ready().await;

        alix_group
            .send("first".as_bytes().to_vec(), FfiSendMessageOpts::default())
            .await
            .unwrap();
        stream_callback.wait_for_delivery(None).await.unwrap();
        assert_eq!(stream_callback.message_count(), 1);

        alix_dm
            .send("second".as_bytes().to_vec(), FfiSendMessageOpts::default())
            .await
            .unwrap();
        stream_callback.wait_for_delivery(None).await.unwrap();
        assert_eq!(stream_callback.message_count(), 2);

        stream.end_and_wait().await.unwrap();
        assert!(stream.is_closed());
        bo.conversations()
            .sync_all_conversations(None)
            .await
            .unwrap();
        // Stream just groups
        let stream_callback = Arc::new(RustStreamCallback::default());
        let stream = bo
            .conversations()
            .stream_all_group_messages(stream_callback.clone(), None)
            .await;
        stream.wait_for_ready().await;

        alix_group
            .send("first".as_bytes().to_vec(), FfiSendMessageOpts::default())
            .await
            .unwrap();
        stream_callback.wait_for_delivery(None).await.unwrap();
        assert_eq!(stream_callback.message_count(), 1);

        alix_dm
            .send("second".as_bytes().to_vec(), FfiSendMessageOpts::default())
            .await
            .unwrap();
        let result = stream_callback.wait_for_delivery(Some(2)).await;
        assert!(result.is_err(), "Stream unexpectedly received a DM message");
        assert_eq!(stream_callback.message_count(), 1);

        stream.end_and_wait().await.unwrap();
        assert!(stream.is_closed());

        bo.conversations()
            .sync_all_conversations(None)
            .await
            .unwrap();
        // Stream just dms
        let stream_callback = Arc::new(RustStreamCallback::default());
        let stream = bo
            .conversations()
            .stream_all_dm_messages(stream_callback.clone(), None)
            .await;
        stream.wait_for_ready().await;

        alix_dm
            .send("first".as_bytes().to_vec(), FfiSendMessageOpts::default())
            .await
            .unwrap();
        stream_callback.wait_for_delivery(None).await.unwrap();
        assert_eq!(stream_callback.message_count(), 1);

        alix_group
            .send("second".as_bytes().to_vec(), FfiSendMessageOpts::default())
            .await
            .unwrap();
        let result = stream_callback.wait_for_delivery(Some(2)).await;
        assert!(
            result.is_err(),
            "Stream unexpectedly received a Group message"
        );
        assert_eq!(stream_callback.message_count(), 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_long_messages() {
        let alix = Tester::new().await;
        let bo = Tester::new().await;

        let dm = alix
            .conversations()
            .find_or_create_dm_by_inbox_id(bo.inbox_id(), FfiCreateDMOptions::default())
            .await
            .unwrap();

        bo.conversations()
            .sync_all_conversations(None)
            .await
            .unwrap();

        let data = xmtp_common::rand_vec::<100000>();
        dm.send(data.clone(), FfiSendMessageOpts::default())
            .await
            .unwrap();

        let bo_dm = bo
            .conversations()
            .find_or_create_dm_by_inbox_id(alix.inbox_id(), FfiCreateDMOptions::default())
            .await
            .unwrap();

        bo_dm.sync().await.unwrap();
        let bo_msgs = bo_dm
            .find_messages(FfiListMessagesOptions::default())
            .await
            .unwrap();
        assert!(bo_msgs.iter().any(|msg| msg.content.eq(&data)));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_stream_consent() {
        let alix_a = Tester::builder().sync_worker().sync_server().build().await;

        let alix_b = alix_a.builder.build().await;

        let bo = Tester::new().await;

        // check that they have the same sync group
        alix_a
            .inner_client
            .test_has_same_sync_group_as(&alix_b.inner_client)
            .await
            .unwrap();
        alix_a
            .worker()
            .register_interest(SyncMetric::PayloadSent, 1)
            .wait()
            .await
            .unwrap();
        alix_a
            .worker()
            .register_interest(SyncMetric::HmacSent, 1)
            .wait()
            .await
            .unwrap();

        alix_b.sync_preferences().await.unwrap();
        alix_b
            .worker()
            .register_interest(SyncMetric::PayloadProcessed, 1)
            .wait()
            .await
            .unwrap();
        alix_a
            .inner_client
            .test_has_same_sync_group_as(&alix_b.inner_client)
            .await
            .unwrap();
        alix_b
            .worker()
            .register_interest(SyncMetric::HmacReceived, 1)
            .wait()
            .await
            .unwrap();

        // create a stream from both installations
        let stream_a_callback = Arc::new(RustStreamCallback::default());
        let stream_b_callback = Arc::new(RustStreamCallback::default());
        let a_stream = alix_a
            .conversations()
            .stream_consent(stream_a_callback.clone())
            .await;
        let b_stream = alix_b
            .conversations()
            .stream_consent(stream_b_callback.clone())
            .await;
        a_stream.wait_for_ready().await;
        b_stream.wait_for_ready().await;
        alix_b.sync_preferences().await.unwrap();

        // consent with bo
        alix_a
            .set_consent_states(vec![FfiConsent {
                entity: bo.inbox_id(),
                entity_type: FfiConsentEntityType::InboxId,
                state: FfiConsentState::Denied,
            }])
            .await
            .unwrap();

        // Wait for alix_a to send the consent sync out
        alix_a
            .worker()
            .register_interest(SyncMetric::ConsentSent, 1)
            .wait()
            .await
            .unwrap();

        // Have alix_b sync the sync group and wait for the new consent to be processed
        alix_b.sync_preferences().await.unwrap();
        alix_b
            .worker()
            .register_interest(SyncMetric::ConsentReceived, 1)
            .wait()
            .await
            .unwrap();

        stream_a_callback.wait_for_delivery(Some(3)).await.unwrap();
        wait_for_ok(|| async {
            alix_b.sync_preferences().await.unwrap();
            stream_b_callback.wait_for_delivery(Some(1)).await
        })
        .await
        .unwrap();

        wait_for_eq(|| async { stream_a_callback.consent_updates_count() }, 1)
            .await
            .unwrap();
        wait_for_eq(|| async { stream_a_callback.consent_updates_count() }, 1)
            .await
            .unwrap();

        // Consent should be the same
        let consent_a = alix_a
            .get_consent_state(FfiConsentEntityType::InboxId, bo.inbox_id())
            .await
            .unwrap();
        let consent_b = alix_b
            .get_consent_state(FfiConsentEntityType::InboxId, bo.inbox_id())
            .await
            .unwrap();
        assert_eq!(consent_a, consent_b);

        // Now we'll allow Bo
        alix_a
            .set_consent_states(vec![FfiConsent {
                entity: bo.inbox_id(),
                entity_type: FfiConsentEntityType::InboxId,
                state: FfiConsentState::Allowed,
            }])
            .await
            .unwrap();

        // Wait for alix_a to send out the consent on the sync group
        alix_a
            .worker()
            .register_interest(SyncMetric::ConsentSent, 3)
            .wait()
            .await
            .unwrap();
        // Have alix_b sync the sync group
        alix_b.sync_preferences().await.unwrap();
        // Wait for alix_b to process the new consent
        alix_b
            .worker()
            .register_interest(SyncMetric::ConsentReceived, 2)
            .wait()
            .await
            .unwrap();

        // This consent should stream
        wait_for_ge(|| async { stream_a_callback.consent_updates_count() }, 2)
            .await
            .unwrap();

        // alix_b should now be ALLOWED with bo via device sync
        let consent_b = alix_b
            .get_consent_state(FfiConsentEntityType::InboxId, bo.inbox_id())
            .await
            .unwrap();
        assert_eq!(consent_b, FfiConsentState::Allowed);

        a_stream.end_and_wait().await.unwrap();
        b_stream.end_and_wait().await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_stream_preferences() {
        let alix_wallet = generate_local_wallet();
        let alix_a_span = info_span!("alix_a");
        let alix_a = Tester::builder()
            .owner(alix_wallet.clone())
            .sync_worker()
            .with_name("alix_a")
            .build()
            .instrument(alix_a_span)
            .await;
        let alix_b_span = info_span!("alix_b");
        let alix_b = Tester::builder()
            .owner(alix_wallet)
            .sync_worker()
            .with_name("alix_b")
            .build()
            .instrument(alix_b_span)
            .await;

        let hmac_sent = alix_a.worker().register_interest(SyncMetric::HmacSent, 1);
        let hmac_received = alix_b
            .worker()
            .register_interest(SyncMetric::HmacReceived, 1);

        let cb = RustStreamCallback::default();
        let notify = cb.enable_notifications();
        tokio::pin!(notify);
        notify.as_mut().enable();

        let stream_b_callback = Arc::new(cb);
        let b_stream = alix_b
            .conversations()
            .stream_preferences(stream_b_callback.clone())
            .await;

        b_stream.wait_for_ready().await;

        alix_a
            .inner_client
            .test_has_same_sync_group_as(&alix_b.inner_client)
            .await
            .unwrap();

        hmac_sent.wait().await.unwrap();
        alix_b.sync_preferences().await.unwrap();
        hmac_received.wait().await.unwrap();

        let result = tokio::time::timeout(std::time::Duration::from_secs(10), notify).await;
        assert!(result.is_ok());

        {
            let updates = stream_b_callback.preference_updates.lock();
            assert!(
                updates
                    .iter()
                    .any(|u| matches!(u, FfiPreferenceUpdate::HMAC { .. }))
            );
        }

        b_stream.end_and_wait().await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_get_hmac_keys() {
        let alix = new_test_client().await;
        let bo = new_test_client().await;

        let alix_group = alix
            .conversations()
            .create_group(
                vec![bo.account_identifier.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();

        let hmac_keys = alix_group.get_hmac_keys().unwrap();

        let keys = hmac_keys.get(&alix_group.id()).unwrap();

        assert!(!keys.is_empty());
        assert_eq!(keys.len(), 3);

        for value in keys {
            assert!(!value.key.is_empty());
            assert_eq!(value.key.len(), 42);
            assert!(value.epoch >= 1);
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_set_and_get_group_consent() {
        let alix = new_test_client().await;
        let bo = new_test_client().await;

        let alix_group = alix
            .conversations()
            .create_group(
                vec![bo.account_identifier.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();

        let alix_initial_consent = alix_group.consent_state().unwrap();
        assert_eq!(alix_initial_consent, FfiConsentState::Allowed);

        bo.conversations().sync().await.unwrap();
        let bo_group = bo.conversation(alix_group.id()).unwrap();

        let bo_initial_consent = bo_group.consent_state().unwrap();
        assert_eq!(bo_initial_consent, FfiConsentState::Unknown);

        alix_group
            .update_consent_state(FfiConsentState::Denied)
            .unwrap();
        let alix_updated_consent = alix_group.consent_state().unwrap();
        assert_eq!(alix_updated_consent, FfiConsentState::Denied);
        bo.set_consent_states(vec![FfiConsent {
            state: FfiConsentState::Allowed,
            entity_type: FfiConsentEntityType::ConversationId,
            entity: hex::encode(bo_group.id()),
        }])
        .await
        .unwrap();
        let bo_updated_consent = bo_group.consent_state().unwrap();
        assert_eq!(bo_updated_consent, FfiConsentState::Allowed);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_set_and_get_dm_consent() {
        let alix = new_test_client().await;
        let bo = new_test_client().await;

        let alix_dm = alix
            .conversations()
            .find_or_create_dm(bo.account_identifier.clone(), FfiCreateDMOptions::default())
            .await
            .unwrap();

        let alix_initial_consent = alix_dm.consent_state().unwrap();
        assert_eq!(alix_initial_consent, FfiConsentState::Allowed);

        bo.conversations().sync().await.unwrap();
        let bo_dm = bo.conversation(alix_dm.id()).unwrap();

        let bo_initial_consent = bo_dm.consent_state().unwrap();
        assert_eq!(bo_initial_consent, FfiConsentState::Unknown);

        alix_dm
            .update_consent_state(FfiConsentState::Denied)
            .unwrap();
        let alix_updated_consent = alix_dm.consent_state().unwrap();
        assert_eq!(alix_updated_consent, FfiConsentState::Denied);
        bo.set_consent_states(vec![FfiConsent {
            state: FfiConsentState::Allowed,
            entity_type: FfiConsentEntityType::ConversationId,
            entity: hex::encode(bo_dm.id()),
        }])
        .await
        .unwrap();
        let bo_updated_consent = bo_dm.consent_state().unwrap();
        assert_eq!(bo_updated_consent, FfiConsentState::Allowed);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_get_dm_peer_inbox_id() {
        let alix = new_test_client().await;
        let bo = new_test_client().await;

        let alix_dm = alix
            .conversations()
            .find_or_create_dm(bo.account_identifier.clone(), FfiCreateDMOptions::default())
            .await
            .unwrap();

        let alix_dm_peer_inbox = alix_dm.dm_peer_inbox_id().unwrap();
        assert_eq!(alix_dm_peer_inbox, bo.inbox_id());

        bo.conversations().sync().await.unwrap();
        let bo_dm = bo.conversation(alix_dm.id()).unwrap();

        let bo_dm_peer_inbox = bo_dm.dm_peer_inbox_id().unwrap();
        assert_eq!(bo_dm_peer_inbox, alix.inbox_id());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_set_and_get_member_consent() {
        let alix = new_test_client().await;
        let bo = new_test_client().await;

        let alix_group = alix
            .conversations()
            .create_group(
                vec![bo.account_identifier.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();
        alix.set_consent_states(vec![FfiConsent {
            state: FfiConsentState::Allowed,
            entity_type: FfiConsentEntityType::InboxId,
            entity: bo.inbox_id(),
        }])
        .await
        .unwrap();
        let bo_consent = alix
            .get_consent_state(FfiConsentEntityType::InboxId, bo.inbox_id())
            .await
            .unwrap();
        assert_eq!(bo_consent, FfiConsentState::Allowed);

        if let Some(member) = alix_group
            .list_members()
            .await
            .unwrap()
            .iter()
            .find(|&m| m.inbox_id == bo.inbox_id())
        {
            assert_eq!(member.consent_state, FfiConsentState::Allowed);
        } else {
            panic!("Error: No member found with the given inbox_id.");
        }
    }

    // Groups contain membership updates, but dms do not
    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_dm_first_messages() {
        let alix = new_test_client().await;
        let bo = new_test_client().await;

        // Alix creates DM with Bo
        let alix_dm = alix
            .conversations()
            .find_or_create_dm(bo.account_identifier.clone(), FfiCreateDMOptions::default())
            .await
            .unwrap();

        // Alix creates group with Bo
        let alix_group = alix
            .conversations()
            .create_group(
                vec![bo.account_identifier.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();

        // Bo syncs to get both conversations
        bo.conversations().sync().await.unwrap();
        let bo_dm = bo.conversation(alix_dm.id()).unwrap();
        let bo_group = bo.conversation(alix_group.id()).unwrap();

        // Alix sends messages in both conversations
        alix_dm
            .send(
                "Hello in DM".as_bytes().to_vec(),
                FfiSendMessageOpts::default(),
            )
            .await
            .unwrap();
        alix_group
            .send(
                "Hello in group".as_bytes().to_vec(),
                FfiSendMessageOpts::default(),
            )
            .await
            .unwrap();

        // Bo syncs the dm and the group
        bo_dm.sync().await.unwrap();
        bo_group.sync().await.unwrap();

        // Get messages for both participants in both conversations
        let alix_dm_messages = alix_dm
            .find_messages(FfiListMessagesOptions::default())
            .await
            .unwrap();
        let bo_dm_messages = bo_dm
            .find_messages(FfiListMessagesOptions::default())
            .await
            .unwrap();
        let alix_group_messages = alix_group
            .find_messages(FfiListMessagesOptions::default())
            .await
            .unwrap();
        let bo_group_messages = bo_group
            .find_messages(FfiListMessagesOptions::default())
            .await
            .unwrap();

        // Verify DM messages
        assert_eq!(alix_dm_messages.len(), 2);
        assert_eq!(bo_dm_messages.len(), 2);
        assert_eq!(
            String::from_utf8_lossy(&alix_dm_messages[1].content),
            "Hello in DM"
        );
        assert_eq!(
            String::from_utf8_lossy(&bo_dm_messages[1].content),
            "Hello in DM"
        );

        // Verify group messages
        assert_eq!(alix_group_messages.len(), 2);
        assert_eq!(bo_group_messages.len(), 2);
        assert_eq!(
            String::from_utf8_lossy(&alix_group_messages[1].content),
            "Hello in group"
        );
        assert_eq!(
            String::from_utf8_lossy(&bo_group_messages[1].content),
            "Hello in group"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_can_not_create_new_inbox_id_with_already_associated_wallet() {
        // Step 1: Generate wallet A
        let wallet_a = generate_local_wallet();
        let ident_a = wallet_a.identifier();

        // Step 2: Use wallet A to create a new client with a new inbox id derived from wallet A
        let wallet_a_inbox_id = ident_a.inbox_id(1).unwrap();
        let ffi_ident: FfiIdentifier = wallet_a.identifier().into();
        let client_a = create_client(
            connect_to_backend_test().await,
            connect_to_backend_test().await,
            Some(tmp_path()),
            Some(xmtp_db::EncryptedMessageStore::<()>::generate_enc_key().into()),
            &wallet_a_inbox_id,
            ffi_ident,
            1,
            None,
            Some(HISTORY_SYNC_URL.to_string()),
            None,
            None,
            None,
        )
        .await
        .unwrap();
        let ffi_inbox_owner = FfiWalletInboxOwner::with_wallet(wallet_a.clone());
        register_client_with_wallet(&ffi_inbox_owner, &client_a).await;

        // Step 3: Generate wallet B
        let wallet_b = generate_local_wallet();
        let wallet_b_ident = wallet_b.identifier();

        // Step 4: Associate wallet B to inbox A
        let add_wallet_signature_request = client_a
            .add_identity(wallet_b.identifier().into())
            .await
            .expect("could not add wallet");
        add_wallet_signature_request
            .add_wallet_signature(&wallet_b)
            .await;
        client_a
            .apply_signature_request(add_wallet_signature_request)
            .await
            .unwrap();

        // Verify that we can now use wallet B to create a new client that has inbox_id == client_a.inbox_id
        let nonce = 1;
        let inbox_id = client_a.inbox_id();

        let ffi_ident: FfiIdentifier = wallet_b.identifier().into();
        let client_b = create_client(
            connect_to_backend_test().await,
            connect_to_backend_test().await,
            Some(tmp_path()),
            Some(xmtp_db::EncryptedMessageStore::<()>::generate_enc_key().into()),
            &inbox_id,
            ffi_ident,
            nonce,
            None,
            Some(HISTORY_SYNC_URL.to_string()),
            None,
            None,
            None,
        )
        .await
        .unwrap();
        let ffi_inbox_owner = FfiWalletInboxOwner::with_wallet(wallet_b.clone());
        register_client_with_wallet(&ffi_inbox_owner, &client_b).await;

        assert!(client_b.inbox_id() == client_a.inbox_id());

        // Verify both clients can receive messages for inbox_id == client_a.inbox_id
        let bo = new_test_client().await;

        // Alix creates DM with Bo
        let bo_dm = bo
            .conversations()
            .find_or_create_dm(wallet_a.identifier().into(), FfiCreateDMOptions::default())
            .await
            .unwrap();

        bo_dm
            .send(
                "Hello in DM".as_bytes().to_vec(),
                FfiSendMessageOpts::default(),
            )
            .await
            .unwrap();

        // Verify that client_a and client_b received the dm message to wallet a address
        client_a
            .conversations()
            .sync_all_conversations(None)
            .await
            .unwrap();
        client_b
            .conversations()
            .sync_all_conversations(None)
            .await
            .unwrap();
        bo.conversations()
            .sync_all_conversations(None)
            .await
            .unwrap();

        let a_dm_messages = client_a
            .conversations()
            .list(FfiListConversationsOptions::default())
            .unwrap()[0]
            .conversation
            .find_messages(FfiListMessagesOptions::default())
            .await
            .unwrap();
        let b_dm_messages = client_b
            .conversations()
            .list(FfiListConversationsOptions::default())
            .unwrap()[0]
            .conversation
            .find_messages(FfiListMessagesOptions::default())
            .await
            .unwrap();
        let bo_dm_messages = bo
            .conversations()
            .list(FfiListConversationsOptions::default())
            .unwrap()[0]
            .conversation
            .find_messages(FfiListMessagesOptions::default())
            .await
            .unwrap();
        assert_eq!(a_dm_messages[1].content, "Hello in DM".as_bytes());
        assert_eq!(b_dm_messages[1].content, "Hello in DM".as_bytes());
        assert_eq!(bo_dm_messages[1].content, "Hello in DM".as_bytes());

        let client_b_inbox_id = wallet_b_ident.inbox_id(nonce).unwrap();
        let ffi_ident: FfiIdentifier = wallet_b.identifier().into();
        let client_b_new_result = create_client(
            connect_to_backend_test().await,
            connect_to_backend_test().await,
            Some(tmp_path()),
            Some(xmtp_db::EncryptedMessageStore::<()>::generate_enc_key().into()),
            &client_b_inbox_id,
            ffi_ident,
            nonce,
            None,
            Some(HISTORY_SYNC_URL.to_string()),
            None,
            None,
            None,
        )
        .await;

        // Client creation for b now fails since wallet b is already associated with inbox a
        match client_b_new_result {
            Err(err) => {
                println!("Error returned: {:?}", err);
                assert_eq!(
                    err.to_string(),
                    "Client builder error: error creating new identity: Inbox ID mismatch"
                        .to_string()
                );
            }
            Ok(_) => panic!("Expected an error, but got Ok"),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_wallet_b_cannot_create_new_client_for_inbox_b_after_association() {
        // Step 1: Wallet A creates a new client with inbox_id A
        let wallet_a = generate_local_wallet();
        let ident_a = wallet_a.identifier();
        let wallet_a_inbox_id = ident_a.inbox_id(1).unwrap();
        let ffi_ident: FfiIdentifier = wallet_a.identifier().into();
        let client_a = create_client(
            connect_to_backend_test().await,
            connect_to_backend_test().await,
            Some(tmp_path()),
            Some(xmtp_db::EncryptedMessageStore::<()>::generate_enc_key().into()),
            &wallet_a_inbox_id,
            ffi_ident,
            1,
            None,
            Some(HISTORY_SYNC_URL.to_string()),
            None,
            None,
            None,
        )
        .await
        .unwrap();
        let ffi_inbox_owner_a = FfiWalletInboxOwner::with_wallet(wallet_a.clone());
        register_client_with_wallet(&ffi_inbox_owner_a, &client_a).await;

        // Step 2: Wallet B creates a new client with inbox_id B
        let wallet_b = generate_local_wallet();
        let ident_b = wallet_b.identifier();
        let wallet_b_inbox_id = ident_b.inbox_id(1).unwrap();
        let ffi_ident: FfiIdentifier = wallet_b.identifier().into();
        let client_b1 = create_client(
            connect_to_backend_test().await,
            connect_to_backend_test().await,
            Some(tmp_path()),
            Some(xmtp_db::EncryptedMessageStore::<()>::generate_enc_key().into()),
            &wallet_b_inbox_id,
            ffi_ident,
            1,
            None,
            Some(HISTORY_SYNC_URL.to_string()),
            None,
            None,
            None,
        )
        .await
        .unwrap();
        let ffi_inbox_owner_b1 = FfiWalletInboxOwner::with_wallet(wallet_b.clone());
        register_client_with_wallet(&ffi_inbox_owner_b1, &client_b1).await;

        // Step 3: Wallet B creates a second client for inbox_id B
        let ffi_ident: FfiIdentifier = wallet_b.identifier().into();
        let _client_b2 = create_client(
            connect_to_backend_test().await,
            connect_to_backend_test().await,
            Some(tmp_path()),
            Some(xmtp_db::EncryptedMessageStore::<()>::generate_enc_key().into()),
            &wallet_b_inbox_id,
            ffi_ident,
            1,
            None,
            Some(HISTORY_SYNC_URL.to_string()),
            None,
            None,
            None,
        )
        .await
        .unwrap();

        // Step 4: Client A adds association to wallet B
        let add_wallet_signature_request = client_a
            .add_identity(wallet_b.identifier().into())
            .await
            .expect("could not add wallet");
        add_wallet_signature_request
            .add_wallet_signature(&wallet_b)
            .await;
        client_a
            .apply_signature_request(add_wallet_signature_request)
            .await
            .unwrap();

        // Step 5: Wallet B tries to create another new client for inbox_id B, but it fails
        let ffi_ident: FfiIdentifier = wallet_b.identifier().into();
        let client_b3 = create_client(
            connect_to_backend_test().await,
            connect_to_backend_test().await,
            Some(tmp_path()),
            Some(xmtp_db::EncryptedMessageStore::<()>::generate_enc_key().into()),
            &wallet_b_inbox_id,
            ffi_ident,
            1,
            None,
            Some(HISTORY_SYNC_URL.to_string()),
            None,
            None,
            None,
        )
        .await;

        // Client creation for b now fails since wallet b is already associated with inbox a
        match client_b3 {
            Err(err) => {
                println!("Error returned: {:?}", err);
                assert_eq!(
                    err.to_string(),
                    "Client builder error: error creating new identity: Inbox ID mismatch"
                        .to_string()
                );
            }
            Ok(_) => panic!("Expected an error, but got Ok"),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_can_list_messages_with_content_types() {
        // Create test clients
        let alix = new_test_client().await;
        let bo = new_test_client().await;

        // Alix creates group with Bo
        let alix_group = alix
            .conversations()
            .create_group(
                vec![bo.account_identifier.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();

        // Bo syncs to get the group
        bo.conversations().sync().await.unwrap();
        let bo_group = bo.conversation(alix_group.id()).unwrap();

        // Alix sends first message
        alix_group
            .send("hey".as_bytes().to_vec(), FfiSendMessageOpts::default())
            .await
            .unwrap();

        // Bo syncs and responds
        bo_group.sync().await.unwrap();
        let bo_message_response = TextCodec::encode("hey alix".to_string()).unwrap();
        let mut buf = Vec::new();
        bo_message_response.encode(&mut buf).unwrap();
        bo_group
            .send(buf, FfiSendMessageOpts::default())
            .await
            .unwrap();

        // Bo sends read receipt
        let read_receipt_content_id = ContentTypeId {
            authority_id: "xmtp.org".to_string(),
            type_id: ReadReceiptCodec::TYPE_ID.to_string(),
            version_major: 1,
            version_minor: 0,
        };
        let read_receipt_encoded_content = EncodedContent {
            r#type: Some(read_receipt_content_id),
            parameters: HashMap::new(),
            fallback: None,
            compression: None,
            content: vec![],
        };

        let mut buf = Vec::new();
        read_receipt_encoded_content.encode(&mut buf).unwrap();
        bo_group
            .send(buf, FfiSendMessageOpts::default())
            .await
            .unwrap();

        // Alix syncs and gets all messages
        alix_group.sync().await.unwrap();
        let latest_message = alix_group
            // ... existing code ...
            .find_messages(FfiListMessagesOptions {
                direction: Some(FfiDirection::Descending),
                limit: Some(1),
                ..Default::default()
            })
            .await
            .unwrap();

        // Verify last message is the read receipt
        assert_eq!(latest_message.len(), 1);
        let latest_message_encoded_content =
            EncodedContent::decode(latest_message.last().unwrap().content.clone().as_slice())
                .unwrap();
        assert_eq!(
            latest_message_encoded_content.r#type.unwrap().type_id,
            "readReceipt"
        );

        // Get only text messages
        let text_messages = alix_group
            .find_messages(FfiListMessagesOptions {
                content_types: Some(vec![FfiContentType::Text]),
                direction: Some(FfiDirection::Descending),
                limit: Some(1),
                ..Default::default()
            })
            .await
            .unwrap();

        // Verify last message is "hey alix" when filtered
        assert_eq!(text_messages.len(), 1);
        let latest_message_encoded_content =
            EncodedContent::decode(text_messages.last().unwrap().content.clone().as_slice())
                .unwrap();
        let text_message = TextCodec::decode(latest_message_encoded_content).unwrap();
        assert_eq!(text_message, "hey alix");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_can_send_and_receive_reaction() {
        // Create two test clients
        let alix = new_test_client().await;
        let bo = new_test_client().await;

        // Create a conversation between them
        let alix_conversation = alix
            .conversations()
            .create_group(
                vec![bo.account_identifier.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();
        // Send initial message to react to
        let mut buf = Vec::new();
        TextCodec::encode("Hello world".to_string())
            .unwrap()
            .encode(&mut buf)
            .unwrap();
        alix_conversation
            .send(buf, FfiSendMessageOpts::default())
            .await
            .unwrap();

        // Have Bo sync to get the conversation and message
        bo.conversations().sync().await.unwrap();
        let bo_conversation = bo.conversation(alix_conversation.id()).unwrap();
        bo_conversation.sync().await.unwrap();

        // Get the message to react to
        let messages = bo_conversation
            .find_messages(FfiListMessagesOptions::default())
            .await
            .unwrap();
        let message_to_react_to = &messages[1];

        // Create and send reaction
        let ffi_reaction = FfiReactionPayload {
            reference: hex::encode(message_to_react_to.id.clone()),
            reference_inbox_id: alix.inbox_id(),
            action: FfiReactionAction::Added,
            content: "👍".to_string(),
            schema: FfiReactionSchema::Unicode,
        };
        let bytes_to_send = encode_reaction(ffi_reaction).unwrap();
        bo_conversation
            .send(bytes_to_send, FfiSendMessageOpts::default())
            .await
            .unwrap();

        // Have Alix sync to get the reaction
        alix_conversation.sync().await.unwrap();

        // Get reactions for the original message
        let messages = alix_conversation
            .find_messages(FfiListMessagesOptions::default())
            .await
            .unwrap();

        // Verify reaction details
        assert_eq!(messages.len(), 3);
        let received_reaction = &messages[2];
        let message_content = received_reaction.content.clone();
        let reaction = decode_reaction(message_content).unwrap();
        assert_eq!(reaction.content, "👍");
        assert_eq!(reaction.action, FfiReactionAction::Added);
        assert_eq!(reaction.reference_inbox_id, alix.inbox_id());
        assert_eq!(
            reaction.reference,
            hex::encode(message_to_react_to.id.clone())
        );
        assert_eq!(reaction.schema, FfiReactionSchema::Unicode);

        // Test find_messages_with_reactions query
        let messages_with_reactions: Vec<FfiMessageWithReactions> = alix_conversation
            .find_messages_with_reactions(FfiListMessagesOptions::default())
            .unwrap();
        assert_eq!(messages_with_reactions.len(), 2);
        let message_with_reactions = &messages_with_reactions[1];
        assert_eq!(message_with_reactions.reactions.len(), 1);
        let message_content = message_with_reactions.reactions[0].content.clone();
        let slice: &[u8] = message_content.as_slice();
        let encoded_content = EncodedContent::decode(slice).unwrap();
        let reaction = ReactionV2::decode(encoded_content.content.as_slice()).unwrap();
        assert_eq!(reaction.content, "👍");
        assert_eq!(reaction.action, ReactionAction::Added as i32);
        assert_eq!(reaction.reference_inbox_id, alix.inbox_id());
        assert_eq!(
            reaction.reference,
            hex::encode(message_to_react_to.id.clone())
        );
        assert_eq!(reaction.schema, ReactionSchema::Unicode as i32);
    }

    #[tokio::test]
    async fn test_reaction_encode_decode() {
        // Create a test reaction
        let original_reaction = FfiReactionPayload {
            reference: "123abc".to_string(),
            reference_inbox_id: "test_inbox_id".to_string(),
            action: FfiReactionAction::Added,
            content: "👍".to_string(),
            schema: FfiReactionSchema::Unicode,
        };

        // Encode the reaction
        let encoded_bytes = encode_reaction(original_reaction.clone())
            .expect("Should encode reaction successfully");

        // Decode the reaction
        let decoded_reaction =
            decode_reaction(encoded_bytes).expect("Should decode reaction successfully");

        // Verify the decoded reaction matches the original
        assert_eq!(decoded_reaction.reference, original_reaction.reference);
        assert_eq!(
            decoded_reaction.reference_inbox_id,
            original_reaction.reference_inbox_id
        );
        assert!(matches!(decoded_reaction.action, FfiReactionAction::Added));
        assert_eq!(decoded_reaction.content, original_reaction.content);
        assert!(matches!(
            decoded_reaction.schema,
            FfiReactionSchema::Unicode
        ));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_update_policies_empty_group() {
        let amal = new_test_client().await;
        let bola = new_test_client().await;

        // Create a group with amal and bola with admin-only permissions
        let admin_only_options = FfiCreateGroupOptions {
            permissions: Some(FfiGroupPermissionsOptions::AdminOnly),
            ..Default::default()
        };
        let amal_group = amal
            .conversations()
            .create_group(
                vec![bola.account_identifier.clone()],
                admin_only_options.clone(),
            )
            .await
            .unwrap();

        // Verify we can update the group name without syncing first
        amal_group
            .update_group_name("New Group Name 1".to_string())
            .await
            .unwrap();

        // Verify the name is updated
        amal_group.sync().await.unwrap();
        assert_eq!(amal_group.group_name().unwrap(), "New Group Name 1");

        // Create a group with just amal
        let amal_solo_group = amal
            .conversations()
            .create_group(vec![], admin_only_options)
            .await
            .unwrap();

        // Verify we can update the group name
        amal_solo_group
            .update_group_name("New Group Name 2".to_string())
            .await
            .unwrap();

        // Verify the name is updated
        amal_solo_group.sync().await.unwrap();
        assert_eq!(amal_solo_group.group_name().unwrap(), "New Group Name 2");
    }

    #[tokio::test]
    async fn test_find_or_create_dm() {
        // Create two test users
        let wallet1 = generate_local_wallet();
        let wallet2 = generate_local_wallet();

        let client1 = new_test_client_with_wallet(wallet1).await;
        let client2 = new_test_client_with_wallet(wallet2).await;

        // Test find_or_create_dm_by_inbox_id
        let inbox_id2 = client2.inbox_id();
        let dm_by_inbox = client1
            .conversations()
            .find_or_create_dm_by_inbox_id(inbox_id2, FfiCreateDMOptions::default())
            .await
            .expect("Should create DM with inbox ID");

        // Verify conversation appears in DM list
        let dms = client1
            .conversations()
            .list_dms(FfiListConversationsOptions::default())
            .unwrap();
        assert_eq!(dms.len(), 1, "Should have one DM conversation");
        assert_eq!(
            dms[0].conversation.id(),
            dm_by_inbox.id(),
            "Listed DM should match created DM"
        );

        // Sync both clients
        client1.conversations().sync().await.unwrap();
        client2.conversations().sync().await.unwrap();

        // First client tries to create another DM with the same inbox id
        let dm_by_inbox2 = client1
            .conversations()
            .find_or_create_dm_by_inbox_id(client2.inbox_id(), FfiCreateDMOptions::default())
            .await
            .unwrap();

        // Sync both clients
        client1.conversations().sync().await.unwrap();
        client2.conversations().sync().await.unwrap();

        // Id should be the same as the existing DM and the num of dms should still be 1
        assert_eq!(
            dm_by_inbox2.id(),
            dm_by_inbox.id(),
            "New DM should match existing DM"
        );
        let dms = client1
            .conversations()
            .list_dms(FfiListConversationsOptions::default())
            .unwrap();
        assert_eq!(dms.len(), 1, "Should still have one DM conversation");

        // Second client tries to create a DM with the client 1 inbox id
        let dm_by_inbox3 = client2
            .conversations()
            .find_or_create_dm_by_inbox_id(client1.inbox_id(), FfiCreateDMOptions::default())
            .await
            .unwrap();

        // Sync both clients
        client1.conversations().sync().await.unwrap();
        client2.conversations().sync().await.unwrap();

        // Id should be the same as the existing DM and the num of dms should still be 1
        assert_eq!(
            dm_by_inbox3.id(),
            dm_by_inbox.id(),
            "New DM should match existing DM"
        );
        let dms = client2
            .conversations()
            .list_dms(FfiListConversationsOptions::default())
            .unwrap();
        assert_eq!(dms.len(), 1, "Should still have one DM conversation");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_can_stream_and_receive_metadata_update() {
        // Create test clients
        let alix = new_test_client().await;
        let bo = new_test_client().await;

        // If we comment out this stream, the test passes
        let stream_callback = Arc::new(RustStreamCallback::default());
        let stream = bo
            .conversations()
            .stream_all_messages(stream_callback.clone(), None)
            .await;
        stream.wait_for_ready().await;

        // Create group and perform actions
        let alix_group = alix
            .conversations()
            .create_group(
                vec![bo.account_identifier.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();

        // Send first message
        let mut buf = Vec::new();
        TextCodec::encode("hello1".to_string())
            .unwrap()
            .encode(&mut buf)
            .unwrap();
        alix_group
            .send(buf, FfiSendMessageOpts::default())
            .await
            .unwrap();

        // Update group name
        alix_group
            .update_group_name("hello".to_string())
            .await
            .unwrap();

        // Send second message
        let mut buf = Vec::new();
        TextCodec::encode("hello2".to_string())
            .unwrap()
            .encode(&mut buf)
            .unwrap();
        alix_group
            .send(buf, FfiSendMessageOpts::default())
            .await
            .unwrap();

        // Sync Bo's client
        bo.conversations().sync().await.unwrap();

        // Get Bo's groups and verify count
        let bo_groups = bo
            .conversations()
            .list_groups(FfiListConversationsOptions::default())
            .unwrap();
        assert_eq!(bo_groups.len(), 1);
        let bo_group = bo_groups[0].conversation.clone();

        // Sync both groups
        bo_group.sync().await.unwrap();
        alix_group.sync().await.unwrap();

        // Get Bo's messages and verify content types
        let bo_messages = bo_group
            .find_messages(FfiListMessagesOptions::default())
            .await
            .unwrap();
        assert_eq!(bo_messages.len(), 4);

        // Verify message content types
        let message_types: Vec<String> = bo_messages
            .iter()
            .map(|msg| {
                let encoded_content = EncodedContent::decode(msg.content.as_slice()).unwrap();
                encoded_content.r#type.unwrap().type_id
            })
            .collect();

        assert_eq!(message_types[0], "group_updated");
        assert_eq!(message_types[1], "text");
        assert_eq!(message_types[2], "group_updated");
        assert_eq!(message_types[3], "text");

        assert_eq!(alix_group.group_name().unwrap(), "hello");
        // this assertion will also fail
        assert_eq!(bo_group.group_name().unwrap(), "hello");

        // Clean up stream
        stream.end_and_wait().await.unwrap();
    }

    #[tokio::test]
    async fn test_multi_remote_attachment_encode_decode() {
        // Create a test attachment
        let original_attachment = FfiMultiRemoteAttachment {
            attachments: vec![
                FfiRemoteAttachmentInfo {
                    filename: Some("test1.jpg".to_string()),
                    content_length: Some(1000),
                    secret: vec![1, 2, 3],
                    content_digest: "123".to_string(),
                    nonce: vec![7, 8, 9],
                    salt: vec![1, 2, 3],
                    scheme: "https".to_string(),
                    url: "https://example.com/test1.jpg".to_string(),
                },
                FfiRemoteAttachmentInfo {
                    filename: Some("test2.pdf".to_string()),
                    content_length: Some(2000),
                    secret: vec![4, 5, 6],
                    content_digest: "456".to_string(),
                    nonce: vec![10, 11, 12],
                    salt: vec![1, 2, 3],
                    scheme: "https".to_string(),
                    url: "https://example.com/test2.pdf".to_string(),
                },
            ],
        };

        // Encode the attachment
        let encoded_bytes = encode_multi_remote_attachment(original_attachment.clone())
            .expect("Should encode multi remote attachment successfully");

        // Decode the attachment
        let decoded_attachment = decode_multi_remote_attachment(encoded_bytes)
            .expect("Should decode multi remote attachment successfully");

        assert_eq!(
            decoded_attachment.attachments.len(),
            original_attachment.attachments.len()
        );

        for (decoded, original) in decoded_attachment
            .attachments
            .iter()
            .zip(original_attachment.attachments.iter())
        {
            assert_eq!(decoded.filename, original.filename);
            assert_eq!(decoded.content_digest, original.content_digest);
            assert_eq!(decoded.nonce, original.nonce);
            assert_eq!(decoded.scheme, original.scheme);
            assert_eq!(decoded.url, original.url);
        }
    }

    #[tokio::test]
    async fn test_transaction_reference_roundtrip() {
        let original = FfiTransactionReference {
            namespace: Some("eip155".to_string()),
            network_id: "1".to_string(),
            reference: "0xabc123".to_string(),
            metadata: Some(FfiTransactionMetadata {
                transaction_type: "transfer".to_string(),
                currency: "ETH".to_string(),
                amount: 0.42,
                decimals: 18,
                from_address: "0xfrom".to_string(),
                to_address: "0xto".to_string(),
            }),
        };

        let encoded = encode_transaction_reference(original.clone()).unwrap();
        let decoded = decode_transaction_reference(encoded).unwrap();

        assert_eq!(original.reference, decoded.reference);
        assert_eq!(
            original.metadata.as_ref().unwrap().currency,
            decoded.metadata.as_ref().unwrap().currency
        );
    }

    #[tokio::test]
    async fn test_attachment_roundtrip() {
        let original = FfiAttachment {
            filename: Some("test.txt".to_string()),
            mime_type: "text/plain".to_string(),
            content: "Hello, World!".as_bytes().to_vec(),
        };

        let encoded = encode_attachment(original.clone()).unwrap();
        let decoded = decode_attachment(encoded).unwrap();

        assert_eq!(original.filename, decoded.filename);
        assert_eq!(original.mime_type, decoded.mime_type);
        assert_eq!(original.content, decoded.content);
    }

    #[tokio::test]
    async fn test_reply_roundtrip() {
        let original = FfiReply {
            reference: "0x1234567890abcdef".to_string(),
            reference_inbox_id: Some("test_inbox_id".to_string()),
            content: FfiEncodedContent {
                type_id: None,
                parameters: HashMap::new(),
                fallback: Some("This is a reply".to_string()),
                compression: None,
                content: b"This is a reply".to_vec(),
            },
        };

        let encoded = encode_reply(original.clone()).unwrap();
        let decoded = decode_reply(encoded).unwrap();

        assert_eq!(original.reference, decoded.reference);
        assert_eq!(original.reference_inbox_id, decoded.reference_inbox_id);
        assert_eq!(original.content, decoded.content);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_find_enriched_messages_with_reactions() {
        let alix = Tester::new().await;
        let bo = Tester::new().await;

        // Create a group with both participants
        let alix_group = alix
            .client
            .conversations()
            .create_group(
                vec![bo.account_identifier.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();

        // Bo accepts the invitation
        bo.client.conversations().sync().await.unwrap();
        let bo_group = bo.client.conversation(alix_group.id()).unwrap();

        // Send a few initial messages using proper text encoding
        let text1 = TextCodec::encode("Message 1".to_string()).unwrap();
        alix_group
            .send(
                encoded_content_to_bytes(text1),
                FfiSendMessageOpts::default(),
            )
            .await
            .unwrap();

        let text2 = TextCodec::encode("Message 2".to_string()).unwrap();
        alix_group
            .send(
                encoded_content_to_bytes(text2),
                FfiSendMessageOpts::default(),
            )
            .await
            .unwrap();

        let text3 = TextCodec::encode("Message 3".to_string()).unwrap();
        bo_group
            .send(
                encoded_content_to_bytes(text3),
                FfiSendMessageOpts::default(),
            )
            .await
            .unwrap();

        // Sync both clients
        alix_group.sync().await.unwrap();
        bo_group.sync().await.unwrap();

        // Get messages to react to
        let all_messages = bo_group
            .find_enriched_messages(FfiListMessagesOptions::default())
            .unwrap();

        // Filter for just text messages to react to
        let text_messages: Vec<_> = all_messages
            .into_iter()
            .filter(|m| {
                m.kind() == FfiGroupMessageKind::Application
                    && m.content_type_id().type_id == "text"
            })
            .collect();

        assert_eq!(text_messages.len(), 3);
        let messages = text_messages;

        // Add reactions to different messages
        let reaction1 = FfiReactionPayload {
            reference: hex::encode(messages[0].id()),
            reference_inbox_id: alix.client.inbox_id(),
            action: FfiReactionAction::Added,
            content: "👍".to_string(),
            schema: FfiReactionSchema::Unicode,
        };
        bo_group
            .send(
                encode_reaction(reaction1).unwrap(),
                FfiSendMessageOpts::default(),
            )
            .await
            .unwrap();

        let reaction2 = FfiReactionPayload {
            reference: hex::encode(messages[1].id()),
            reference_inbox_id: alix.client.inbox_id(),
            action: FfiReactionAction::Added,
            content: "❤️".to_string(),
            schema: FfiReactionSchema::Unicode,
        };
        alix_group
            .send(
                encode_reaction(reaction2).unwrap(),
                FfiSendMessageOpts::default(),
            )
            .await
            .unwrap();

        // Remove a reaction
        let reaction3 = FfiReactionPayload {
            reference: hex::encode(messages[0].id()),
            reference_inbox_id: alix.client.inbox_id(),
            action: FfiReactionAction::Removed,
            content: "👍".to_string(),
            schema: FfiReactionSchema::Unicode,
        };
        bo_group
            .send(
                encode_reaction(reaction3).unwrap(),
                FfiSendMessageOpts::default(),
            )
            .await
            .unwrap();

        // Sync and verify messages with reactions
        alix_group.sync().await.unwrap();
        bo_group.sync().await.unwrap();

        // Test find_enriched_messages returns all messages including reactions
        let all_messages = alix_group
            .find_enriched_messages(FfiListMessagesOptions::default())
            .unwrap();

        // Should have 1 membership change + 3 text messages
        assert_eq!(all_messages.len(), 4);

        let message_0 = all_messages
            .iter()
            .find(|m| m.id() == messages[0].id())
            .unwrap();

        // Verify reaction content
        for reaction in message_0.reactions() {
            if let FfiDecodedMessageContent::Reaction(reaction) = reaction.content() {
                assert!(reaction.content == "👍" || reaction.content == "❤️");
            } else {
                panic!("Expected reaction content type");
            }
        }

        assert_eq!(message_0.reactions().len(), 2);
        assert_eq!(message_0.reaction_count(), 2);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_find_enriched_messages_with_replies() {
        let alix = Tester::new().await;
        let bo = Tester::new().await;

        // Create a DM conversation
        let alix_dm = alix
            .client
            .conversations()
            .find_or_create_dm(bo.account_identifier.clone(), FfiCreateDMOptions::default())
            .await
            .unwrap();

        // Bo finds the DM
        bo.client.conversations().sync().await.unwrap();
        let bo_dm = bo.client.dm_conversation(alix.client.inbox_id()).unwrap();

        // Send initial messages using proper text encoding
        let text1 = TextCodec::encode("Hello!".to_string()).unwrap();
        let msg1_id = alix_dm
            .send(
                encoded_content_to_bytes(text1),
                FfiSendMessageOpts::default(),
            )
            .await
            .unwrap();

        let text2 = TextCodec::encode("Hi there!".to_string()).unwrap();
        let msg2_id = bo_dm
            .send(
                encoded_content_to_bytes(text2),
                FfiSendMessageOpts::default(),
            )
            .await
            .unwrap();

        let text3 = TextCodec::encode("How are you?".to_string()).unwrap();
        alix_dm
            .send(
                encoded_content_to_bytes(text3),
                FfiSendMessageOpts::default(),
            )
            .await
            .unwrap();

        // Sync both clients
        alix_dm.sync().await.unwrap();
        bo_dm.sync().await.unwrap();

        // Get messages to reply to
        let messages = alix_dm
            .find_enriched_messages(FfiListMessagesOptions::default())
            .unwrap();
        // 3 messages sent + group membership change
        assert_eq!(messages.len(), 4);

        // Create replies to different messages
        let reply1 = FfiReply {
            reference: hex::encode(msg1_id),
            reference_inbox_id: Some(alix.client.inbox_id()),
            content: TextCodec::encode("Replying to Hello".to_string())
                .unwrap()
                .into(),
        };
        bo_dm
            .send(encode_reply(reply1).unwrap(), FfiSendMessageOpts::default())
            .await
            .unwrap();

        let reply2 = FfiReply {
            reference: hex::encode(msg2_id),
            reference_inbox_id: Some(bo.client.inbox_id()),
            content: TextCodec::encode("Replying to Hi there".to_string())
                .unwrap()
                .into(),
        };
        alix_dm
            .send(encode_reply(reply2).unwrap(), FfiSendMessageOpts::default())
            .await
            .unwrap();

        // Add a reaction to a reply
        alix_dm.sync().await.unwrap();
        let updated_messages = alix_dm
            .find_enriched_messages(FfiListMessagesOptions::default())
            .unwrap();

        // Find the first reply message
        updated_messages
            .iter()
            .find(|m| {
                if let FfiDecodedMessageContent::Reply(reply) = m.content() {
                    // Check if the content matches
                    if let Some(FfiDecodedMessageBody::Text(text)) = &reply.content {
                        text.content == "Replying to Hello"
                    } else {
                        false
                    }
                } else {
                    false
                }
            })
            .unwrap();
    }

    #[tokio::test]
    async fn test_read_receipt_roundtrip() {
        let original = FfiReadReceipt {};

        let encoded = encode_read_receipt(original.clone()).unwrap();
        decode_read_receipt(encoded).unwrap();
    }

    #[tokio::test]
    async fn test_remote_attachment_roundtrip() {
        let original = FfiRemoteAttachment {
            filename: Some("remote_file.txt".to_string()),
            content_length: 2048,
            url: "https://example.com/file.txt".to_string(),
            content_digest: "sha256:abc123def456".to_string(),
            scheme: "https".to_string(),
            secret: vec![1, 2, 3, 4, 5],
            nonce: vec![6, 7, 8, 9, 10],
            salt: vec![11, 12, 13, 14, 15],
        };

        let encoded = encode_remote_attachment(original.clone()).unwrap();
        let decoded = decode_remote_attachment(encoded).unwrap();

        assert_eq!(original.filename, decoded.filename);
        assert_eq!(original.content_length, decoded.content_length);
        assert_eq!(original.url, decoded.url);
        assert_eq!(original.content_digest, decoded.content_digest);
        assert_eq!(original.secret, decoded.secret);
        assert_eq!(original.nonce, decoded.nonce);
        assert_eq!(original.salt, decoded.salt);
    }

    #[tokio::test]
    async fn test_can_successfully_thread_dms() {
        // Create two test users
        let wallet_bo = generate_local_wallet();
        let wallet_alix = generate_local_wallet();

        let client_bo = new_test_client_with_wallet(wallet_bo).await;
        let client_alix = new_test_client_with_wallet(wallet_alix).await;

        let bo_provider = client_bo.inner_client.context.mls_provider();
        let bo_conn = bo_provider.key_store().db();
        let alix_provider = client_alix.inner_client.context.mls_provider();
        let alix_conn = alix_provider.key_store().db();

        // Find or create DM conversations
        let convo_bo = client_bo
            .conversations()
            .find_or_create_dm_by_inbox_id(client_alix.inbox_id(), FfiCreateDMOptions::default())
            .await
            .unwrap();
        let convo_alix = client_alix
            .conversations()
            .find_or_create_dm_by_inbox_id(client_bo.inbox_id(), FfiCreateDMOptions::default())
            .await
            .unwrap();

        // Send messages
        convo_bo.send_text("Bo hey").await.unwrap();
        tokio::time::sleep(Duration::from_millis(500)).await;
        convo_alix.send_text("Alix hey").await.unwrap();

        let group_bo = bo_conn.find_group(&convo_bo.id()).unwrap().unwrap();
        let group_alix = alix_conn.find_group(&convo_alix.id()).unwrap().unwrap();
        assert!(group_bo.last_message_ns.unwrap() < group_alix.last_message_ns.unwrap());
        assert_eq!(group_bo.id, convo_bo.id());

        // Check messages
        let bo_messages = convo_bo
            .find_messages(FfiListMessagesOptions::default())
            .await
            .unwrap();
        let alix_messages = convo_alix
            .find_messages(FfiListMessagesOptions::default())
            .await
            .unwrap();

        assert_eq!(bo_messages.len(), 2, "Bo should see 2 messages");
        assert_eq!(alix_messages.len(), 2, "Alix should see 2 messages");

        // Sync conversations
        client_bo
            .conversations()
            .sync_all_conversations(None)
            .await
            .unwrap();
        client_alix
            .conversations()
            .sync_all_conversations(None)
            .await
            .unwrap();

        let bo_messages = convo_bo
            .find_messages(FfiListMessagesOptions::default())
            .await
            .unwrap();
        let alix_messages = convo_alix
            .find_messages(FfiListMessagesOptions::default())
            .await
            .unwrap();
        assert_eq!(bo_messages.len(), 3, "Bo should see 3 messages after sync");
        assert_eq!(
            alix_messages.len(),
            3,
            "Alix should see 3 messages after sync"
        );

        // Ensure conversations remain the same
        let convo_alix_2 = client_alix
            .conversations()
            .find_or_create_dm_by_inbox_id(client_bo.inbox_id(), FfiCreateDMOptions::default())
            .await
            .unwrap();
        let convo_bo_2 = client_bo
            .conversations()
            .find_or_create_dm_by_inbox_id(client_alix.inbox_id(), FfiCreateDMOptions::default())
            .await
            .unwrap();

        let topic_bo_same = client_bo.conversation(convo_bo.id()).unwrap();
        let topic_alix_same = client_alix.conversation(convo_alix.id()).unwrap();

        assert_eq!(
            convo_alix_2.id(),
            convo_bo_2.id(),
            "Conversations should match"
        );
        assert_eq!(
            convo_alix.id(),
            convo_bo_2.id(),
            "Conversations should match"
        );
        assert_ne!(
            convo_alix.id(),
            convo_bo.id(),
            "Conversations id should not match dms should be matched on peerInboxId"
        );
        assert_eq!(convo_alix.id(), topic_bo_same.id(), "Topics should match");
        assert_eq!(convo_alix.id(), topic_alix_same.id(), "Topics should match");
        let alix_dms = client_alix
            .conversations()
            .list_dms(FfiListConversationsOptions::default())
            .unwrap();
        let bo_dms = client_bo
            .conversations()
            .list_dms(FfiListConversationsOptions::default())
            .unwrap();
        assert_eq!(
            convo_alix.id(),
            bo_dms[0].conversation.id(),
            "Dms should match"
        );
        assert_eq!(
            convo_alix.id(),
            alix_dms[0].conversation.id(),
            "Dms should match"
        );

        // Send additional messages
        let text_message_bo2 = TextCodec::encode("Bo hey2".to_string()).unwrap();
        convo_alix_2
            .send(
                encoded_content_to_bytes(text_message_bo2),
                FfiSendMessageOpts::default(),
            )
            .await
            .unwrap();
        let text_message_alix2 = TextCodec::encode("Alix hey2".to_string()).unwrap();
        convo_bo_2
            .send(
                encoded_content_to_bytes(text_message_alix2),
                FfiSendMessageOpts::default(),
            )
            .await
            .unwrap();
        convo_bo_2.sync().await.unwrap();
        convo_alix_2.sync().await.unwrap();

        // Validate final message count
        let final_bo_messages = convo_alix_2
            .find_messages(FfiListMessagesOptions::default())
            .await
            .unwrap();
        let final_alix_messages = convo_bo_2
            .find_messages(FfiListMessagesOptions::default())
            .await
            .unwrap();

        assert_eq!(final_bo_messages.len(), 5, "Bo should see 5 messages");
        assert_eq!(final_alix_messages.len(), 5, "Alix should see 5 messages");
    }

    #[tokio::test]
    async fn test_can_successfully_thread_dms_with_no_messages() {
        // Create two test users
        let wallet_bo = generate_local_wallet();
        let wallet_alix = generate_local_wallet();

        let client_bo = new_test_client_with_wallet(wallet_bo).await;
        let client_alix = new_test_client_with_wallet(wallet_alix).await;

        // Find or create DM conversations
        let convo_bo = client_bo
            .conversations()
            .find_or_create_dm_by_inbox_id(client_alix.inbox_id(), FfiCreateDMOptions::default())
            .await
            .unwrap();
        let convo_alix = client_alix
            .conversations()
            .find_or_create_dm_by_inbox_id(client_bo.inbox_id(), FfiCreateDMOptions::default())
            .await
            .unwrap();

        client_bo
            .conversations()
            .sync_all_conversations(None)
            .await
            .unwrap();
        client_alix
            .conversations()
            .sync_all_conversations(None)
            .await
            .unwrap();

        let group_bo = client_bo.conversation(convo_bo.id()).unwrap();
        let group_alix = client_alix.conversation(convo_alix.id()).unwrap();
        assert_eq!(group_bo.id(), group_alix.id(), "Conversations should match");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_key_package_validation() {
        // Create a test client
        let client = Tester::new().await;

        // Get the client's inbox state to retrieve installation IDs
        let inbox_state = client.inbox_state(true).await.unwrap();
        // let inbox_state = client.get_latest_inbox_state("f87420435131ea1b911ad66fbe4b626b107f81955da023d049f8aef6636b8e1b".to_string()).await.unwrap();
        // let inbox_state = client.get_latest_inbox_state("bd03ba1d688c7ababe4e39eb0012a3cff7003e0faef2e164ff95e1ce4db30141".to_string()).await.unwrap();

        // Extract installation IDs from the inbox state
        let installation_ids: Vec<Vec<u8>> = inbox_state
            .installations
            .iter()
            .map(|installation| installation.id.clone())
            .collect();

        assert!(
            !installation_ids.is_empty(),
            "Client should have at least one installation ID"
        );

        // Get key packages for the installation IDs
        let key_package_statuses = client
            .get_key_package_statuses_for_installation_ids(installation_ids.clone())
            .await
            .unwrap();

        // Verify we got results for all installation IDs
        assert_eq!(
            key_package_statuses.len(),
            installation_ids.len(),
            "Should get key package status for each installation ID"
        );

        // Check each key package status
        for (installation_id, key_package_status) in key_package_statuses {
            println!("Installation ID: {:?}", hex::encode(&installation_id));

            if let Some(error) = &key_package_status.validation_error {
                println!("Key package validation error: {}", error);
            } else if let Some(lifetime) = &key_package_status.lifetime {
                let not_before_date =
                    chrono::DateTime::<chrono::Utc>::from_timestamp(lifetime.not_before as i64, 0)
                        .map(|dt| dt.to_rfc3339())
                        .unwrap_or_else(|| lifetime.not_before.to_string());
                let not_after_date =
                    chrono::DateTime::<chrono::Utc>::from_timestamp(lifetime.not_after as i64, 0)
                        .map(|dt| dt.to_rfc3339())
                        .unwrap_or_else(|| lifetime.not_after.to_string());

                println!(
                    "Key package valid: not_before={} ({}), not_after={} ({})",
                    lifetime.not_before, not_before_date, lifetime.not_after, not_after_date
                );
                println!();

                // Verify the lifetime is valid (not expired)
                let current_time = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();

                assert!(
                    lifetime.not_before <= current_time,
                    "Key package should be valid now"
                );
                assert!(
                    lifetime.not_after > current_time,
                    "Key package should not be expired"
                );
            } else {
                println!("No lifetime for this key package")
            }
        }
    }

    #[tokio::test]
    async fn test_sync_consent() {
        // Create two test users
        let alix = Tester::builder().sync_server().sync_worker().build().await;
        let bo = Tester::new().await;

        // Create a group conversation
        let alix_group = alix
            .conversations()
            .create_group_with_inbox_ids(vec![bo.inbox_id()], FfiCreateGroupOptions::default())
            .await
            .unwrap();
        let initial_consent = alix_group.consent_state().unwrap();
        assert_eq!(initial_consent, FfiConsentState::Allowed);

        let alix2 = alix.builder.build().await;
        let state = alix2.inbox_state(true).await.unwrap();
        assert_eq!(state.installations.len(), 2);

        alix.conversations()
            .sync_all_conversations(None)
            .await
            .unwrap();
        alix2
            .conversations()
            .sync_all_conversations(None)
            .await
            .unwrap();

        let sg1 = alix
            .inner_client
            .device_sync_client()
            .get_sync_group()
            .await
            .unwrap();
        let sg2 = alix2
            .inner_client
            .device_sync_client()
            .get_sync_group()
            .await
            .unwrap();

        assert_eq!(sg1.group_id, sg2.group_id);

        alix.conversations()
            .sync_all_conversations(None)
            .await
            .unwrap();
        alix2
            .conversations()
            .sync_all_conversations(None)
            .await
            .unwrap();

        alix2.inner_client.sync_welcomes().await.unwrap();

        // Update consent state
        alix_group
            .update_consent_state(FfiConsentState::Denied)
            .unwrap();
        alix.worker()
            .register_interest(SyncMetric::ConsentSent, 3)
            .wait()
            .await
            .unwrap();

        sg2.sync().await.unwrap();

        alix2
            .worker()
            .register_interest(SyncMetric::ConsentReceived, 1)
            .wait()
            .await
            .unwrap();

        let alix_group2 = alix2.conversation(alix_group.id()).unwrap();
        assert_eq!(
            alix_group2.consent_state().unwrap(),
            FfiConsentState::Denied
        );
    }

    #[tokio::test]
    async fn test_can_find_duplicate_dms_for_group() {
        let wallet_a = generate_local_wallet();
        let wallet_b = generate_local_wallet();

        let client_a = new_test_client_with_wallet(wallet_a).await;
        let client_b = new_test_client_with_wallet(wallet_b).await;

        // Create two DMs (same logical participants, will generate duplicate dm_id)
        let dm1 = client_a
            .conversations()
            .find_or_create_dm_by_inbox_id(client_b.inbox_id(), FfiCreateDMOptions::default())
            .await
            .unwrap();

        let _dm2 = client_b
            .conversations()
            .find_or_create_dm_by_inbox_id(client_a.inbox_id(), FfiCreateDMOptions::default())
            .await
            .unwrap();

        client_a
            .conversations()
            .sync_all_conversations(None)
            .await
            .unwrap();
        client_b
            .conversations()
            .sync_all_conversations(None)
            .await
            .unwrap();

        let group_a = client_a.conversation(dm1.id()).unwrap();
        let duplicates = group_a.find_duplicate_dms().await.unwrap();

        assert_eq!(duplicates.len(), 1);
    }

    #[tokio::test]
    async fn test_can_quickly_fetch_dm_peer_inbox_id() {
        let wallet_a = generate_local_wallet();
        let wallet_b = generate_local_wallet();

        let client_a = new_test_client_with_wallet(wallet_a).await;
        let client_b = new_test_client_with_wallet(wallet_b).await;

        // Initialize streaming at the beginning, before creating the DM
        let stream_callback = Arc::new(RustStreamCallback::default());
        let stream = client_a
            .conversations()
            .stream(stream_callback.clone())
            .await;

        // Wait for the streaming to initialize
        stream.wait_for_ready().await;

        // Test find_or_create_dm returns correct dm_peer_inbox_id
        let dm = client_a
            .conversations()
            .find_or_create_dm_by_inbox_id(client_b.inbox_id(), FfiCreateDMOptions::default())
            .await
            .unwrap();

        assert_eq!(dm.dm_peer_inbox_id().unwrap(), client_b.inbox_id());

        // Test conversations.list returns correct dm_peer_inbox_id
        let client_a_conversation_list = client_a
            .conversations()
            .list(FfiListConversationsOptions::default())
            .unwrap();
        assert_eq!(client_a_conversation_list.len(), 1);
        assert_eq!(
            client_a_conversation_list[0]
                .conversation()
                .dm_peer_inbox_id()
                .unwrap(),
            client_b.inbox_id()
        );

        // Wait for streaming to receive the conversation
        // This is similar to how test_conversation_streaming and other streaming tests work
        for _ in 0..10 {
            let conversation_count = stream_callback.conversations.lock().len();

            if conversation_count > 0 {
                break;
            }

            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        // Get the streamed conversations
        let streamed_conversations = stream_callback.conversations.lock().clone();

        // Verify we received the conversation from the stream
        assert!(
            !streamed_conversations.is_empty(),
            "Should have received the conversation from the stream"
        );

        // Verify the streamed conversation has the correct dm_peer_inbox_id
        let found_matching_peer = streamed_conversations.iter().any(|conversation| {
            if let Some(dm_peer_id) = conversation.dm_peer_inbox_id() {
                dm_peer_id == client_b.inbox_id()
            } else {
                false
            }
        });

        assert!(
            found_matching_peer,
            "Should have received conversation with matching peer inbox ID"
        );

        // Clean up the stream
        stream.end_and_wait().await.unwrap();
    }

    #[tokio::test]
    async fn test_new_installation_group_message_visibility() {
        let alix = Tester::builder().sync_worker().build().await;
        let bo = Tester::new().await;

        let group = alix
            .conversations()
            .create_group_with_inbox_ids(vec![bo.inbox_id()], Default::default())
            .await
            .unwrap();

        let text_message_alix = TextCodec::encode("hello from alix".to_string()).unwrap();
        group
            .send(
                encoded_content_to_bytes(text_message_alix.clone()),
                FfiSendMessageOpts::default(),
            )
            .await
            .unwrap();

        let alix2 = alix.builder.build().await;

        bo.conversations().sync().await.unwrap();
        let bo_group = bo.conversation(group.id()).unwrap();
        let text_message_bo = TextCodec::encode("hello from bo".to_string()).unwrap();
        bo_group
            .send(
                encoded_content_to_bytes(text_message_bo.clone()),
                FfiSendMessageOpts::default(),
            )
            .await
            .unwrap();
        alix.conversations()
            .sync_all_conversations(None)
            .await
            .unwrap();
        alix2
            .conversations()
            .sync_all_conversations(None)
            .await
            .unwrap();

        alix.inner_client
            .test_has_same_sync_group_as(&alix2.inner_client)
            .await
            .unwrap();

        let group2 = alix2.conversation(group.id()).unwrap();
        let messages = group2
            .find_messages(FfiListMessagesOptions::default())
            .await
            .unwrap();

        assert_eq!(
            messages.len(),
            2,
            "Expected two message to be visible to new installation"
        );

        let text_message_alix2 = TextCodec::encode("hi from alix2".to_string()).unwrap();
        let msg_from_alix2 = group2
            .send(
                encoded_content_to_bytes(text_message_alix2.clone()),
                FfiSendMessageOpts::default(),
            )
            .await
            .unwrap();

        bo.conversations()
            .sync_all_conversations(None)
            .await
            .unwrap();
        let bob_group = bo.conversation(group.id()).unwrap();
        let bob_msgs = bob_group
            .find_messages(FfiListMessagesOptions::default())
            .await
            .unwrap();

        assert!(
            bob_msgs.iter().any(|m| m.id == msg_from_alix2),
            "Bob should see the message sent by alix2"
        );

        alix.conversations()
            .sync_all_conversations(None)
            .await
            .unwrap();
        let alice_msgs = group
            .find_messages(FfiListMessagesOptions::default())
            .await
            .unwrap();

        assert!(
            alice_msgs.iter().any(|m| m.id == msg_from_alix2),
            "Original Alix should see the message from alix2"
        );
    }

    #[tokio::test]
    async fn test_cannot_create_more_than_max_installations() {
        // Create a base tester
        let alix_wallet = PrivateKeySigner::random();
        let bo = Tester::new().await;
        let alix = new_test_client_no_panic(alix_wallet.clone(), None)
            .await
            .unwrap();

        // Create (MAX_INSTALLATIONS_PER_INBOX - 1) additional installations (total MAX_INSTALLATIONS_PER_INBOX)
        let mut installations = vec![];
        for _ in 0..(MAX_INSTALLATIONS_PER_INBOX - 1) {
            let new_client_installation = new_test_client_no_panic(alix_wallet.clone(), None)
                .await
                .unwrap();
            installations.push(new_client_installation);
        }

        // Verify we have MAX_INSTALLATIONS_PER_INBOX installations
        let state = alix.inbox_state(true).await.unwrap();
        assert_eq!(state.installations.len(), MAX_INSTALLATIONS_PER_INBOX);

        // Attempt to create an additional installation, expect failure
        let alix_max_plus_one_result = new_test_client_no_panic(alix_wallet.clone(), None).await;
        assert!(
            alix_max_plus_one_result.is_err(),
            "Expected failure when creating MAX_INSTALLATIONS_PER_INBOX + 1 installation, but got Ok"
        );

        // Create a group with one of the valid installations
        let bo_group = bo
            .conversations()
            .create_group_with_inbox_ids(
                vec![installations[2].inbox_id()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();

        // Confirm group members list Alix's inbox with exactly 5 installations
        let members = bo_group.list_members().await.unwrap();
        let alix_member = members
            .iter()
            .find(|m| m.inbox_id == alix.inbox_id())
            .expect("Alix should be a group member");
        assert_eq!(
            alix_member.installation_ids.len(),
            MAX_INSTALLATIONS_PER_INBOX
        );

        // Revoke one of Alix's installations (e.g. installations[4])
        let signature_request = alix
            .revoke_installations(vec![installations[4].installation_id()])
            .await
            .unwrap();

        signature_request.add_wallet_signature(&alix_wallet).await;
        alix.apply_signature_request(signature_request)
            .await
            .unwrap();

        let state_after_revoke = alix.inbox_state(true).await.unwrap();
        assert_eq!(
            state_after_revoke.installations.len(),
            MAX_INSTALLATIONS_PER_INBOX - 1
        );

        // Now try building alix6 again – should succeed
        let _new_client_installation = new_test_client_no_panic(alix_wallet.clone(), None).await;
        let updated_state = alix.inbox_state(true).await.unwrap();
        assert_eq!(
            updated_state.installations.len(),
            MAX_INSTALLATIONS_PER_INBOX
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_static_revoke_installations() {
        let wallet = PrivateKeySigner::random();

        let ident = wallet.identifier();
        let ffi_ident: FfiIdentifier = ident.clone().into();
        let api_backend = connect_to_backend_test().await;

        let client_1 = new_test_client_with_wallet(wallet.clone()).await;
        let client_2 = new_test_client_with_wallet(wallet.clone()).await;
        let _client_3 = new_test_client_with_wallet(wallet.clone()).await;
        let _client_4 = new_test_client_with_wallet(wallet.clone()).await;
        let _client_5 = new_test_client_with_wallet(wallet.clone()).await;

        let inbox_id = client_1.inbox_id();

        let client_1_state = client_1.inbox_state(true).await.unwrap();
        let client_2_state = client_2.inbox_state(true).await.unwrap();
        assert_eq!(client_1_state.installations.len(), 5);
        assert_eq!(client_2_state.installations.len(), 5);

        let revoke_request = revoke_installations(
            api_backend.clone(),
            ffi_ident,
            &inbox_id,
            vec![client_2.installation_id()],
        )
        .unwrap();

        revoke_request.add_wallet_signature(&wallet).await;
        apply_signature_request(api_backend.clone(), revoke_request)
            .await
            .unwrap();

        let client_1_state_after = client_1.inbox_state(true).await.unwrap();
        let client_2_state_after = client_2.inbox_state(true).await.unwrap();

        assert_eq!(client_1_state_after.installations.len(), 4);
        assert_eq!(client_2_state_after.installations.len(), 4);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_static_revoke_fails_with_non_recovery_identity() {
        let wallet_a = PrivateKeySigner::random();
        let wallet_b = PrivateKeySigner::random();

        let client_a = new_test_client_with_wallet(wallet_a.clone()).await;
        let client_a2 = new_test_client_with_wallet(wallet_a.clone()).await;
        let inbox_id = client_a.inbox_id();

        let add_identity_request = client_a
            .add_identity(wallet_b.identifier().into())
            .await
            .unwrap();
        add_identity_request.add_wallet_signature(&wallet_b).await;
        client_a
            .apply_signature_request(add_identity_request)
            .await
            .unwrap();

        let client_a_state = client_a.inbox_state(true).await.unwrap();
        assert_eq!(client_a_state.installations.len(), 2);

        let ffi_ident: FfiIdentifier = wallet_b.identifier().into();
        let api_backend = connect_to_backend_test().await;

        let revoke_request = revoke_installations(
            api_backend.clone(),
            ffi_ident,
            &inbox_id,
            vec![client_a2.installation_id()],
        )
        .unwrap();

        revoke_request.add_wallet_signature(&wallet_b).await;
        let revoke_result = apply_signature_request(api_backend.clone(), revoke_request).await;

        assert!(
            revoke_result.is_err(),
            "Revocation should fail when using a non-recovery identity"
        );

        let client_a_state_after = client_a.inbox_state(true).await.unwrap();
        assert_eq!(client_a_state_after.installations.len(), 2);
    }

    #[tokio::test]
    async fn test_can_get_inbox_state_statically() {
        let alix_wallet = PrivateKeySigner::random();
        let alix = new_test_client_no_panic(alix_wallet.clone(), None)
            .await
            .unwrap();
        let _alix2 = new_test_client_no_panic(alix_wallet.clone(), None)
            .await
            .unwrap();
        let _alix3 = new_test_client_no_panic(alix_wallet.clone(), None)
            .await
            .unwrap();

        let api_backend = connect_to_backend_test().await;

        let state = inbox_state_from_inbox_ids(api_backend, vec![alix.inbox_id()])
            .await
            .unwrap();
        assert_eq!(state[0].installations.len(), 3);
        assert_eq!(
            state[0].creation_signature_kind.clone().unwrap(),
            FfiSignatureKind::Erc191
        )
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_sorts_members_by_created_at_using_ffi_identifiers() {
        let ffi_inbox_owner = FfiWalletInboxOwner::new();
        let ident = ffi_inbox_owner.identifier();
        let nonce = 1;
        let inbox_id = ident.inbox_id(nonce).unwrap();

        let path = tmp_path();
        let key = static_enc_key().to_vec();
        let client = create_client(
            connect_to_backend_test().await,
            connect_to_backend_test().await,
            Some(path.clone()),
            Some(key),
            &inbox_id,
            ffi_inbox_owner.identifier(),
            nonce,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

        let signature_request = client.signature_request().unwrap().clone();
        register_client_with_wallet(&ffi_inbox_owner, &client).await;

        signature_request
            .add_wallet_signature(&ffi_inbox_owner.wallet)
            .await;

        let initial_state = client
            .get_latest_inbox_state(inbox_id.clone())
            .await
            .expect("Failed to fetch inbox state");

        assert_eq!(
            initial_state.account_identities.len(),
            1,
            "Should have 1 identity initially"
        );

        for _i in 0..5 {
            let wallet_to_add = generate_local_wallet();
            let new_account_address = wallet_to_add.identifier();

            let signature_request = client
                .add_identity(new_account_address.into())
                .await
                .expect("could not add wallet");

            signature_request.add_wallet_signature(&wallet_to_add).await;

            client
                .apply_signature_request(signature_request)
                .await
                .unwrap();

            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        let updated_ffi_state = client
            .get_latest_inbox_state(inbox_id.clone())
            .await
            .expect("Failed to fetch updated inbox state");

        assert_eq!(
            updated_ffi_state.account_identities.len(),
            1 + 5,
            "Expected 1 initial identity + 5 added"
        );

        let association_state = client
            .inner_client
            .identity_updates()
            .get_latest_association_state(&client.inner_client.context.store().db(), &inbox_id)
            .await
            .expect("Failed to fetch association state");

        let expected_order: Vec<_> = association_state
            .members()
            .iter()
            .filter_map(|m| match &m.identifier {
                MemberIdentifier::Ethereum(addr) => Some(addr.to_string()),
                _ => None,
            })
            .collect();

        let ffi_identities: Vec<_> = updated_ffi_state
            .account_identities
            .iter()
            .map(|id| id.identifier.clone())
            .collect();

        assert_eq!(
            ffi_identities, expected_order,
            "FFI identifiers are not ordered by creation timestamp"
        );
    }

    #[tokio::test]
    async fn test_is_connected_after_connect() {
        let api_backend = connect_to_backend_test().await;

        let connected = is_connected(api_backend).await;

        assert!(connected, "Expected API client to report as connected");

        let api = connect_to_backend("http://127.0.0.1:59999".to_string(), None, false, None)
            .await
            .unwrap();
        let api = ApiClientWrapper::new(api.0.clone(), Default::default());
        let result = api
            .query_group_messages(xmtp_common::rand_vec::<16>().into())
            .await;
        assert!(result.is_err(), "Expected connection to fail");
    }

    #[tokio::test]
    async fn test_get_last_read_times() {
        let alix_wallet = generate_local_wallet();
        let bo_wallet = generate_local_wallet();

        let alix_client = new_test_client_with_wallet(alix_wallet).await;
        let bo_client = new_test_client_with_wallet(bo_wallet).await;

        // Create a DM between Alix and Bo
        let alix_dm = alix_client
            .conversations()
            .find_or_create_dm(
                bo_client.account_identifier.clone(),
                FfiCreateDMOptions {
                    message_disappearing_settings: None,
                },
            )
            .await
            .unwrap();

        let bo_dm = bo_client
            .conversations()
            .find_or_create_dm(
                alix_client.account_identifier.clone(),
                FfiCreateDMOptions {
                    message_disappearing_settings: None,
                },
            )
            .await
            .unwrap();

        // Bo sends a read receipt
        let read_receipt = FfiReadReceipt {};
        let read_receipt_encoded = encode_read_receipt(read_receipt).unwrap();
        bo_dm
            .send(read_receipt_encoded, FfiSendMessageOpts::default())
            .await
            .unwrap();

        alix_client
            .conversations()
            .sync_all_conversations(None)
            .await
            .unwrap();
        bo_client
            .conversations()
            .sync_all_conversations(None)
            .await
            .unwrap();

        // Test get_last_read_times - should return Bo's read receipt timestamp
        let alix_last_read_times = alix_dm.get_last_read_times().unwrap();
        let bo_last_read_times = bo_dm.get_last_read_times().unwrap();

        // Should have one entry for Bo's inbox ID
        assert_eq!(alix_last_read_times.len(), 1);
        assert_eq!(bo_last_read_times.len(), 1);
        assert_eq!(alix_last_read_times, bo_last_read_times);

        // Get Bo's inbox ID
        let bo_inbox_id = bo_client.inbox_id();

        // Verify that Bo's read receipt timestamp is recorded
        assert!(alix_last_read_times.contains_key(&bo_inbox_id));
        let bo_read_time = alix_last_read_times.get(&bo_inbox_id).unwrap();
        assert!(
            *bo_read_time > 0,
            "Read receipt timestamp should be positive"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_pagination_of_conversations_list() {
        let bo_client = new_test_client().await;
        let caro_client = new_test_client().await;

        // Create 15 groups
        let mut groups = Vec::new();
        for i in 0..15 {
            let group = bo_client
                .conversations()
                .create_group(
                    vec![caro_client.account_identifier.clone()],
                    FfiCreateGroupOptions {
                        group_name: Some(format!("Test Group {}", i)),
                        ..Default::default()
                    },
                )
                .await
                .unwrap();
            groups.push(group);
        }

        // Send a message to every 7th group to ensure they're ordered by last message
        // and not by created_at
        for (index, group) in groups.iter().enumerate() {
            if index % 2 == 0 {
                group.send_text("Jumbling the sort").await.unwrap();
            }
        }

        // Track all conversations retrieved through pagination
        let mut all_conversations = std::collections::HashSet::new();
        let mut page_count = 0;

        // Get the first page
        let mut page = bo_client
            .conversations()
            .list_groups(FfiListConversationsOptions {
                limit: Some(5),
                order_by: Some(FfiGroupQueryOrderBy::LastActivity),
                ..Default::default()
            })
            .unwrap();

        while !page.is_empty() {
            page_count += 1;

            // Add new conversation IDs to our set
            for conversation in &page {
                let conversation_arc = conversation.conversation();
                assert!(!all_conversations.contains(&conversation_arc.id()));
                all_conversations.insert(conversation_arc.id());
            }

            // If we got fewer than the limit, we've reached the end
            if page.len() < 5 {
                break;
            }

            // Get the oldest (last) conversation's timestamp for the next page
            let last_conversation = page.last().unwrap().conversation();

            let before = if let Some(last_message) = page.last().unwrap().last_message() {
                last_message.sent_at_ns
            } else {
                last_conversation.created_at_ns()
            };

            // Get the next page
            page = bo_client
                .conversations()
                .list_groups(FfiListConversationsOptions {
                    last_activity_before_ns: Some(before),
                    order_by: Some(FfiGroupQueryOrderBy::LastActivity),
                    limit: Some(5),
                    ..Default::default()
                })
                .unwrap();

            // Safety check to prevent infinite loop
            if page_count > 10 {
                panic!("Too many pages, possible infinite loop");
            }
        }

        // Validate results
        assert_eq!(
            all_conversations.len(),
            15,
            "Should have retrieved all 15 groups"
        );

        // Verify all created groups are in the results
        for group in &groups {
            assert!(
                all_conversations.contains(&group.id()),
                "Group {} should be in paginated results",
                hex::encode(group.id())
            );
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_overlapping_streams() {
        let alix = Tester::new().await;
        let bo = Tester::new().await;

        let message_callbacks = Arc::new(RustStreamCallback::default());
        let conversation_callbacks = Arc::new(RustStreamCallback::default());
        // Stream all group messages
        let stream_messages = bo.conversations().stream(message_callbacks.clone()).await;
        // Stream all groups
        let stream_conversations = bo
            .conversations()
            .stream(conversation_callbacks.clone())
            .await;
        stream_messages.wait_for_ready().await;
        stream_conversations.wait_for_ready().await;

        // Create group and send first message
        let alix_group = alix
            .conversations()
            .create_group(
                vec![bo.account_identifier.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();

        alix_group
            .send("hi".into(), FfiSendMessageOpts::default())
            .await
            .unwrap();

        // The group should be received in both streams without erroring
        message_callbacks.wait_for_delivery(None).await.unwrap();
        conversation_callbacks
            .wait_for_delivery(None)
            .await
            .unwrap();
    }
}
