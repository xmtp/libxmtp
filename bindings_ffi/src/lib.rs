pub mod inbox_owner;
pub mod logger;
mod v2;

use std::convert::TryInto;

use inbox_owner::FfiInboxOwner;
use logger::FfiLogger;
use std::error::Error;
use std::sync::Arc;

use xmtp_api_grpc::grpc_api_helper::Client as TonicApiClient;
use xmtp_mls::groups::MlsGroup;
use xmtp_mls::storage::group_message::StoredGroupMessage;
use xmtp_mls::types::Address;
use xmtp_mls::{
    builder::ClientBuilder,
    client::Client as MlsClient,
    storage::{EncryptedMessageStore, EncryptionKey, StorageOption},
};

use crate::inbox_owner::RustInboxOwner;
pub use crate::inbox_owner::SigningError;
use crate::logger::init_logger;

pub type RustXmtpClient = MlsClient<TonicApiClient>;
uniffi::include_scaffolding!("xmtpv3");

#[derive(uniffi::Error, Debug)]
#[uniffi(handle_unknown_callback_error)]
pub enum GenericError {
    Generic { err: String },
}

impl<T: Error> From<T> for GenericError {
    fn from(error: T) -> Self {
        Self::Generic {
            err: stringify_error_chain(&error),
        }
    }
}

// TODO Use non-string errors across Uniffi interface
fn stringify_error_chain<T: Error>(error: &T) -> String {
    let mut result = format!("Error: {}\n", error);

    let mut source = error.source();
    while let Some(src) = source {
        result += &format!("Caused by: {}\n", src);
        source = src.source();
    }

    result
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
            let key: EncryptionKey = key.try_into().map_err(|_err| GenericError::Generic {
                err: "Malformed 32 byte encryption key".to_string(),
            })?;
            EncryptedMessageStore::new(storage_option, key)?
        }
        None => EncryptedMessageStore::new_unencrypted(storage_option)?,
    };

    log::info!("Creating XMTP client");
    let xmtp_client: RustXmtpClient = ClientBuilder::new(inbox_owner.into())
        .api_client(api_client)
        .store(store)
        .build()?;

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

#[uniffi::export]
impl FfiXmtpClient {
    pub fn account_address(&self) -> Address {
        self.inner_client.account_address()
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
    pub async fn create_group(
        &self,
        _account_address: String,
    ) -> Result<Arc<FfiGroup>, GenericError> {
        let convo = self.inner_client.create_group()?;

        let out = Arc::new(FfiGroup {
            inner_client: self.inner_client.clone(),
            group_id: convo.group_id,
            created_at_ns: convo.created_at_ns,
        });

        Ok(out)
    }

    pub async fn list(&self) -> Result<Vec<Arc<FfiGroup>>, GenericError> {
        let inner = self.inner_client.as_ref();
        inner.sync_welcomes().await?;

        let convo_list: Vec<Arc<FfiGroup>> = inner
            .find_groups(None, None, None, None)?
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
}

#[derive(uniffi::Object)]
pub struct FfiGroup {
    inner_client: Arc<RustXmtpClient>,
    group_id: Vec<u8>,
    created_at_ns: i64,
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

    pub async fn find_messages(
        &self,
        opts: FfiListMessagesOptions,
    ) -> Result<Vec<FfiMessage>, GenericError> {
        let group = MlsGroup::new(
            self.inner_client.as_ref(),
            self.group_id.clone(),
            self.created_at_ns,
        );
        group.sync().await?;

        let messages: Vec<FfiMessage> = group
            .find_messages(None, opts.sent_before_ns, opts.sent_after_ns, opts.limit)?
            .into_iter()
            .map(|msg| msg.into())
            .collect();

        Ok(messages)
    }

    pub async fn add_members(&self, account_addresses: Vec<String>) -> Result<(), GenericError> {
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
    // TODO pub kind: GroupMessageKind,
}

impl From<StoredGroupMessage> for FfiMessage {
    fn from(msg: StoredGroupMessage) -> Self {
        Self {
            id: msg.id,
            sent_at_ns: msg.sent_at_ns,
            convo_id: msg.group_id,
            addr_from: msg.sender_wallet_address,
            content: msg.decrypted_message_bytes,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{env, sync::Arc};

    use crate::{
        create_client, inbox_owner::SigningError, logger::FfiLogger, FfiInboxOwner, FfiXmtpClient,
    };
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
        fn log(&self, _level: u32, _level_label: String, _message: String) {}
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

        create_client(
            Box::new(MockLogger {}),
            Box::new(ffi_inbox_owner),
            xmtp_api_grpc::LOCALHOST_ADDRESS.to_string(),
            false,
            Some(tmp_path()),
            None,
        )
        .await
        .unwrap()
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
            Box::new(ffi_inbox_owner.clone()),
            xmtp_api_grpc::LOCALHOST_ADDRESS.to_string(),
            false,
            Some(path.clone()),
            None,
        )
        .await
        .unwrap();

        let installation_pub_key = client_a.inner_client.installation_public_key();
        drop(client_a);

        let client_b = create_client(
            Box::new(MockLogger {}),
            Box::new(ffi_inbox_owner),
            xmtp_api_grpc::LOCALHOST_ADDRESS.to_string(),
            false,
            Some(path),
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
            Box::new(ffi_inbox_owner.clone()),
            xmtp_api_grpc::LOCALHOST_ADDRESS.to_string(),
            false,
            Some(path.clone()),
            Some(key),
        )
        .await
        .unwrap();

        drop(client_a);

        let mut other_key = static_enc_key();
        other_key[31] = 1;

        let result_errored = create_client(
            Box::new(MockLogger {}),
            Box::new(ffi_inbox_owner),
            xmtp_api_grpc::LOCALHOST_ADDRESS.to_string(),
            false,
            Some(path),
            Some(other_key.to_vec()),
        )
        .await
        .is_err();

        assert!(result_errored, "did not error on wrong encryption key")
    }

    // #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    // async fn test_conversation_list() {
    //     let client_a = new_test_client().await;
    //     let client_b = new_test_client().await;

    //     // Create a conversation between the two clients
    //     let conversation = client_a
    //         .conversations()
    //         .new_conversation(client_b.account_address())
    //         .await
    //         .unwrap();
    //     conversation.send(vec![1, 2, 3]).await.unwrap();
    //     let convos = client_b.conversations().list().await.unwrap();
    //     assert_eq!(convos.len(), 1);
    //     assert_eq!(
    //         convos.first().unwrap().peer_address,
    //         client_a.account_address()
    //     );
    // }

    // #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    // async fn test_send_and_list() {
    //     let alice = new_test_client().await;
    //     let bob = new_test_client().await;

    //     let alice_to_bob = alice
    //         .conversations()
    //         .new_conversation(bob.wallet_address())
    //         .await
    //         .unwrap();

    //     alice_to_bob.send(vec![1, 2, 3]).await.unwrap();
    //     let messages = alice_to_bob
    //         .list_messages(FfiListMessagesOptions {
    //             start_time_ns: None,
    //             end_time_ns: None,
    //             limit: None,
    //         })
    //         .await
    //         .unwrap();
    //     assert_eq!(messages.len(), 1);
    //     assert_eq!(messages[0].content, vec![1, 2, 3]);
    // }
}
