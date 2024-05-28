pub use crate::inbox_owner::SigningError;
use crate::logger::init_logger;
use crate::logger::FfiLogger;
use crate::GenericError;
use std::collections::HashMap;
use std::convert::TryInto;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tokio::sync::oneshot::Sender;
use xmtp_api_grpc::grpc_api_helper::Client as TonicApiClient;
use xmtp_id::associations::builder::SignatureRequest;
use xmtp_id::associations::generate_inbox_id as xmtp_id_generate_inbox_id;
use xmtp_id::associations::Erc1271Signature;
use xmtp_id::associations::RecoverableEcdsaSignature;
use xmtp_id::InboxId;
use xmtp_mls::api::ApiClientWrapper;
use xmtp_mls::groups::group_metadata::ConversationType;
use xmtp_mls::groups::group_metadata::GroupMetadata;
use xmtp_mls::groups::group_permissions::GroupMutablePermissions;
use xmtp_mls::groups::PreconfiguredPolicies;
use xmtp_mls::groups::UpdateAdminListType;
use xmtp_mls::identity::IdentityStrategy;
use xmtp_mls::retry::Retry;
use xmtp_mls::{
    builder::ClientBuilder,
    client::Client as MlsClient,
    groups::MlsGroup,
    storage::{
        group_message::DeliveryStatus, group_message::GroupMessageKind,
        group_message::StoredGroupMessage, EncryptedMessageStore, EncryptionKey, StorageOption,
    },
};

pub type RustXmtpClient = MlsClient<TonicApiClient>;

/// XMTP SDK's may embed libxmtp (v3) alongside existing v2 protocol logic
/// for backwards-compatibility purposes. In this case, the client may already
/// have a wallet-signed v2 key. Depending on the source of this key,
/// libxmtp may choose to bootstrap v3 installation keys using the existing
/// legacy key.
#[derive(uniffi::Enum)]
pub enum LegacyIdentitySource {
    // A client with no support for v2 messages
    None,
    // A cached v2 key was provided on client initialization
    Static,
    // A private bundle exists on the network from which the v2 key was fetched
    Network,
    // A new v2 key was generated on client initialization
    KeyGenerator,
}

#[allow(clippy::too_many_arguments)]
#[allow(unused)]
#[uniffi::export(async_runtime = "tokio")]
pub async fn create_client(
    logger: Box<dyn FfiLogger>,
    host: String,
    is_secure: bool,
    db: Option<String>,
    encryption_key: Option<Vec<u8>>,
    account_address: String,
    legacy_identity_source: LegacyIdentitySource,
    legacy_signed_private_key_proto: Option<Vec<u8>>,
) -> Result<Arc<FfiXmtpClient>, GenericError> {
    log::info!(
        "Creating API client for host: {}, isSecure: {}",
        host,
        is_secure
    );
    let api_client = TonicApiClient::create(host.clone(), is_secure).await?;

    log::info!(
        "Creating message store with path: {:?} and encryption key: {}",
        db,
        encryption_key.is_some()
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
            EncryptedMessageStore::new(storage_option, key)?
        }
        None => EncryptedMessageStore::new_unencrypted(storage_option)?,
    };

    log::info!("Creating XMTP client");
    let legacy_key_result =
        legacy_signed_private_key_proto.ok_or("No legacy key provided".to_string());
    // TODO: uncomment
    // let legacy_identity = match legacy_identity_source {
    //     LegacyIdentitySource::None => LegacyIdentity::None,
    //     LegacyIdentitySource::Static => LegacyIdentity::Static(legacy_key_result?),
    //     LegacyIdentitySource::Network => LegacyIdentity::Network(legacy_key_result?),
    //     LegacyIdentitySource::KeyGenerator => LegacyIdentity::KeyGenerator(legacy_key_result?),
    // };
    let identity_strategy =
        IdentityStrategy::CreateIfNotFound(account_address.clone().to_lowercase(), None);
    let xmtp_client: RustXmtpClient = ClientBuilder::new(identity_strategy)
        .api_client(api_client)
        .store(store)
        .build()
        .await?;

    log::info!(
        "Created XMTP client for inbox_id: {}",
        xmtp_client.inbox_id()
    );
    Ok(Arc::new(FfiXmtpClient {
        inner_client: Arc::new(xmtp_client),
        account_address,
    }))
}

#[allow(unused)]
#[uniffi::export(async_runtime = "tokio")]
pub async fn get_inbox_id_for_address(
    logger: Box<dyn FfiLogger>,
    host: String,
    is_secure: bool,
    account_address: String,
) -> Result<Option<String>, GenericError> {
    init_logger(logger);

    let api_client = ApiClientWrapper::new(
        TonicApiClient::create(host.clone(), is_secure).await?,
        Retry::default(),
    );

    let results = api_client
        .get_inbox_ids(vec![account_address.clone()])
        .await
        .map_err(GenericError::from_error)?;

    Ok(results.get(&account_address).cloned())
}

