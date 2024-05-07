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
use xmtp_mls::groups::group_metadata::ConversationType;
use xmtp_mls::groups::group_metadata::GroupMetadata;
use xmtp_mls::groups::PreconfiguredPolicies;
use xmtp_mls::identity::v3::{IdentityStrategy, LegacyIdentity};
use xmtp_mls::{
    builder::ClientBuilder,
    client::Client as MlsClient,
    groups::MlsGroup,
    storage::{
        group_message::DeliveryStatus, group_message::GroupMessageKind,
        group_message::StoredGroupMessage, EncryptedMessageStore, EncryptionKey, StorageOption,
    },
    types::Address,
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
    init_logger(logger);

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
    let legacy_identity = match legacy_identity_source {
        LegacyIdentitySource::None => LegacyIdentity::None,
        LegacyIdentitySource::Static => LegacyIdentity::Static(legacy_key_result?),
        LegacyIdentitySource::Network => LegacyIdentity::Network(legacy_key_result?),
        LegacyIdentitySource::KeyGenerator => LegacyIdentity::KeyGenerator(legacy_key_result?),
    };
    let identity_strategy = IdentityStrategy::CreateIfNotFound(account_address, legacy_identity);
    let xmtp_client: RustXmtpClient = ClientBuilder::new(identity_strategy)
        .api_client(api_client)
        .store(store)
        .build()
        .await?;

    log::info!(
        "Created XMTP client for address: {}",
        xmtp_client.account_address()
    );
    Ok(Arc::new(FfiXmtpClient {
        inner_client: Arc::new(xmtp_client),
    }))
}

#[derive(uniffi::Object)]
pub struct FfiXmtpClient {
    inner_client: Arc<RustXmtpClient>,
}

#[uniffi::export(async_runtime = "tokio")]
impl FfiXmtpClient {
    pub fn account_address(&self) -> Address {
        self.inner_client.account_address()
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
}

#[uniffi::export(async_runtime = "tokio")]
impl FfiXmtpClient {
    pub fn text_to_sign(&self) -> Option<String> {
        self.inner_client.text_to_sign()
    }

