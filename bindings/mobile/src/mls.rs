use crate::fork_recovery::FfiForkRecoveryOpts;
use crate::identity::{FfiCollectionExt, FfiCollectionTryExt, FfiIdentifier};
pub use crate::inbox_owner::SigningError;
use crate::logger::init_logger;
use crate::message::{
    FfiActions, FfiDecodedMessage, FfiDeliveryStatus, FfiIntent, FfiReactionPayload,
};
use crate::worker::FfiSyncWorker;
use crate::worker::FfiSyncWorkerMode;
use crate::{FfiGroupUpdated, FfiReply, FfiSubscribeError, FfiWalletSendCalls, GenericError};
use futures::future::try_join_all;
use prost::Message;
use std::{collections::HashMap, convert::TryInto, sync::Arc};
use tokio::sync::Mutex;
use xmtp_api::{ApiClientWrapper, strategies};
use xmtp_api_d14n::{ClientBundle, MessageBackendBuilder};
use xmtp_common::time::now_ns;
use xmtp_common::{AbortHandle, GenericStreamHandle, StreamHandle};
use xmtp_content_types::actions::{Actions, ActionsCodec};
use xmtp_content_types::attachment::Attachment;
use xmtp_content_types::attachment::AttachmentCodec;
use xmtp_content_types::group_updated::GroupUpdatedCodec;
use xmtp_content_types::intent::{Intent, IntentCodec};
use xmtp_content_types::leave_request::LeaveRequestCodec;
use xmtp_content_types::markdown::MarkdownCodec;
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
use xmtp_content_types::wallet_send_calls::WalletSendCallsCodec;
use xmtp_content_types::{ContentCodec, encoded_content_to_bytes};
use xmtp_db::NativeDb;
use xmtp_db::group::DmIdExt;
use xmtp_db::group::{ConversationType, GroupMembershipState, GroupQueryOrderBy};
use xmtp_db::group_message::{ContentType, MsgQueryArgs};
use xmtp_db::group_message::{SortBy, SortDirection, StoredGroupMessageWithReactions};
use xmtp_db::user_preferences::HmacKey;
use xmtp_db::{
    EncryptedMessageStore, EncryptionKey, StorageOption,
    consent_record::{ConsentState, ConsentType, StoredConsentRecord},
    group::GroupQueryArgs,
    group_message::{GroupMessageKind, StoredGroupMessage},
};
use xmtp_id::associations::{Identifier, ident, verify_signed_with_public_context};
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
use xmtp_mls::context::XmtpSharedContext;
use xmtp_mls::cursor_store::SqliteCursorStore;
use xmtp_mls::groups::ConversationDebugInfo;
use xmtp_mls::identity_updates::revoke_installations_with_verifier;
use xmtp_mls::identity_updates::{
    apply_signature_request_with_verifier, get_creation_signature_kind,
};
use xmtp_mls::mls_common::group::DMMetadataOptions;
use xmtp_mls::mls_common::group::GroupMetadataOptions;
use xmtp_mls::mls_common::group_metadata::GroupMetadata;
use xmtp_mls::mls_common::group_mutable_metadata::MessageDisappearingSettings;
use xmtp_mls::mls_common::group_mutable_metadata::MetadataField;
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
use xmtp_proto::api::IsConnectedCheck;
use xmtp_proto::api_client::AggregateStats;
use xmtp_proto::api_client::ApiStats;
use xmtp_proto::api_client::IdentityStats;
use xmtp_proto::types::Cursor;
use xmtp_proto::types::{ApiIdentifier, GroupMessageMetadata};
use xmtp_proto::xmtp::mls::message_contents::EncodedContent;
use xmtp_proto::xmtp::mls::message_contents::content_types::LeaveRequest;
use xmtp_proto::xmtp::mls::message_contents::content_types::{MultiRemoteAttachment, ReactionV2};

// Re-export types from message module that are used in public APIs
pub use crate::message::{
    FfiAttachment, FfiLeaveRequest, FfiMultiRemoteAttachment, FfiReadReceipt, FfiRemoteAttachment,
    FfiTransactionReference,
};

pub mod device_sync;
pub mod gateway_auth;
#[cfg(any(test, feature = "bench"))]
pub mod inbox_owner;
#[cfg(any(test, feature = "bench"))]
pub mod test_utils;

pub type RustXmtpClient = MlsClient<xmtp_mls::MlsContext>;
pub type RustMlsGroup = MlsGroup<xmtp_mls::MlsContext>;