#[allow(unused)]
#[uniffi::export]
pub fn generate_inbox_id(account_address: String, nonce: u64) -> String {
    xmtp_id_generate_inbox_id(&account_address, &nonce)
}

#[derive(uniffi::Object)]
pub struct FfiSignatureRequest {
    // Using `tokio::sync::Mutex`bc rust MutexGuard cannot be sent between threads.
    inner: Arc<tokio::sync::Mutex<SignatureRequest>>,
}

#[uniffi::export(async_runtime = "tokio")]
impl FfiSignatureRequest {
    // Signature that's signed by EOA wallet
    pub async fn add_ecdsa_signature(&self, signature_bytes: Vec<u8>) -> Result<(), GenericError> {
        let mut inner = self.inner.lock().await;
        let signature_text = inner.signature_text();
        inner
            .add_signature(Box::new(RecoverableEcdsaSignature::new(
                signature_text,
                signature_bytes,
            )))
            .await?;

        Ok(())
    }

    pub async fn add_erc1271_signature(
        &self,
        signature_bytes: Vec<u8>,
        address: String,
        chain_rpc_url: String,
    ) -> Result<(), GenericError> {
        let mut inner = self.inner.lock().await;
        let erc1271_signature = Erc1271Signature::new_with_rpc(
            inner.signature_text(),
            signature_bytes,
            address,
            chain_rpc_url,
        )
        .await?;
        inner.add_signature(Box::new(erc1271_signature)).await?;
        Ok(())
    }

    pub async fn signature_text(&self) -> Result<String, GenericError> {
        Ok(self.inner.lock().await.signature_text())
    }

    /// missing signatures that are from [MemberKind::Address]
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
    account_address: String,
}

#[uniffi::export(async_runtime = "tokio")]
impl FfiXmtpClient {
    pub fn inbox_id(&self) -> InboxId {
        self.inner_client.inbox_id()
    }

    pub fn conversations(&self) -> Arc<FfiConversations> {
        Arc::new(FfiConversations {
            inner_client: self.inner_client.clone(),
        })
    }

    pub async fn can_message(
        &self,
        account_addresses: Vec<String>,
    ) -> Result<HashMap<String, bool>, GenericError> {
        let inner = self.inner_client.as_ref();

        let results: HashMap<String, bool> = inner.can_message(account_addresses).await?;

        Ok(results)
    }

    pub fn installation_id(&self) -> Vec<u8> {
        self.inner_client.installation_public_key()
    }

    pub fn release_db_connection(&self) -> Result<(), GenericError> {
        Ok(self.inner_client.release_db_connection()?)
    }

