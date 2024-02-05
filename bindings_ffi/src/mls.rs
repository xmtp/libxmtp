use crate::inbox_owner::RustInboxOwner;
pub use crate::inbox_owner::SigningError;
use crate::logger::init_logger;
use crate::logger::FfiLogger;
use crate::GenericError;
use futures::StreamExt;
use std::convert::TryInto;
use std::sync::{Arc, Mutex};
use tokio::sync::{oneshot, oneshot::Sender};
use xmtp_api_grpc::grpc_api_helper::Client as TonicApiClient;
use xmtp_mls::builder::IdentityStrategy;
use xmtp_mls::builder::LegacyIdentity;
use xmtp_mls::{
    builder::ClientBuilder,
    client::Client as MlsClient,
    groups::MlsGroup,
    storage::{
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
    let identity_strategy =
        IdentityStrategy::CreateUnsignedIfNotFound(account_address, legacy_identity);
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
    ) -> Result<Vec<bool>, GenericError> {
        let inner = self.inner_client.as_ref();

        let results: Vec<bool> = inner.can_message(account_addresses).await?;

        Ok(results)
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

#[uniffi::export(async_runtime = "tokio")]
impl FfiConversations {
    pub async fn create_group(
        &self,
        account_addresses: Vec<String>,
    ) -> Result<Arc<FfiGroup>, GenericError> {
        log::info!(
            "creating group with account addresses: {}",
            account_addresses.join(", ")
        );

        let convo = self.inner_client.create_group()?;
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
        let inner_client = Arc::clone(&self.inner_client);
        let (close_sender, close_receiver) = oneshot::channel::<()>();

        tokio::spawn(async move {
            let client = inner_client.as_ref();
            let mut stream = client.stream_conversations().await.unwrap();
            let mut close_receiver = close_receiver;
            loop {
                tokio::select! {
                    item = stream.next() => {
                        match item {
                            Some(convo) => callback.on_conversation(Arc::new(FfiGroup {
                                inner_client: inner_client.clone(),
                                group_id: convo.group_id,
                                created_at_ns: convo.created_at_ns,
                            })),
                            None => break
                        }
                    }
                    _ = &mut close_receiver => {
                        break;
                    }
                }
            }
            log::info!("closing stream");
        });

        Ok(Arc::new(FfiStreamCloser {
            close_fn: Arc::new(Mutex::new(Some(close_sender))),
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
}

#[uniffi::export(async_runtime = "tokio")]
impl FfiGroup {
    pub async fn send(&self, content_bytes: Vec<u8>) -> Result<(), GenericError> {
        let group = MlsGroup::new(
            self.inner_client.as_ref(),
            self.group_id.clone(),
            self.created_at_ns,
        );

        group.send_message(content_bytes.as_slice()).await?;

        Ok(())
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

        let messages: Vec<FfiMessage> = group
            .find_messages(None, opts.sent_before_ns, opts.sent_after_ns, opts.limit)?
            .into_iter()
            .map(|msg| msg.into())
            .collect();

        Ok(messages)
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

    pub async fn stream(
        &self,
        message_callback: Box<dyn FfiMessageCallback>,
    ) -> Result<Arc<FfiStreamCloser>, GenericError> {
        let inner_client = Arc::clone(&self.inner_client);
        let group_id = self.group_id.clone();
        let created_at_ns = self.created_at_ns;
        let (close_sender, close_receiver) = oneshot::channel::<()>();

        tokio::spawn(async move {
            let client = inner_client.as_ref();
            let group = MlsGroup::new(&client, group_id, created_at_ns);
            let mut stream = group.stream().await.unwrap();
            let mut close_receiver = close_receiver;
            loop {
                tokio::select! {
                    item = stream.next() => {
                        match item {
                            Some(message) => message_callback.on_message(message.into()),
                            None => break
                        }
                    }
                    _ = &mut close_receiver => {
                        break;
                    }
                }
            }
            log::info!("closing stream");
        });

        Ok(Arc::new(FfiStreamCloser {
            close_fn: Arc::new(Mutex::new(Some(close_sender))),
        }))
    }

    pub fn created_at_ns(&self) -> i64 {
        self.created_at_ns
    }
}

#[uniffi::export]
impl FfiGroup {
    pub fn id(&self) -> Vec<u8> {
        self.group_id.clone()
    }
}

#[derive(uniffi::Record)]
pub struct FfiMessage {
    pub id: Vec<u8>,
    pub sent_at_ns: i64,
    pub convo_id: Vec<u8>,
    pub addr_from: String,
    pub content: Vec<u8>,
}

impl From<StoredGroupMessage> for FfiMessage {
    fn from(msg: StoredGroupMessage) -> Self {
        Self {
            id: msg.id,
            sent_at_ns: msg.sent_at_ns,
            convo_id: msg.group_id,
            addr_from: msg.sender_account_address,
            content: msg.decrypted_message_bytes,
        }
    }
}

#[derive(uniffi::Object)]
pub struct FfiStreamCloser {
    close_fn: Arc<Mutex<Option<Sender<()>>>>,
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
}

#[uniffi::export(callback_interface)]
pub trait FfiMessageCallback: Send + Sync {
    fn on_message(&self, message: FfiMessage);
}

#[uniffi::export(callback_interface)]
pub trait FfiConversationCallback: Send + Sync {
    fn on_conversation(&self, conversation: Arc<FfiGroup>);
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
        fn on_message(&self, _: FfiMessage) {
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
            .create_group(vec![bola.account_address()])
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
        assert!(
            !client_amal
                .can_message(vec![bola.get_address()])
                .await
                .unwrap()[0]
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
        assert!(
            client_amal
                .can_message(vec![bola.get_address()])
                .await
                .unwrap()[0]
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
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
            .create_group(vec![bola.account_address()])
            .await
            .unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        assert_eq!(stream_callback.message_count(), 1);
        // Create another group and add bola
        amal.conversations()
            .create_group(vec![bola.account_address()])
            .await
            .unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        assert_eq!(stream_callback.message_count(), 2);

        stream.end();
    }

    // Disabling this flakey test until it's reliable
    #[tokio::test(flavor = "multi_thread", worker_threads = 5)]
    async fn test_streaming() {
        let amal = new_test_client().await;
        let bola = new_test_client().await;

        let group = amal
            .conversations()
            .create_group(vec![bola.account_address()])
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
}