/// the opaque Xmtp Api Client for iOS/Android bindings
#[derive(uniffi::Object, Clone)]
pub struct XmtpApiClient(xmtp_mls::XmtpClientBundle);

/// connect to the XMTP backend
/// specifying `gateway_host` enables the D14n backend
/// and assumes `host` is set to the correct
/// d14n backend url.
#[uniffi::export(async_runtime = "tokio")]
pub async fn connect_to_backend(
    v3_host: String,
    gateway_host: Option<String>,
    is_secure: bool,
    client_mode: Option<FfiClientMode>,
    app_version: Option<String>,
    auth_callback: Option<Arc<dyn gateway_auth::FfiAuthCallback>>,
    auth_handle: Option<Arc<gateway_auth::FfiAuthHandle>>,
) -> Result<Arc<XmtpApiClient>, GenericError> {
    init_logger();

    let client_mode = client_mode.unwrap_or_default();

    log::info!(
        v3_host,
        is_secure,
        "Creating API client for host: {}, gateway: {:?}, isSecure: {}",
        v3_host,
        gateway_host,
        is_secure
    );
    let mut backend = ClientBundle::builder();
    let backend = backend
        .v3_host(&v3_host)
        .maybe_gateway_host(gateway_host)
        .app_version(app_version.clone().unwrap_or_default())
        .is_secure(is_secure)
        .maybe_auth_callback(
            auth_callback
                .map(|callback| Arc::new(gateway_auth::FfiAuthCallbackBridge::new(callback)) as _),
        )
        .readonly(matches!(client_mode, FfiClientMode::Notification))
        .maybe_auth_handle(auth_handle.map(|handle| handle.as_ref().clone().into()))
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
    let backend = MessageBackendBuilder::default().from_bundle(api.0.clone())?;
    let api: ApiClientWrapper<xmtp_mls::XmtpApiClient> =
        ApiClientWrapper::new(backend, strategies::exponential_cooldown());
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
pub async fn get_newest_message_metadata(
    api: Arc<XmtpApiClient>,
    group_ids: Vec<Vec<u8>>,
) -> Result<HashMap<Vec<u8>, FfiMessageMetadata>, GenericError> {
    let backend = MessageBackendBuilder::default().from_bundle(api.0.clone())?;
    let api: ApiClientWrapper<xmtp_mls::XmtpApiClient> =
        ApiClientWrapper::new(backend, strategies::exponential_cooldown());

    let group_id_refs: Vec<&[u8]> = group_ids.iter().map(|id| id.as_slice()).collect();

    let metadata = api.get_newest_message_metadata(group_id_refs).await?;

    metadata
        .into_iter()
        .map(|(k, v)| Ok((k.to_vec(), FfiMessageMetadata::try_from(v)?)))
        .collect()
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
    let backend = MessageBackendBuilder::default().from_bundle(api.0.clone())?;
    let api: ApiClientWrapper<xmtp_mls::XmtpApiClient> =
        ApiClientWrapper::new(backend, strategies::exponential_cooldown());
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
    let backend = MessageBackendBuilder::default().from_bundle(api.0.clone())?;
    let api = ApiClientWrapper::new(backend, strategies::exponential_cooldown());
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
    fork_recovery_opts: Option<FfiForkRecoveryOpts>,
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

    let api_client: xmtp_mls::XmtpClientBundle = Arc::unwrap_or_clone(api).0;
    let sync_api_client: xmtp_mls::XmtpClientBundle = Arc::unwrap_or_clone(sync_api).0;
    let cursor_store = Arc::new(SqliteCursorStore::new(store.db()));
    let mut backend = MessageBackendBuilder::default();
    backend.cursor_store(cursor_store);
    let api_client = backend.clone().from_bundle(api_client)?;
    let sync_api_client = backend.from_bundle(sync_api_client)?;

    let mut builder = xmtp_mls::Client::builder(identity_strategy)
        .api_clients(api_client, sync_api_client)
        .enable_api_stats()?
        .enable_api_debug_wrapper()?
        .with_remote_verifier()?
        .with_allow_offline(allow_offline)
        .store(store);

    if let Some(sync_worker_mode) = device_sync_mode {
        builder = builder.device_sync_worker_mode(sync_worker_mode.into());
    }

    if let Some(fork_recovery_opts) = fork_recovery_opts {
        builder = builder.fork_recovery_opts(fork_recovery_opts.into());
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
    let backend = MessageBackendBuilder::default().from_bundle(api.0.clone())?;
    let mut api = ApiClientWrapper::new(backend, strategies::exponential_cooldown());
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

#[derive(Default, Clone, Copy, uniffi::Enum)]
pub enum FfiClientMode {
    #[default]
    Default,
    Notification,
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

    pub async fn fetch_inbox_updates_count(
        &self,
        refresh_from_network: bool,
        inbox_ids: Vec<String>,
    ) -> Result<HashMap<InboxId, u32>, GenericError> {
        let ids = inbox_ids.iter().map(AsRef::as_ref).collect();
        self.inner_client
            .fetch_inbox_updates_count(refresh_from_network, ids)
            .await
            .map_err(Into::into)
    }

    pub async fn fetch_own_inbox_updates_count(
        &self,
        refresh_from_network: bool,
    ) -> Result<u32, GenericError> {
        self.inner_client
            .fetch_own_inbox_updates_count(refresh_from_network)
            .await
            .map_err(Into::into)
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

    pub async fn sync_preferences(&self) -> Result<FfiGroupSyncSummary, GenericError> {
        let inner = self.inner_client.as_ref();
        let summary = inner.sync_all_welcomes_and_history_sync_groups().await?;

        Ok(summary.into())
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
     * Returns Some FfiSignatureRequest if we have installations to revoke.
     * If we have no other installations to revoke, returns None.
     */
    pub async fn revoke_all_other_installations_signature_request(
        &self,
    ) -> Result<Option<Arc<FfiSignatureRequest>>, GenericError> {
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
}

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct FfiGroupSyncSummary {
    pub num_eligible: u64,
    pub num_synced: u64,
}

impl From<xmtp_mls::groups::welcome_sync::GroupSyncSummary> for FfiGroupSyncSummary {
    fn from(summary: xmtp_mls::groups::welcome_sync::GroupSyncSummary) -> Self {
        Self {
            num_eligible: summary.num_eligible as u64,
            num_synced: summary.num_synced as u64,
        }
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
    pub update_app_data_policy: FfiPermissionPolicy,
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
        metadata_permissions_map.insert(
            MetadataField::AppData.to_string(),
            policy_set.update_app_data_policy.try_into()?,
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
    AppData,
}

impl From<&FfiMetadataField> for MetadataField {
    fn from(field: &FfiMetadataField) -> Self {
        match field {
            FfiMetadataField::GroupName => MetadataField::GroupName,
            FfiMetadataField::Description => MetadataField::Description,
            FfiMetadataField::ImageUrlSquare => MetadataField::GroupImageUrlSquare,
            FfiMetadataField::AppData => MetadataField::AppData,
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

    pub async fn create_group_by_identity(
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
            convo.add_members_by_identity(account_identities).await?;
        } else {
            convo.sync().await?;
        }

        Ok(convo)
    }

    pub async fn create_group(
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
            convo.add_members(inbox_ids).await?;
        } else {
            convo.sync().await?;
        };

        Ok(convo)
    }

    pub async fn find_or_create_dm_by_identity(
        &self,
        target_identity: FfiIdentifier,
        opts: FfiCreateDMOptions,
    ) -> Result<Arc<FfiConversation>, GenericError> {
        let target_identity = target_identity.try_into()?;
        log::info!("creating dm with target address: {target_identity:?}",);
        self.inner_client
            .find_or_create_dm_by_identity(target_identity, Some(opts.into_dm_metadata_options()))
            .await
            .map(|g| Arc::new(g.into()))
            .map_err(Into::into)
    }

    pub async fn find_or_create_dm(
        &self,
        inbox_id: String,
        opts: FfiCreateDMOptions,
    ) -> Result<Arc<FfiConversation>, GenericError> {
        log::info!("creating dm with target inbox_id: {}", inbox_id);
        self.inner_client
            .find_or_create_dm(inbox_id, Some(opts.into_dm_metadata_options()))
            .await
            .map(|g| Arc::new(g.into()))
            .map_err(Into::into)
    }

    pub async fn process_streamed_welcome_message(
        &self,
        envelope_bytes: Vec<u8>,
    ) -> Result<Vec<Arc<FfiConversation>>, GenericError> {
        self.inner_client
            .process_streamed_welcome_message(envelope_bytes)
            .await
            .map(|list| list.into_iter().map(|g| Arc::new(g.into())).collect())
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
    ) -> Result<FfiGroupSyncSummary, GenericError> {
        let inner = self.inner_client.as_ref();
        let consents: Option<Vec<ConsentState>> =
            consent_states.map(|states| states.into_iter().map(|state| state.into()).collect());
        let summary = inner.sync_all_welcomes_and_groups(consents).await?;

        Ok(summary.into())
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

    /// Get notified when a message is deleted by the disappearing messages worker.
    /// The callback receives the decoded message that was deleted.
    pub async fn stream_message_deletions(
        &self,
        callback: Arc<dyn FfiMessageDeletionCallback>,
    ) -> FfiStreamCloser {
        let handle = RustXmtpClient::stream_message_deletions_with_callback(
            self.inner_client.clone(),
            move |msg| {
                if let Ok(message) = msg {
                    let ffi_message: FfiDecodedMessage = message.into();
                    callback.on_message_deleted(Arc::new(ffi_message))
                }
            },
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

#[derive(uniffi::Enum, PartialEq, Debug)]
pub enum FfiGroupMembershipState {
    Allowed,
    Rejected,
    Pending,
    Restored,
    PendingRemove,
}

impl From<GroupMembershipState> for FfiGroupMembershipState {
    fn from(state: GroupMembershipState) -> Self {
        match state {
            GroupMembershipState::Allowed => FfiGroupMembershipState::Allowed,
            GroupMembershipState::Rejected => FfiGroupMembershipState::Rejected,
            GroupMembershipState::Pending => FfiGroupMembershipState::Pending,
            GroupMembershipState::Restored => FfiGroupMembershipState::Restored,
            GroupMembershipState::PendingRemove => FfiGroupMembershipState::PendingRemove,
        }
    }
}

impl From<FfiGroupMembershipState> for GroupMembershipState {
    fn from(state: FfiGroupMembershipState) -> Self {
        match state {
            FfiGroupMembershipState::Allowed => GroupMembershipState::Allowed,
            FfiGroupMembershipState::Rejected => GroupMembershipState::Rejected,
            FfiGroupMembershipState::Pending => GroupMembershipState::Pending,
            FfiGroupMembershipState::Restored => GroupMembershipState::Restored,
            FfiGroupMembershipState::PendingRemove => GroupMembershipState::PendingRemove,
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

#[derive(uniffi::Enum, Clone)]
pub enum FfiSortBy {
    SentAt,
    InsertedAt,
}

impl From<FfiSortBy> for SortBy {
    fn from(sort_by: FfiSortBy) -> Self {
        match sort_by {
            FfiSortBy::SentAt => SortBy::SentAt,
            FfiSortBy::InsertedAt => SortBy::InsertedAt,
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
    pub sort_by: Option<FfiSortBy>,
    pub inserted_after_ns: Option<i64>,
    pub inserted_before_ns: Option<i64>,
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
            sort_by: opts.sort_by.map(Into::into),
            inserted_after_ns: opts.inserted_after_ns,
            inserted_before_ns: opts.inserted_before_ns,
            exclude_disappearing: false,
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
    WalletSendCalls,
    LeaveRequest,
    Markdown,
    Actions,
    Intent,
    MultiRemoteAttachment,
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
            FfiContentType::WalletSendCalls => ContentType::WalletSendCalls,
            FfiContentType::LeaveRequest => ContentType::LeaveRequest,
            FfiContentType::Markdown => ContentType::Markdown,
            FfiContentType::Actions => ContentType::Actions,
            FfiContentType::Intent => ContentType::Intent,
            FfiContentType::MultiRemoteAttachment => ContentType::MultiRemoteAttachment,
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
    pub app_data: Option<String>,
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
            app_data: self.app_data,
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

    /// Delete a message by its ID. Returns the ID of the deletion message.
    pub fn delete_message(&self, message_id: Vec<u8>) -> Result<Vec<u8>, GenericError> {
        let deletion_id = self.inner.delete_message(message_id)?;
        Ok(deletion_id)
    }

    /// Publish all unpublished messages
    pub async fn publish_messages(&self) -> Result<(), GenericError> {
        self.inner.publish_messages().await?;
        Ok(())
    }

    /// Prepare a message for later publishing.
    /// Stores the message locally without publishing. Returns the message ID.
    pub fn prepare_message(
        &self,
        content_bytes: Vec<u8>,
        should_push: bool,
    ) -> Result<Vec<u8>, GenericError> {
        let id = self
            .inner
            .prepare_message_for_later_publish(content_bytes.as_slice(), should_push)?;
        Ok(id)
    }

    /// Publish a previously prepared message by ID.
    pub async fn publish_stored_message(&self, message_id: Vec<u8>) -> Result<(), GenericError> {
        self.inner.publish_stored_message(&message_id).await?;
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
    ) -> Result<Vec<FfiMessage>, FfiSubscribeError> {
        let message = self
            .inner
            .process_streamed_group_message(envelope_bytes)
            .await?;
        Ok(message.into_iter().map(Into::into).collect())
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

    pub fn membership_state(&self) -> Result<FfiGroupMembershipState, GenericError> {
        let state = self.inner.membership_state()?;
        Ok(state.into())
    }

    pub async fn add_members_by_identity(
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
            .add_members_by_identity(&account_identifiers)
            .await
            .map(FfiUpdateGroupMembershipResult::from)
            .map_err(Into::into)
    }

    pub async fn add_members(
        &self,
        inbox_ids: Vec<String>,
    ) -> Result<FfiUpdateGroupMembershipResult, GenericError> {
        log::info!("Adding members by inbox ID: {}", inbox_ids.join(", "));

        self.inner
            .add_members(&inbox_ids)
            .await
            .map(FfiUpdateGroupMembershipResult::from)
            .map_err(Into::into)
    }

    pub async fn remove_members_by_identity(
        &self,
        account_identifiers: Vec<FfiIdentifier>,
    ) -> Result<(), GenericError> {
        self.inner
            .remove_members_by_identity(&account_identifiers.to_internal()?)
            .await
            .map_err(Into::into)
    }

    pub async fn remove_members(&self, inbox_ids: Vec<String>) -> Result<(), GenericError> {
        let ids = inbox_ids.iter().map(AsRef::as_ref).collect::<Vec<&str>>();
        self.inner.remove_members(ids.as_slice()).await?;
        Ok(())
    }

    pub async fn leave_group(&self) -> Result<(), GenericError> {
        self.inner.leave_group().await?;
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

    pub async fn update_app_data(&self, app_data: String) -> Result<(), GenericError> {
        self.inner.update_app_data(app_data).await?;
        Ok(())
    }

    pub fn app_data(&self) -> Result<String, GenericError> {
        let app_data = self.inner.app_data()?;
        Ok(app_data)
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

// Intent FFI encode/decode functions

#[uniffi::export]
pub fn encode_intent(intent: FfiIntent) -> Result<Vec<u8>, GenericError> {
    let intent: Intent = intent.try_into()?;

    let encoded =
        IntentCodec::encode(intent).map_err(|e| GenericError::Generic { err: e.to_string() })?;

    let mut buf = Vec::new();
    encoded
        .encode(&mut buf)
        .map_err(|e| GenericError::Generic { err: e.to_string() })?;

    Ok(buf)
}

#[uniffi::export]
pub fn decode_intent(bytes: Vec<u8>) -> Result<FfiIntent, GenericError> {
    let encoded_content = EncodedContent::decode(bytes.as_slice())
        .map_err(|e| GenericError::Generic { err: e.to_string() })?;

    let intent = IntentCodec::decode(encoded_content)
        .map_err(|e| GenericError::Generic { err: e.to_string() })?;

    intent.try_into()
}

// Actions FFI encode/decode functions

#[uniffi::export]
pub fn encode_actions(actions: FfiActions) -> Result<Vec<u8>, GenericError> {
    let actions: Actions = actions.into();

    let encoded =
        ActionsCodec::encode(actions).map_err(|e| GenericError::Generic { err: e.to_string() })?;

    let mut buf = Vec::new();
    encoded
        .encode(&mut buf)
        .map_err(|e| GenericError::Generic { err: e.to_string() })?;

    Ok(buf)
}

#[uniffi::export]
pub fn decode_actions(bytes: Vec<u8>) -> Result<FfiActions, GenericError> {
    let encoded_content = EncodedContent::decode(bytes.as_slice())
        .map_err(|e| GenericError::Generic { err: e.to_string() })?;

    let actions = ActionsCodec::decode(encoded_content)
        .map_err(|e| GenericError::Generic { err: e.to_string() })?;

    actions.try_into()
}

// LeaveRequest FFI encode function
#[uniffi::export]
pub fn encode_leave_request(request: FfiLeaveRequest) -> Result<Vec<u8>, GenericError> {
    let leave_request: LeaveRequest = request.into();

    let encoded = LeaveRequestCodec::encode(leave_request)
        .map_err(|e| GenericError::Generic { err: e.to_string() })?;

    let mut buf = Vec::new();
    encoded
        .encode(&mut buf)
        .map_err(|e| GenericError::Generic { err: e.to_string() })?;

    Ok(buf)
}

// LeaveRequest FFI decode function
#[uniffi::export]
pub fn decode_leave_request(bytes: Vec<u8>) -> Result<FfiLeaveRequest, GenericError> {
    let encoded_content = EncodedContent::decode(bytes.as_slice())
        .map_err(|e| GenericError::Generic { err: e.to_string() })?;

    LeaveRequestCodec::decode(encoded_content)
        .map(Into::into)
        .map_err(|e| GenericError::Generic { err: e.to_string() })
}

#[uniffi::export]
pub fn decode_group_updated(bytes: Vec<u8>) -> Result<FfiGroupUpdated, GenericError> {
    let encoded_content = EncodedContent::decode(bytes.as_slice())
        .map_err(|e| GenericError::Generic { err: e.to_string() })?;

    GroupUpdatedCodec::decode(encoded_content)
        .map(Into::into)
        .map_err(|e| GenericError::Generic { err: e.to_string() })
}

#[uniffi::export]
pub fn encode_text(text: String) -> Result<Vec<u8>, GenericError> {
    let encoded =
        TextCodec::encode(text).map_err(|e| GenericError::Generic { err: e.to_string() })?;

    let mut buf = Vec::new();
    encoded
        .encode(&mut buf)
        .map_err(|e| GenericError::Generic { err: e.to_string() })?;

    Ok(buf)
}

#[uniffi::export]
pub fn decode_text(bytes: Vec<u8>) -> Result<String, GenericError> {
    let encoded_content = EncodedContent::decode(bytes.as_slice())
        .map_err(|e| GenericError::Generic { err: e.to_string() })?;

    TextCodec::decode(encoded_content).map_err(|e| GenericError::Generic { err: e.to_string() })
}

#[uniffi::export]
pub fn encode_markdown(text: String) -> Result<Vec<u8>, GenericError> {
    let encoded =
        MarkdownCodec::encode(text).map_err(|e| GenericError::Generic { err: e.to_string() })?;

    let mut buf = Vec::new();
    encoded
        .encode(&mut buf)
        .map_err(|e| GenericError::Generic { err: e.to_string() })?;

    Ok(buf)
}

#[uniffi::export]
pub fn decode_markdown(bytes: Vec<u8>) -> Result<String, GenericError> {
    let encoded_content = EncodedContent::decode(bytes.as_slice())
        .map_err(|e| GenericError::Generic { err: e.to_string() })?;

    MarkdownCodec::decode(encoded_content).map_err(|e| GenericError::Generic { err: e.to_string() })
}

#[uniffi::export]
pub fn encode_wallet_send_calls(
    wallet_send_calls: FfiWalletSendCalls,
) -> Result<Vec<u8>, GenericError> {
    let encoded = WalletSendCallsCodec::encode(wallet_send_calls.into())
        .map_err(|e| GenericError::Generic { err: e.to_string() })?;

    let mut buf = Vec::new();
    encoded
        .encode(&mut buf)
        .map_err(|e| GenericError::Generic { err: e.to_string() })?;

    Ok(buf)
}

#[uniffi::export]
pub fn decode_wallet_send_calls(bytes: Vec<u8>) -> Result<FfiWalletSendCalls, GenericError> {
    let encoded_content = EncodedContent::decode(bytes.as_slice())
        .map_err(|e| GenericError::Generic { err: e.to_string() })?;

    WalletSendCallsCodec::decode(encoded_content)
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
    pub inserted_at_ns: i64,
    pub expire_at_ns: Option<i64>,
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
            inserted_at_ns: msg.inserted_at_ns,
            expire_at_ns: msg.expire_at_ns,
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

#[uniffi::export(with_foreign)]
pub trait FfiMessageDeletionCallback: Send + Sync {
    fn on_message_deleted(&self, message: Arc<FfiDecodedMessage>);
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
            update_app_data_policy: get_policy(MetadataField::AppData.as_str()),
        })
    }
}

#[cfg(test)]
pub mod tests;