    pub async fn db_reconnect(&self) -> Result<(), GenericError> {
        Ok(self.inner_client.reconnect_db()?)
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl FfiXmtpClient {
    pub fn signature_request(&self) -> Option<Arc<FfiSignatureRequest>> {
        self.inner_client
            .identity()
            .signature_request()
            .map(|request| {
                Arc::new(FfiSignatureRequest {
                    inner: Arc::new(tokio::sync::Mutex::new(request)),
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
}

#[derive(uniffi::Record)]
pub struct FfiListConversationsOptions {
    pub created_after_ns: Option<i64>,
    pub created_before_ns: Option<i64>,
    pub limit: Option<i64>,
}

#[derive(uniffi::Object)]
pub struct FfiConversations {
    inner_client: Arc<RustXmtpClient>,
}

#[derive(uniffi::Enum)]
pub enum GroupPermissions {
    AllMembers,
    AdminOnly,
}

impl From<PreconfiguredPolicies> for GroupPermissions {
    fn from(policy: PreconfiguredPolicies) -> Self {
        match policy {
            PreconfiguredPolicies::AllMembers => GroupPermissions::AllMembers,
            PreconfiguredPolicies::AdminsOnly => GroupPermissions::AdminOnly,
        }
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl FfiConversations {
    pub async fn create_group(
        &self,
        account_addresses: Vec<String>,
        permissions: Option<GroupPermissions>,
    ) -> Result<Arc<FfiGroup>, GenericError> {
        log::info!(
            "creating group with account addresses: {}",
            account_addresses.join(", ")
        );

        let group_permissions = match permissions {
            Some(GroupPermissions::AllMembers) => {
                Some(xmtp_mls::groups::PreconfiguredPolicies::AllMembers)
            }
            Some(GroupPermissions::AdminOnly) => {
                Some(xmtp_mls::groups::PreconfiguredPolicies::AdminsOnly)
            }
            _ => None,
        };

        let convo = self.inner_client.create_group(group_permissions)?;
        if !account_addresses.is_empty() {
            convo
                .add_members(&self.inner_client, account_addresses)
                .await?;
        }
        let out = Arc::new(FfiGroup {
            inner_client: self.inner_client.clone(),
            group_id: convo.group_id,
            created_at_ns: convo.created_at_ns,
        });

        Ok(out)
    }

    pub fn process_streamed_welcome_message(
        &self,
        envelope_bytes: Vec<u8>,
    ) -> Result<Arc<FfiGroup>, GenericError> {
        let inner = self.inner_client.as_ref();
        let group = inner.process_streamed_welcome_message(envelope_bytes)?;
        let out = Arc::new(FfiGroup {
            inner_client: self.inner_client.clone(),
            group_id: group.group_id,
            created_at_ns: group.created_at_ns,
        });
        Ok(out)
    }

    pub async fn sync(&self) -> Result<(), GenericError> {
        let inner = self.inner_client.as_ref();
        inner.sync_welcomes().await?;
        Ok(())
    }

    pub async fn list(
        &self,
        opts: FfiListConversationsOptions,
    ) -> Result<Vec<Arc<FfiGroup>>, GenericError> {
        let inner = self.inner_client.as_ref();
        let convo_list: Vec<Arc<FfiGroup>> = inner
            .find_groups(
                None,
                opts.created_after_ns,
                opts.created_before_ns,
                opts.limit,
            )?
            .into_iter()
            .map(|group| {
                Arc::new(FfiGroup {
                    inner_client: self.inner_client.clone(),
                    group_id: group.group_id,
                    created_at_ns: group.created_at_ns,
                })
            })
            .collect();

        Ok(convo_list)
    }

    pub async fn stream(
        &self,
        callback: Box<dyn FfiConversationCallback>,
    ) -> Result<Arc<FfiStreamCloser>, GenericError> {
        let client = self.inner_client.clone();
        let stream_closer = RustXmtpClient::stream_conversations_with_callback(
            client.clone(),
            move |convo| {
                callback.on_conversation(Arc::new(FfiGroup {
                    inner_client: client.clone(),
                    group_id: convo.group_id,
                    created_at_ns: convo.created_at_ns,
                }))
            },
            || {}, // on_close_callback
        )?;

        Ok(Arc::new(FfiStreamCloser {
            close_fn: stream_closer.close_fn,
            is_closed_atomic: stream_closer.is_closed_atomic,
        }))
    }

    pub async fn stream_all_messages(
        &self,
        message_callback: Box<dyn FfiMessageCallback>,
    ) -> Result<Arc<FfiStreamCloser>, GenericError> {
        let stream_closer = RustXmtpClient::stream_all_messages_with_callback(
            self.inner_client.clone(),
            move |message| message_callback.on_message(message.into()),
        )
        .await?;

        Ok(Arc::new(FfiStreamCloser {
            close_fn: stream_closer.close_fn,
            is_closed_atomic: stream_closer.is_closed_atomic,
        }))
    }
}

#[derive(uniffi::Object)]
pub struct FfiGroup {
    inner_client: Arc<RustXmtpClient>,
    group_id: Vec<u8>,
    created_at_ns: i64,
}

#[derive(uniffi::Record)]
pub struct FfiGroupMember {
    pub inbox_id: String,
    pub account_addresses: Vec<String>,
    pub installation_ids: Vec<Vec<u8>>,
}

#[derive(uniffi::Record)]
pub struct FfiListMessagesOptions {
    pub sent_before_ns: Option<i64>,
    pub sent_after_ns: Option<i64>,
    pub limit: Option<i64>,
    pub delivery_status: Option<FfiDeliveryStatus>,
}

#[uniffi::export(async_runtime = "tokio")]
impl FfiGroup {
    pub async fn send(&self, content_bytes: Vec<u8>) -> Result<Vec<u8>, GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        let message_id = group
            .send_message(content_bytes.as_slice(), &self.inner_client)
            .await?;
        Ok(message_id)
    }

    pub async fn sync(&self) -> Result<(), GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        group.sync(&self.inner_client).await?;

        Ok(())
    }

    pub fn find_messages(
        &self,
        opts: FfiListMessagesOptions,
    ) -> Result<Vec<FfiMessage>, GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        let delivery_status = opts.delivery_status.map(|status| status.into());

        let messages: Vec<FfiMessage> = group
            .find_messages(
                None,
                opts.sent_before_ns,
                opts.sent_after_ns,
                delivery_status,
                opts.limit,
            )?
            .into_iter()
            .map(|msg| msg.into())
            .collect();

        Ok(messages)
    }

    pub async fn process_streamed_group_message(
        &self,
        envelope_bytes: Vec<u8>,
    ) -> Result<FfiMessage, GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );
        let message = group
            .process_streamed_group_message(envelope_bytes, self.inner_client.clone())
            .await?;
        let ffi_message = message.into();

        Ok(ffi_message)
    }

    pub fn list_members(&self) -> Result<Vec<FfiGroupMember>, GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        let members: Vec<FfiGroupMember> = group
            .members()?
            .into_iter()
            .map(|member| FfiGroupMember {
                inbox_id: member.inbox_id,
                account_addresses: member.account_addresses,
                installation_ids: member.installation_ids,
            })
            .collect();

        Ok(members)
    }

    pub async fn add_members(&self, account_addresses: Vec<String>) -> Result<(), GenericError> {
        log::info!("adding members: {}", account_addresses.join(","));

        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        group
            .add_members(&self.inner_client, account_addresses)
            .await?;

        Ok(())
    }

    pub async fn add_members_by_inbox_id(
        &self,
        inbox_ids: Vec<String>,
    ) -> Result<(), GenericError> {
        log::info!("adding members by inbox id: {}", inbox_ids.join(","));

        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        group
            .add_members_by_inbox_id(&self.inner_client, inbox_ids)
            .await?;

        Ok(())
    }

    pub async fn remove_members(&self, account_addresses: Vec<String>) -> Result<(), GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        group
            .remove_members(&self.inner_client, account_addresses)
            .await?;

        Ok(())
    }

    pub async fn remove_members_by_inbox_id(
        &self,
        inbox_ids: Vec<String>,
    ) -> Result<(), GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        group
            .remove_members_by_inbox_id(&self.inner_client, inbox_ids)
            .await?;

        Ok(())
    }

    pub async fn update_group_name(&self, group_name: String) -> Result<(), GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        group
            .update_group_name(&self.inner_client, group_name)
            .await?;

        Ok(())
    }

