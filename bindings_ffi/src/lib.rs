pub mod inbox_owner;
pub mod logger;

use std::convert::TryInto;

use inbox_owner::FfiInboxOwner;
use log::info;
use logger::FfiLogger;
use std::error::Error;
use std::sync::Arc;
use xmtp::conversation::{ListMessagesOptions, SecretConversation};
use xmtp::conversations::Conversations;
use xmtp::storage::{EncryptedMessageStore, EncryptionKey, StorageOption, StoredMessage};
use xmtp::types::Address;
use xmtp_networking::grpc_api_helper::Client as TonicApiClient;

use crate::inbox_owner::RustInboxOwner;
pub use crate::inbox_owner::SigningError;
use crate::logger::init_logger;

pub type RustXmtpClient = xmtp::Client<TonicApiClient>;
uniffi::include_scaffolding!("xmtpv3");

#[derive(uniffi::Error, Debug)]
#[uniffi(handle_unknown_callback_error)]
pub enum GenericError {
    Generic { err: String },
}

impl From<String> for GenericError {
    fn from(err: String) -> Self {
        Self::Generic { err }
    }
}

impl From<uniffi::UnexpectedUniFFICallbackError> for GenericError {
    fn from(e: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::Generic { err: e.reason }
    }
}

// TODO Use non-string errors across Uniffi interface
fn stringify_error_chain(error: &(dyn Error + 'static)) -> String {
    let mut result = format!("Error: {}\n", error);

    let mut source = error.source();
    while let Some(src) = source {
        result += &format!("Caused by: {}\n", src);
        source = src.source();
    }

    result
}

fn static_enc_key() -> EncryptionKey {
    [2u8; 32]
}

#[uniffi::export(async_runtime = "tokio")]
pub async fn create_client(
    logger: Box<dyn FfiLogger>,
    ffi_inbox_owner: Box<dyn FfiInboxOwner>,
    host: String,
    is_secure: bool,
    db: Option<String>,
    encryption_key: Option<Vec<u8>>,
) -> Result<Arc<FfiXmtpClient>, GenericError> {
    init_logger(logger);

    let inbox_owner = RustInboxOwner::new(ffi_inbox_owner);
    let api_client = TonicApiClient::create(host.clone(), is_secure)
        .await
        .map_err(|e| stringify_error_chain(&e))?;

    let key: EncryptionKey = match encryption_key {
        Some(key) => key.try_into().unwrap(),
        None => static_enc_key(),
    };

    let store = match db {
        Some(path) => {
            info!("Using persistent storage: {} ", path);
            EncryptedMessageStore::new(StorageOption::Persistent(path), key)
        }

        None => {
            info!("Using ephemeral store");
            EncryptedMessageStore::new(StorageOption::Ephemeral, key)
        }
    }
    .map_err(|e| stringify_error_chain(&e))?;

    let mut xmtp_client: RustXmtpClient = xmtp::ClientBuilder::new(inbox_owner.into())
        .api_client(api_client)
        .store(store)
        .build()
        .map_err(|e| stringify_error_chain(&e))?;
    xmtp_client
        .init()
        .await
        .map_err(|e| stringify_error_chain(&e))?;

    info!(
        "Created XMTP client for address: {}",
        xmtp_client.wallet_address()
    );
    Ok(Arc::new(FfiXmtpClient {
        inner_client: Arc::new(xmtp_client),
    }))
}

#[derive(uniffi::Object)]
pub struct FfiXmtpClient {
    inner_client: Arc<RustXmtpClient>,
}

#[uniffi::export]
impl FfiXmtpClient {
    pub fn wallet_address(&self) -> Address {
        self.inner_client.wallet_address()
    }

    pub fn conversations(&self) -> Arc<FfiConversations> {
        Arc::new(FfiConversations {
            inner_client: self.inner_client.clone(),
        })
    }
}

#[derive(uniffi::Object)]
pub struct FfiConversations {
    inner_client: Arc<RustXmtpClient>,
}

#[uniffi::export(async_runtime = "tokio")]
impl FfiConversations {
    pub async fn new_conversation(
        &self,
        wallet_address: String,
    ) -> Result<Arc<FfiConversation>, GenericError> {
        let convo = SecretConversation::new(self.inner_client.as_ref(), wallet_address)
            .map_err(|e| e.to_string())?;
        // TODO: This should happen as part of `new_secret_conversation` and should only send to new participants
        convo.initialize().await.map_err(|e| e.to_string())?;

        let out = Arc::new(FfiConversation {
            inner_client: self.inner_client.clone(),
            id: convo.convo_id(),
            peer_address: convo.peer_address(),
        });

        Ok(out)
    }

    pub async fn list(&self) -> Result<Vec<Arc<FfiConversation>>, GenericError> {
        let inner = self.inner_client.as_ref();
        let convo_list = Conversations::list(inner, true)
            .await
            .map_err(|e| e.to_string())?;
        let out: Vec<Arc<FfiConversation>> = convo_list
            .into_iter()
            .map(|convo| {
                Arc::new(FfiConversation {
                    inner_client: self.inner_client.clone(),
                    id: convo.convo_id(),
                    peer_address: convo.peer_address(),
                })
            })
            .collect();

        Ok(out)
    }
}

#[derive(uniffi::Object)]
pub struct FfiConversation {
    inner_client: Arc<RustXmtpClient>,
    id: String,
    peer_address: String,
}

#[derive(uniffi::Record)]
pub struct FfiListMessagesOptions {
    pub start_time_ns: Option<i64>,
    pub end_time_ns: Option<i64>,
    pub limit: Option<i64>,
}

impl FfiListMessagesOptions {
    fn to_options(&self) -> ListMessagesOptions {
        ListMessagesOptions {
            start_time_ns: self.start_time_ns,
            end_time_ns: self.end_time_ns,
            limit: self.limit,
        }
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl FfiConversation {
    pub async fn send(&self, content_bytes: Vec<u8>) -> Result<(), GenericError> {
        let conversation = xmtp::conversation::SecretConversation::new(
            self.inner_client.as_ref(),
            self.peer_address.clone(),
        )
        .map_err(|e| e.to_string())?;
        conversation
            .send(content_bytes)
            .await
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    pub async fn list_messages(
        &self,
        opts: FfiListMessagesOptions,
    ) -> Result<Vec<FfiMessage>, GenericError> {
        Conversations::receive(self.inner_client.as_ref()).map_err(|e| e.to_string())?;

        let conversation = xmtp::conversation::SecretConversation::new(
            self.inner_client.as_ref(),
            self.peer_address.clone(),
        )
        .map_err(|e| e.to_string())?;
        let options: ListMessagesOptions = opts.to_options();

        let messages: Vec<FfiMessage> = conversation
            .list_messages(&options)
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|msg| msg.into())
            .collect();

        Ok(messages)
    }
}

#[uniffi::export]
impl FfiConversation {
    pub fn id(&self) -> String {
        self.id.clone()
    }
}

#[derive(uniffi::Record)]
pub struct FfiMessage {
    pub id: String,
    pub sent_at_ns: i64,
    pub convo_id: String,
    pub addr_from: String,
    pub content: Vec<u8>,
}

impl From<StoredMessage> for FfiMessage {
    fn from(msg: StoredMessage) -> Self {
        Self {
            id: msg.id.to_string(),
            sent_at_ns: msg.sent_at_ns,
            convo_id: msg.convo_id,
            addr_from: msg.addr_from,
            content: msg.content,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::{
        create_client, inbox_owner::SigningError, logger::FfiLogger, static_enc_key, FfiInboxOwner,
        FfiListMessagesOptions, FfiXmtpClient,
    };
    use tempfile::TempPath;
    use xmtp::InboxOwner;
    use xmtp_cryptography::{
        signature::{RecoverableSignature, SigningKey},
        utils::rng,
    };

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
        fn log(&self, _level: u32, _level_label: String, _message: String) {}
    }

    async fn new_test_client() -> Arc<FfiXmtpClient> {
        let ffi_inbox_owner = LocalWalletInboxOwner::new();

        create_client(
            Box::new(MockLogger {}),
            Box::new(ffi_inbox_owner),
            xmtp_networking::LOCALHOST_ADDRESS.to_string(),
            false,
            None,
            None,
        )
        .await
        .unwrap()
    }

    // Try a query on a test topic, and make sure we get a response
    #[tokio::test]
    async fn test_client_creation() {
        let client = new_test_client().await;
        assert!(!client.wallet_address().is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_create_client_with_storage() {
        let ffi_inbox_owner = LocalWalletInboxOwner::new();

        let dbfilename = xmtp_cryptography::utils::generate_local_wallet().get_address();
        let path = TempPath::from_path(format!("./test-{}.db", dbfilename));

        let client_a = create_client(
            Box::new(MockLogger {}),
            Box::new(ffi_inbox_owner.clone()),
            xmtp_networking::LOCALHOST_ADDRESS.to_string(),
            false,
            Some(path.to_string_lossy().to_string()),
            None,
        )
        .await
        .unwrap();

        let installation_id = client_a.inner_client.installation_id();
        drop(client_a);

        let client_b = create_client(
            Box::new(MockLogger {}),
            Box::new(ffi_inbox_owner),
            xmtp_networking::LOCALHOST_ADDRESS.to_string(),
            false,
            Some(path.to_string_lossy().to_string()),
            None,
        )
        .await
        .unwrap();

        let other_installation_id = client_b.inner_client.installation_id();
        drop(client_b);

        assert!(
            installation_id == other_installation_id,
            "did not use same installation ID"
        )
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_create_client_with_key() {
        let ffi_inbox_owner = LocalWalletInboxOwner::new();

        let dbfilename = xmtp_cryptography::utils::generate_local_wallet().get_address();
        let path = TempPath::from_path(format!("./test-{}.db", dbfilename));

        let key = static_enc_key().to_vec();

        let client_a = create_client(
            Box::new(MockLogger {}),
            Box::new(ffi_inbox_owner.clone()),
            xmtp_networking::LOCALHOST_ADDRESS.to_string(),
            false,
            Some(path.to_string_lossy().to_string()),
            Some(key),
        )
        .await
        .unwrap();

        drop(client_a);

        let mut other_key = static_enc_key();
        other_key[0] = 1;

        let result_errored = create_client(
            Box::new(MockLogger {}),
            Box::new(ffi_inbox_owner),
            xmtp_networking::LOCALHOST_ADDRESS.to_string(),
            false,
            Some(path.to_string_lossy().to_string()),
            Some(other_key.to_vec()),
        )
        .await
        .is_err();

        assert!(result_errored, "did not error on wrong encryption key")
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_conversation_list() {
        let client_a = new_test_client().await;
        let client_b = new_test_client().await;

        // Create a conversation between the two clients
        client_a
            .conversations()
            .new_conversation(client_b.wallet_address())
            .await
            .unwrap();

        let convos = client_b.conversations().list().await.unwrap();
        assert_eq!(convos.len(), 1);
        assert_eq!(
            convos.first().unwrap().peer_address,
            client_a.wallet_address()
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_send_and_list() {
        let alice = new_test_client().await;
        let bob = new_test_client().await;

        let alice_to_bob = alice
            .conversations()
            .new_conversation(bob.wallet_address())
            .await
            .unwrap();

        alice_to_bob.send(vec![1, 2, 3]).await.unwrap();
        let messages = alice_to_bob
            .list_messages(FfiListMessagesOptions {
                start_time_ns: None,
                end_time_ns: None,
                limit: None,
            })
            .await
            .unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, vec![1, 2, 3]);
    }
}