    pub async fn register_identity(
        &self,
        recoverable_wallet_signature: Option<Vec<u8>>,
    ) -> Result<(), GenericError> {
        self.inner_client
            .register_identity(recoverable_wallet_signature)
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
    EveryoneIsAdmin,
    GroupCreatorIsAdmin,
}

impl From<PreconfiguredPolicies> for GroupPermissions {
    fn from(policy: PreconfiguredPolicies) -> Self {
        match policy {
            PreconfiguredPolicies::EveryoneIsAdmin => GroupPermissions::EveryoneIsAdmin,
            PreconfiguredPolicies::GroupCreatorIsAdmin => GroupPermissions::GroupCreatorIsAdmin,
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
            Some(GroupPermissions::EveryoneIsAdmin) => {
                Some(xmtp_mls::groups::PreconfiguredPolicies::EveryoneIsAdmin)
            }
            Some(GroupPermissions::GroupCreatorIsAdmin) => {
                Some(xmtp_mls::groups::PreconfiguredPolicies::GroupCreatorIsAdmin)
            }
            _ => None,
        };

        let convo = self.inner_client.create_group(group_permissions)?;
        if !account_addresses.is_empty() {
            convo.add_members(account_addresses).await?;
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
    pub account_address: String,
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
            self.inner_client.as_ref(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        let message_id = group.send_message(content_bytes.as_slice()).await?;

        Ok(message_id)
    }

    pub async fn sync(&self) -> Result<(), GenericError> {
        let group = MlsGroup::new(
            self.inner_client.as_ref(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        group.sync().await?;

        Ok(())
    }

    pub fn find_messages(
        &self,
        opts: FfiListMessagesOptions,
    ) -> Result<Vec<FfiMessage>, GenericError> {
        let group = MlsGroup::new(
            self.inner_client.as_ref(),
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
            self.inner_client.as_ref(),
            self.group_id.clone(),
            self.created_at_ns,
        );
        let message = group.process_streamed_group_message(envelope_bytes).await?;
        let ffi_message = message.into();

        Ok(ffi_message)
    }

    pub fn list_members(&self) -> Result<Vec<FfiGroupMember>, GenericError> {
        let group = MlsGroup::new(
            self.inner_client.as_ref(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        let members: Vec<FfiGroupMember> = group
            .members()?
            .into_iter()
            .map(|member| FfiGroupMember {
                account_address: member.account_address,
                installation_ids: member.installation_ids,
            })
            .collect();

        Ok(members)
    }

    pub async fn add_members(&self, account_addresses: Vec<String>) -> Result<(), GenericError> {
        log::info!("adding members: {}", account_addresses.join(","));

        let group = MlsGroup::new(
            self.inner_client.as_ref(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        group.add_members(account_addresses).await?;

        Ok(())
    }

    pub async fn remove_members(&self, account_addresses: Vec<String>) -> Result<(), GenericError> {
        let group = MlsGroup::new(
            self.inner_client.as_ref(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        group.remove_members(account_addresses).await?;

        Ok(())
    }

    pub async fn update_group_name(&self, group_name: String) -> Result<(), GenericError> {
        let group = MlsGroup::new(
            self.inner_client.as_ref(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        group.update_group_name(group_name).await?;

        Ok(())
    }

    pub fn group_name(&self) -> Result<String, GenericError> {
        let group = MlsGroup::new(
            self.inner_client.as_ref(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        let group_name = group.group_name()?;

        Ok(group_name)
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
        )
        .await?;

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
            self.inner_client.as_ref(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        Ok(group.is_active()?)
    }

    pub fn added_by_address(&self) -> Result<String, GenericError> {
        let group = MlsGroup::new(
            self.inner_client.as_ref(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        Ok(group.added_by_address()?)
    }

    pub fn group_metadata(&self) -> Result<Arc<FfiGroupMetadata>, GenericError> {
        let group = MlsGroup::new(
            self.inner_client.as_ref(),
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
    pub addr_from: String,
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
            addr_from: msg.sender_account_address,
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
    pub fn creator_account_address(&self) -> String {
        self.inner.creator_account_address.clone()
    }

    pub fn conversation_type(&self) -> String {
        match self.inner.conversation_type {
            ConversationType::Group => "group".to_string(),
            ConversationType::Dm => "dm".to_string(),
            ConversationType::Sync => "sync".to_string(),
        }
    }

    pub fn policy_type(&self) -> Result<GroupPermissions, GenericError> {
        Ok(self.inner.preconfigured_policy()?.into())
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        inbox_owner::SigningError, logger::FfiLogger, FfiConversationCallback, FfiInboxOwner,
        LegacyIdentitySource,
    };
    use std::{
        env,
        sync::{Arc, Mutex},
    };

    use super::{create_client, FfiMessage, FfiMessageCallback, FfiXmtpClient};
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
        num_messages: Arc<Mutex<u32>>,
    }

    impl RustStreamCallback {
        pub fn new() -> Self {
            Self {
                num_messages: Arc::new(Mutex::new(0)),
            }
        }

        pub fn message_count(&self) -> u32 {
            *self.num_messages.lock().unwrap()
        }
    }

    impl FfiMessageCallback for RustStreamCallback {
        fn on_message(&self, message: FfiMessage) {
            let message = String::from_utf8(message.content).unwrap_or("<not UTF8>".to_string());
            log::info!("Received: {}", message);
            *self.num_messages.lock().unwrap() += 1;
        }
    }

    impl FfiConversationCallback for RustStreamCallback {
        fn on_conversation(&self, _: Arc<super::FfiGroup>) {
            *self.num_messages.lock().unwrap() += 1;
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

        let text_to_sign = client.text_to_sign().unwrap();
        let signature = ffi_inbox_owner.sign(text_to_sign).unwrap();

        client.register_identity(Some(signature)).await.unwrap();
        return client;
    }

    // Try a query on a test topic, and make sure we get a response
    #[tokio::test]
    async fn test_client_creation() {
        let client = new_test_client().await;
        assert!(!client.account_address().is_empty());
    }

    #[tokio::test]
    async fn test_legacy_identity() {
        let legacy_address = "0x419cb1fa5635b0c6df47c9dc5765c8f1f4dff78e";
        let legacy_signed_private_key_proto = vec![
            8, 128, 154, 196, 133, 220, 244, 197, 216, 23, 18, 34, 10, 32, 214, 70, 104, 202, 68,
            204, 25, 202, 197, 141, 239, 159, 145, 249, 55, 242, 147, 126, 3, 124, 159, 207, 96,
            135, 134, 122, 60, 90, 82, 171, 131, 162, 26, 153, 1, 10, 79, 8, 128, 154, 196, 133,
            220, 244, 197, 216, 23, 26, 67, 10, 65, 4, 232, 32, 50, 73, 113, 99, 115, 168, 104,
            229, 206, 24, 217, 132, 223, 217, 91, 63, 137, 136, 50, 89, 82, 186, 179, 150, 7, 127,
            140, 10, 165, 117, 233, 117, 196, 134, 227, 143, 125, 210, 187, 77, 195, 169, 162, 116,
            34, 20, 196, 145, 40, 164, 246, 139, 197, 154, 233, 190, 148, 35, 131, 240, 106, 103,
            18, 70, 18, 68, 10, 64, 90, 24, 36, 99, 130, 246, 134, 57, 60, 34, 142, 165, 221, 123,
            63, 27, 138, 242, 195, 175, 212, 146, 181, 152, 89, 48, 8, 70, 104, 94, 163, 0, 25,
            196, 228, 190, 49, 108, 141, 60, 174, 150, 177, 115, 229, 138, 92, 105, 170, 226, 204,
            249, 206, 12, 37, 145, 3, 35, 226, 15, 49, 20, 102, 60, 16, 1,
        ];

        let client = create_client(
            Box::new(MockLogger {}),
            xmtp_api_grpc::LOCALHOST_ADDRESS.to_string(),
            false,
            Some(tmp_path()),
            None,
            legacy_address.to_string(),
            LegacyIdentitySource::KeyGenerator,
            Some(legacy_signed_private_key_proto),
        )
        .await
        .unwrap();

        assert!(client.text_to_sign().is_none());
        client.register_identity(None).await.unwrap();
        assert_eq!(client.account_address(), legacy_address);
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
        let text_to_sign = client_a.text_to_sign().unwrap();
        let signature = ffi_inbox_owner.sign(text_to_sign).unwrap();
        client_a.register_identity(Some(signature)).await.unwrap();

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
        bola.register_identity(None).await.unwrap();

        let group = amal
            .conversations()
            .create_group(vec![bola.account_address()], None)
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

        let text_to_sign = client.text_to_sign().unwrap();
        let mut signature = inbox_owner.sign(text_to_sign).unwrap();
        signature[0] ^= 1;

        assert!(client.register_identity(Some(signature)).await.is_err());
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
        let text_to_sign = client_bola.text_to_sign().unwrap();
        let signature = bola.sign(text_to_sign).unwrap();
        client_bola
            .register_identity(Some(signature))
            .await
            .unwrap();

        let can_message_result2 = client_amal
            .can_message(vec![bola.get_address()])
            .await
            .unwrap();

        assert!(
            can_message_result2
                .get(&bola.get_address().to_string())
                .map(|&value| value)
                .unwrap_or(false),
            "Expected the can_message result to be true for the address"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
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
            .create_group(vec![bola.account_address()], None)
            .await
            .unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        assert_eq!(stream_callback.message_count(), 1);
        // Create another group and add bola
        amal.conversations()
            .create_group(vec![bola.account_address()], None)
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
            .create_group(vec![caro.account_address()], None)
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
            .create_group(vec![caro.account_address()], None)
            .await
            .unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        bo_group.send("second".as_bytes().to_vec()).await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        alix_group.send("third".as_bytes().to_vec()).await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        bo_group.send("fourth".as_bytes().to_vec()).await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

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
            .create_group(vec![bola.account_address()], None)
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
            "Created addresses {} and {}",
            amal.account_address(),
            bola.account_address()
        );

        let amal_group = amal
            .conversations()
            .create_group(vec![bola.account_address()], None)
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
            .remove_members(vec![bola.account_address()])
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
            .add_members(vec![bola.account_address()])
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
            .create_group(vec![bola.account_address()], None)
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

        // Check Bola's group for the added_by_address of the inviter
        let added_by_address = bola_group.added_by_address().unwrap();

        // // Verify the welcome host_credential is equal to Amal's
        assert_eq!(
            amal.account_address(),
            added_by_address,
            "The Inviter and added_by_address do not match!"
        );
    }
}