    pub fn group_name(&self) -> Result<String, GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        let group_name = group.group_name()?;

        Ok(group_name)
    }

    pub fn admin_list(&self) -> Result<Vec<String>, GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        let admin_list = group.admin_list()?;

        Ok(admin_list)
    }

    pub fn super_admin_list(&self) -> Result<Vec<String>, GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        let super_admin_list = group.super_admin_list()?;

        Ok(super_admin_list)
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
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );
        group.update_admin_list(&self.inner_client,  UpdateAdminListType::Add, inbox_id).await?;

        Ok(())
    }

    pub async fn remove_admin(&self, inbox_id: String) -> Result<(), GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );
        group.update_admin_list(&self.inner_client,  UpdateAdminListType::Remove, inbox_id).await?;

        Ok(())
    }

    pub async fn add_super_admin(&self, inbox_id: String) -> Result<(), GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );
        group.update_admin_list(&self.inner_client,  UpdateAdminListType::AddSuper, inbox_id).await?;

        Ok(())
    }

    pub async fn remove_super_admin(&self, inbox_id: String) -> Result<(), GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );
        group.update_admin_list(&self.inner_client,  UpdateAdminListType::RemoveSuper, inbox_id).await?;

        Ok(())
    }

    pub fn group_permissions(&self) -> Result<Arc<FfiGroupPermissions>, GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        let permissions = group.permissions()?;
        Ok(Arc::new(FfiGroupPermissions {
            inner: Arc::new(permissions),
        }))
    }

    pub async fn stream(
        &self,
        message_callback: Box<dyn FfiMessageCallback>,
    ) -> Result<Arc<FfiStreamCloser>, GenericError> {
        let inner_client = Arc::clone(&self.inner_client);
        let stream_closer = MlsGroup::stream_with_callback(
            inner_client,
            self.group_id.clone(),
            self.created_at_ns,
            move |message| message_callback.on_message(message.into()),
        )?;

        Ok(Arc::new(FfiStreamCloser {
            close_fn: stream_closer.close_fn,
            is_closed_atomic: stream_closer.is_closed_atomic,
        }))
    }

    pub fn created_at_ns(&self) -> i64 {
        self.created_at_ns
    }

    pub fn is_active(&self) -> Result<bool, GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        Ok(group.is_active()?)
    }

    pub fn added_by_inbox_id(&self) -> Result<String, GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        Ok(group.added_by_inbox_id()?)
    }

    pub fn group_metadata(&self) -> Result<Arc<FfiGroupMetadata>, GenericError> {
        let group = MlsGroup::new(
            self.inner_client.context().clone(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        let metadata = group.metadata()?;
        Ok(Arc::new(FfiGroupMetadata {
            inner: Arc::new(metadata),
        }))
    }
}

#[uniffi::export]
impl FfiGroup {
    pub fn id(&self) -> Vec<u8> {
        self.group_id.clone()
    }
}

#[derive(uniffi::Enum)]
pub enum FfiGroupMessageKind {
    Application,
    MembershipChange,
}

impl From<GroupMessageKind> for FfiGroupMessageKind {
    fn from(kind: GroupMessageKind) -> Self {
        match kind {
            GroupMessageKind::Application => FfiGroupMessageKind::Application,
            GroupMessageKind::MembershipChange => FfiGroupMessageKind::MembershipChange,
        }
    }
}

#[derive(uniffi::Enum)]
pub enum FfiDeliveryStatus {
    Unpublished,
    Published,
    Failed,
}

impl From<DeliveryStatus> for FfiDeliveryStatus {
    fn from(status: DeliveryStatus) -> Self {
        match status {
            DeliveryStatus::Unpublished => FfiDeliveryStatus::Unpublished,
            DeliveryStatus::Published => FfiDeliveryStatus::Published,
            DeliveryStatus::Failed => FfiDeliveryStatus::Failed,
        }
    }
}

impl From<FfiDeliveryStatus> for DeliveryStatus {
    fn from(status: FfiDeliveryStatus) -> Self {
        match status {
            FfiDeliveryStatus::Unpublished => DeliveryStatus::Unpublished,
            FfiDeliveryStatus::Published => DeliveryStatus::Published,
            FfiDeliveryStatus::Failed => DeliveryStatus::Failed,
        }
    }
}

#[derive(uniffi::Record)]
pub struct FfiMessage {
    pub id: Vec<u8>,
    pub sent_at_ns: i64,
    pub convo_id: Vec<u8>,
    pub sender_inbox_id: String,
    pub content: Vec<u8>,
    pub kind: FfiGroupMessageKind,
    pub delivery_status: FfiDeliveryStatus,
}

impl From<StoredGroupMessage> for FfiMessage {
    fn from(msg: StoredGroupMessage) -> Self {
        Self {
            id: msg.id,
            sent_at_ns: msg.sent_at_ns,
            convo_id: msg.group_id,
            sender_inbox_id: msg.sender_inbox_id,
            content: msg.decrypted_message_bytes,
            kind: msg.kind.into(),
            delivery_status: msg.delivery_status.into(),
        }
    }
}

#[derive(uniffi::Object)]
pub struct FfiStreamCloser {
    close_fn: Arc<Mutex<Option<Sender<()>>>>,
    is_closed_atomic: Arc<AtomicBool>,
}

#[uniffi::export]
impl FfiStreamCloser {
    pub fn end(&self) {
        match self.close_fn.lock() {
            Ok(mut close_fn_option) => {
                let _ = close_fn_option.take().map(|close_fn| close_fn.send(()));
            }
            _ => {
                log::warn!("close_fn already closed");
            }
        }
    }

    pub fn is_closed(&self) -> bool {
        self.is_closed_atomic.load(Ordering::Relaxed)
    }
}

#[uniffi::export(callback_interface)]
pub trait FfiMessageCallback: Send + Sync {
    fn on_message(&self, message: FfiMessage);
}

#[uniffi::export(callback_interface)]
pub trait FfiConversationCallback: Send + Sync {
    fn on_conversation(&self, conversation: Arc<FfiGroup>);
}

#[derive(uniffi::Object)]
pub struct FfiGroupMetadata {
    inner: Arc<GroupMetadata>,
}

#[uniffi::export]
impl FfiGroupMetadata {
    pub fn creator_inbox_id(&self) -> String {
        self.inner.creator_inbox_id.clone()
    }

    pub fn conversation_type(&self) -> String {
        match self.inner.conversation_type {
            ConversationType::Group => "group".to_string(),
            ConversationType::Dm => "dm".to_string(),
            ConversationType::Sync => "sync".to_string(),
        }
    }
}

#[derive(uniffi::Object)]
pub struct FfiGroupPermissions {
    inner: Arc<GroupMutablePermissions>,
}

#[uniffi::export]
impl FfiGroupPermissions {
    pub fn policy_type(&self) -> Result<GroupPermissions, GenericError> {
        Ok(self.inner.preconfigured_policy()?.into())
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        get_inbox_id_for_address, inbox_owner::SigningError, logger::FfiLogger,
        FfiConversationCallback, FfiInboxOwner, LegacyIdentitySource,
    };
    use std::{
        env,
        sync::{
            atomic::{AtomicU32, Ordering},
            Arc,
        },
    };

    use super::{create_client, FfiMessage, FfiMessageCallback, FfiXmtpClient};
    use ethers::utils::hex;
    use ethers_core::rand::{
        self,
        distributions::{Alphanumeric, DistString},
    };
    use xmtp_cryptography::{signature::RecoverableSignature, utils::rng};
    use xmtp_mls::{storage::EncryptionKey, InboxOwner};

    #[derive(Clone)]
    pub struct LocalWalletInboxOwner {
        wallet: xmtp_cryptography::utils::LocalWallet,
    }

    impl LocalWalletInboxOwner {
        pub fn new() -> Self {
            Self {
                wallet: xmtp_cryptography::utils::LocalWallet::new(&mut rng()),
            }
        }
    }

    impl FfiInboxOwner for LocalWalletInboxOwner {
        fn get_address(&self) -> String {
            self.wallet.get_address()
        }

        fn sign(&self, text: String) -> Result<Vec<u8>, SigningError> {
            let recoverable_signature =
                self.wallet.sign(&text).map_err(|_| SigningError::Generic)?;
            match recoverable_signature {
                RecoverableSignature::Eip191Signature(signature_bytes) => Ok(signature_bytes),
            }
        }
    }

    pub struct MockLogger {}

    impl FfiLogger for MockLogger {
        fn log(&self, _level: u32, level_label: String, message: String) {
            println!("{}: {}", level_label, message)
        }
    }

    #[derive(Clone)]
    struct RustStreamCallback {
        num_messages: Arc<AtomicU32>,
    }

    impl RustStreamCallback {
        pub fn new() -> Self {
            Self {
                num_messages: Arc::new(AtomicU32::new(0)),
            }
        }

        pub fn message_count(&self) -> u32 {
            self.num_messages.load(Ordering::SeqCst)
        }
    }

    impl FfiMessageCallback for RustStreamCallback {
        fn on_message(&self, message: FfiMessage) {
            let message = String::from_utf8(message.content).unwrap_or("<not UTF8>".to_string());
            log::info!("Received: {}", message);
            let _ = self.num_messages.fetch_add(1, Ordering::SeqCst);
        }
    }

    impl FfiConversationCallback for RustStreamCallback {
        fn on_conversation(&self, _: Arc<super::FfiGroup>) {
            let _ = self.num_messages.fetch_add(1, Ordering::SeqCst);
        }
    }

    pub fn rand_string() -> String {
        Alphanumeric.sample_string(&mut rand::thread_rng(), 24)
    }

    pub fn tmp_path() -> String {
        let db_name = rand_string();
        format!("{}/{}.db3", env::temp_dir().to_str().unwrap(), db_name)
    }

    fn static_enc_key() -> EncryptionKey {
        [2u8; 32]
    }

    async fn register_client(inbox_owner: &LocalWalletInboxOwner, client: &FfiXmtpClient) {
        let signature_request = client.signature_request().unwrap();
        signature_request
            .add_ecdsa_signature(
                inbox_owner
                    .sign(signature_request.signature_text().await.unwrap())
                    .unwrap(),
            )
            .await
            .unwrap();
        client.register_identity(signature_request).await.unwrap();
    }

    async fn new_test_client() -> Arc<FfiXmtpClient> {
        let ffi_inbox_owner = LocalWalletInboxOwner::new();

        let client = create_client(
            Box::new(MockLogger {}),
            xmtp_api_grpc::LOCALHOST_ADDRESS.to_string(),
            false,
            Some(tmp_path()),
            None,
            ffi_inbox_owner.get_address(),
            LegacyIdentitySource::None,
            None,
        )
        .await
        .unwrap();
        register_client(&ffi_inbox_owner, &client).await;
        return client;
    }

    #[tokio::test]
    async fn get_inbox_id() {
        let client = new_test_client().await;
        let real_inbox_id = client.inbox_id();

        let from_network = get_inbox_id_for_address(
            Box::new(MockLogger {}),
            xmtp_api_grpc::LOCALHOST_ADDRESS.to_string(),
            false,
            client.account_address.clone(),
        )
        .await
        .unwrap()
        .unwrap();

        assert_eq!(real_inbox_id, from_network);
    }

    // Try a query on a test topic, and make sure we get a response
    #[tokio::test]
    async fn test_client_creation() {
        let client = new_test_client().await;
        assert!(!client.signature_request().is_none());
    }

    #[tokio::test]
    #[ignore]
    async fn test_legacy_identity() {
        let account_address = "0x0bD00B21aF9a2D538103c3AAf95Cb507f8AF1B28".to_lowercase();
        let legacy_keys = hex::decode("0880bdb7a8b3f6ede81712220a20ad528ea38ce005268c4fb13832cfed13c2b2219a378e9099e48a38a30d66ef991a96010a4c08aaa8e6f5f9311a430a41047fd90688ca39237c2899281cdf2756f9648f93767f91c0e0f74aed7e3d3a8425e9eaa9fa161341c64aa1c782d004ff37ffedc887549ead4a40f18d1179df9dff124612440a403c2cb2338fb98bfe5f6850af11f6a7e97a04350fc9d37877060f8d18e8f66de31c77b3504c93cf6a47017ea700a48625c4159e3f7e75b52ff4ea23bc13db77371001").unwrap();

        let client = create_client(
            Box::new(MockLogger {}),
            xmtp_api_grpc::LOCALHOST_ADDRESS.to_string(),
            false,
            Some(tmp_path()),
            None,
            account_address.to_string(),
            LegacyIdentitySource::KeyGenerator,
            Some(legacy_keys),
        )
        .await
        .unwrap();

        assert!(client.signature_request().is_none());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_create_client_with_storage() {
        let ffi_inbox_owner = LocalWalletInboxOwner::new();

        let path = tmp_path();

        let client_a = create_client(
            Box::new(MockLogger {}),
            xmtp_api_grpc::LOCALHOST_ADDRESS.to_string(),
            false,
            Some(path.clone()),
            None,
            ffi_inbox_owner.get_address(),
            LegacyIdentitySource::None,
            None,
        )
        .await
        .unwrap();
        register_client(&ffi_inbox_owner, &client_a).await;

        let installation_pub_key = client_a.inner_client.installation_public_key();
        drop(client_a);

        let client_b = create_client(
            Box::new(MockLogger {}),
            xmtp_api_grpc::LOCALHOST_ADDRESS.to_string(),
            false,
            Some(path),
            None,
            ffi_inbox_owner.get_address(),
            LegacyIdentitySource::None,
            None,
        )
        .await
        .unwrap();

        let other_installation_pub_key = client_b.inner_client.installation_public_key();
        drop(client_b);

        assert!(
            installation_pub_key == other_installation_pub_key,
            "did not use same installation ID"
        )
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_create_client_with_key() {
        let ffi_inbox_owner = LocalWalletInboxOwner::new();

        let path = tmp_path();

        let key = static_enc_key().to_vec();

        let client_a = create_client(
            Box::new(MockLogger {}),
            xmtp_api_grpc::LOCALHOST_ADDRESS.to_string(),
            false,
            Some(path.clone()),
            Some(key),
            ffi_inbox_owner.get_address(),
            LegacyIdentitySource::None,
            None,
        )
        .await
        .unwrap();

        drop(client_a);

        let mut other_key = static_enc_key();
        other_key[31] = 1;

        let result_errored = create_client(
            Box::new(MockLogger {}),
            xmtp_api_grpc::LOCALHOST_ADDRESS.to_string(),
            false,
            Some(path),
            Some(other_key.to_vec()),
            ffi_inbox_owner.get_address(),
            LegacyIdentitySource::None,
            None,
        )
        .await
        .is_err();

        assert!(result_errored, "did not error on wrong encryption key")
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_create_group_with_members() {
        let amal = new_test_client().await;
        let bola = new_test_client().await;

        let group = amal
            .conversations()
            .create_group(vec![bola.account_address.clone()], None)
            .await
            .unwrap();

        let members = group.list_members().unwrap();
        assert_eq!(members.len(), 2);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_invalid_external_signature() {
        let inbox_owner = LocalWalletInboxOwner::new();
        let path = tmp_path();

        let client = create_client(
            Box::new(MockLogger {}),
            xmtp_api_grpc::LOCALHOST_ADDRESS.to_string(),
            false,
            Some(path.clone()),
            None, // encryption_key
            inbox_owner.get_address(),
            LegacyIdentitySource::None,
            None, // v2_signed_private_key_proto
        )
        .await
        .unwrap();

        let signature_request = client.signature_request().unwrap();
        assert!(client.register_identity(signature_request).await.is_err());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_can_message() {
        let amal = LocalWalletInboxOwner::new();
        let bola = LocalWalletInboxOwner::new();
        let path = tmp_path();

        let client_amal = create_client(
            Box::new(MockLogger {}),
            xmtp_api_grpc::LOCALHOST_ADDRESS.to_string(),
            false,
            Some(path.clone()),
            None,
            amal.get_address(),
            LegacyIdentitySource::None,
            None,
        )
        .await
        .unwrap();
        let can_message_result = client_amal
            .can_message(vec![bola.get_address()])
            .await
            .unwrap();

        assert!(
            can_message_result
                .get(&bola.get_address().to_string())
                .map(|&value| !value)
                .unwrap_or(false),
            "Expected the can_message result to be false for the address"
        );

        let client_bola = create_client(
            Box::new(MockLogger {}),
            xmtp_api_grpc::LOCALHOST_ADDRESS.to_string(),
            false,
            Some(path.clone()),
            None,
            bola.get_address(),
            LegacyIdentitySource::None,
            None,
        )
        .await
        .unwrap();
        register_client(&bola, &client_bola).await;

        let can_message_result2 = client_amal
            .can_message(vec![bola.get_address()])
            .await
            .unwrap();

        assert!(
            can_message_result2
                .get(&bola.get_address().to_string())
                .copied()
                .unwrap_or(false),
            "Expected the can_message result to be true for the address"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    // This one is flaky for me. Passes reliably locally and fails on CI
    #[ignore]
    async fn test_conversation_streaming() {
        let amal = new_test_client().await;
        let bola = new_test_client().await;

        let stream_callback = RustStreamCallback::new();

        let stream = bola
            .conversations()
            .stream(Box::new(stream_callback.clone()))
            .await
            .unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        amal.conversations()
            .create_group(vec![bola.account_address.clone()], None)
            .await
            .unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

        assert_eq!(stream_callback.message_count(), 1);
        // Create another group and add bola
        amal.conversations()
            .create_group(vec![bola.account_address.clone()], None)
            .await
            .unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        assert_eq!(stream_callback.message_count(), 2);

        stream.end();
        tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
        assert!(stream.is_closed());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_stream_all_messages() {
        let alix = new_test_client().await;
        let bo = new_test_client().await;
        let caro = new_test_client().await;

        let alix_group = alix
            .conversations()
            .create_group(vec![caro.account_address.clone()], None)
            .await
            .unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let stream_callback = RustStreamCallback::new();

        let stream = caro
            .conversations()
            .stream_all_messages(Box::new(stream_callback.clone()))
            .await
            .unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        alix_group.send("first".as_bytes().to_vec()).await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        let bo_group = bo
            .conversations()
            .create_group(vec![caro.account_address.clone()], None)
            .await
            .unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        bo_group.send("second".as_bytes().to_vec()).await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        alix_group.send("third".as_bytes().to_vec()).await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        bo_group.send("fourth".as_bytes().to_vec()).await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

        assert_eq!(stream_callback.message_count(), 4);
        stream.end();
        tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
        assert!(stream.is_closed());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_message_streaming() {
        let amal = new_test_client().await;
        let bola = new_test_client().await;

        let group = amal
            .conversations()
            .create_group(vec![bola.account_address.clone()], None)
            .await
            .unwrap();

        let stream_callback = RustStreamCallback::new();
        let stream_closer = group
            .stream(Box::new(stream_callback.clone()))
            .await
            .unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        group.send("hello".as_bytes().to_vec()).await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        group.send("goodbye".as_bytes().to_vec()).await.unwrap();
        // Because of the event loop, I need to make the test give control
        // back to the stream before it can process each message. Using sleep to do that.
        // I think this will work fine in practice
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        assert_eq!(stream_callback.message_count(), 2);

        stream_closer.end();
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
            .create_group(vec![bola.account_address.clone()], None)
            .await
            .unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let stream_callback = RustStreamCallback::new();
        let stream_closer = bola
            .conversations()
            .stream_all_messages(Box::new(stream_callback.clone()))
            .await
            .unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        amal_group.send("hello1".as_bytes().to_vec()).await.unwrap();
        amal_group.send("hello2".as_bytes().to_vec()).await.unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        assert_eq!(stream_callback.message_count(), 2);
        assert!(!stream_closer.is_closed());

        amal_group
            .remove_members_by_inbox_id(vec![bola.inbox_id().clone()])
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(2000)).await;
        assert_eq!(stream_callback.message_count(), 3); // Member removal transcript message

        amal_group.send("hello3".as_bytes().to_vec()).await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        assert_eq!(stream_callback.message_count(), 3); // Don't receive messages while removed
        assert!(!stream_closer.is_closed());

        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        amal_group
            .add_members(vec![bola.account_address.clone()])
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        assert_eq!(stream_callback.message_count(), 3); // Don't receive transcript messages while removed

        amal_group.send("hello4".as_bytes().to_vec()).await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        assert_eq!(stream_callback.message_count(), 4); // Receiving messages again
        assert!(!stream_closer.is_closed());

        stream_closer.end();
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        assert!(stream_closer.is_closed());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_group_who_added_me() {
        // Create Clients
        let amal = new_test_client().await;
        let bola = new_test_client().await;

        // Amal creates a group and adds Bola to the group
        amal.conversations()
            .create_group(vec![bola.account_address.clone()], None)
            .await
            .unwrap();

        // Bola syncs groups - this will decrypt the Welcome, identify who added Bola
        // and then store that value on the group and insert into the database
        let bola_conversations = bola.conversations();
        let _ = bola_conversations.sync().await;

        // Bola gets the group id. This will be needed to fetch the group from
        // the database.
        let bola_groups = bola_conversations
            .list(crate::FfiListConversationsOptions {
                created_after_ns: None,
                created_before_ns: None,
                limit: None,
            })
            .await
            .unwrap();

        let bola_group = bola_groups.first().unwrap();

        // Check Bola's group for the added_by_inbox_id of the inviter
        let added_by_inbox_id = bola_group.added_by_inbox_id().unwrap();

        // // Verify the welcome host_credential is equal to Amal's
        assert_eq!(
            amal.inbox_id(),
            added_by_inbox_id,
            "The Inviter and added_by_address do not match!"
        );
    }
}
