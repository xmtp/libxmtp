pub mod inbox_owner;
pub mod logger;
mod v2;

use std::convert::TryInto;

use futures::StreamExt;
use inbox_owner::FfiInboxOwner;
use logger::FfiLogger;
use std::error::Error;
use std::sync::{Arc, Mutex};
use tokio::sync::{oneshot, oneshot::Sender};
use xmtp_proto::api_client::{
    BatchQueryResponse, PagingInfo, PublishResponse, QueryResponse, XmtpApiClient,
};
use xmtp_proto::xmtp::message_api::v1::IndexCursor;

use xmtp_api_grpc::grpc_api_helper::Client as TonicApiClient;
use xmtp_mls::groups::MlsGroup;
use xmtp_mls::storage::group_message::StoredGroupMessage;
use xmtp_mls::types::Address;
use xmtp_mls::{
    builder::ClientBuilder,
    client::Client as MlsClient,
    storage::{EncryptedMessageStore, EncryptionKey, StorageOption},
};
use xmtp_proto::xmtp::message_api::v1::{
    cursor::Cursor as InnerCursor, BatchQueryRequest, Cursor, Envelope, PublishRequest,
    QueryRequest, SortDirection,
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

#[uniffi::export(async_runtime = "tokio")]
impl FfiXmtpClient {
    pub async fn register_identity(&self) -> Result<(), GenericError> {
        self.inner_client.register_identity().await?;

        Ok(())
    }
}

#[derive(uniffi::Enum)]
pub enum FfiSortDirection {
    Unspecified = 0,
    Ascending = 1,
    Descending = 2,
}

impl FfiSortDirection {
    pub fn from_i32(val: i32) -> Self {
        match val {
            0 => FfiSortDirection::Unspecified,
            1 => FfiSortDirection::Ascending,
            2 => FfiSortDirection::Descending,
            _ => panic!("Invalid sort direction"),
        }
    }
}

#[derive(uniffi::Record)]
pub struct FfiPagingInfo {
    limit: u32,
    cursor: Option<FfiCursor>,
    direction: FfiSortDirection,
}

#[derive(uniffi::Record)]
pub struct FfiCursor {
    pub digest: Vec<u8>,
    pub sender_time_ns: u64,
}

#[derive(uniffi::Record)]
pub struct FfiV2QueryRequest {
    content_topics: Vec<String>,
    start_time_ns: u64,
    end_time_ns: u64,
    paging_info: Option<FfiPagingInfo>,
}

#[derive(uniffi::Record)]
pub struct FfiEnvelope {
    content_topic: String,
    timestamp_ns: u64,
    message: Vec<u8>,
}

#[derive(uniffi::Record)]
pub struct FfiPublishRequest {
    envelopes: Vec<FfiEnvelope>,
    // ... the rest of the fields go here
}

impl From<FfiEnvelope> for Envelope {
    fn from(env: FfiEnvelope) -> Self {
        Self {
            content_topic: env.content_topic,
            timestamp_ns: env.timestamp_ns,
            message: env.message,
        }
    }
}

impl From<Envelope> for FfiEnvelope {
    fn from(env: Envelope) -> Self {
        Self {
            content_topic: env.content_topic,
            timestamp_ns: env.timestamp_ns,
            message: env.message,
        }
    }
}

impl From<PublishRequest> for FfiPublishRequest {
    fn from(req: PublishRequest) -> Self {
        Self {
            envelopes: req.envelopes.into_iter().map(|env| env.into()).collect(),
        }
    }
}

impl From<FfiPublishRequest> for PublishRequest {
    fn from(req: FfiPublishRequest) -> Self {
        Self {
            envelopes: req.envelopes.into_iter().map(|env| env.into()).collect(),
        }
    }
}

#[derive(uniffi::Record)]
pub struct FfiV2PublishResponse {}

impl From<FfiV2PublishResponse> for xmtp_proto::xmtp::message_api::v1::PublishResponse {
    fn from(_resp: FfiV2PublishResponse) -> Self {
        Self {}
    }
}

impl From<PublishResponse> for FfiV2PublishResponse {
    fn from(_resp: PublishResponse) -> Self {
        Self {}
    }
}

impl From<SortDirection> for FfiSortDirection {
    fn from(dir: SortDirection) -> Self {
        match dir {
            SortDirection::Unspecified => FfiSortDirection::Unspecified,
            SortDirection::Ascending => FfiSortDirection::Ascending,
            SortDirection::Descending => FfiSortDirection::Descending,
        }
    }
}

impl From<FfiSortDirection> for SortDirection {
    fn from(dir: FfiSortDirection) -> Self {
        match dir {
            FfiSortDirection::Unspecified => SortDirection::Unspecified,
            FfiSortDirection::Ascending => SortDirection::Ascending,
            FfiSortDirection::Descending => SortDirection::Descending,
        }
    }
}

impl From<FfiCursor> for Cursor {
    fn from(cursor: FfiCursor) -> Self {
        Self {
            cursor: Some(InnerCursor::Index(IndexCursor {
                digest: cursor.digest,
                sender_time_ns: cursor.sender_time_ns,
            })),
        }
    }
}

fn proto_cursor_to_ffi(cursor: Option<Cursor>) -> Option<FfiCursor> {
    match cursor {
        Some(proto_cursor) => match proto_cursor.cursor {
            Some(InnerCursor::Index(index_cursor)) => Some(FfiCursor {
                digest: index_cursor.digest,
                sender_time_ns: index_cursor.sender_time_ns,
            }),
            _ => None,
        },
        _ => None,
    }
}

impl From<FfiV2QueryRequest> for QueryRequest {
    fn from(req: FfiV2QueryRequest) -> Self {
        Self {
            content_topics: req.content_topics,
            start_time_ns: req.start_time_ns,
            end_time_ns: req.end_time_ns,
            paging_info: req.paging_info.map(|paging_info| {
                return PagingInfo {
                    limit: paging_info.limit,
                    direction: paging_info.direction as i32,
                    cursor: paging_info.cursor.map(|c| c.into()), // TODO: fix me
                };
            }),
        }
    }
}

impl From<QueryRequest> for FfiV2QueryRequest {
    fn from(req: QueryRequest) -> Self {
        Self {
            content_topics: req.content_topics,
            start_time_ns: req.start_time_ns,
            end_time_ns: req.end_time_ns,
            paging_info: req.paging_info.map(|paging_info| {
                return FfiPagingInfo {
                    limit: paging_info.limit,
                    direction: FfiSortDirection::from_i32(paging_info.direction),
                    cursor: proto_cursor_to_ffi(paging_info.cursor),
                };
            }),
        }
    }
}

#[derive(uniffi::Record)]
pub struct FfiV2QueryResponse {
    envelopes: Vec<FfiEnvelope>,
    paging_info: Option<FfiPagingInfo>,
}

impl From<QueryResponse> for FfiV2QueryResponse {
    fn from(resp: QueryResponse) -> Self {
        Self {
            envelopes: resp.envelopes.into_iter().map(|env| env.into()).collect(),
            paging_info: resp.paging_info.map(|paging_info| {
                return FfiPagingInfo {
                    limit: paging_info.limit,
                    direction: FfiSortDirection::from_i32(paging_info.direction),
                    cursor: None,
                };
            }),
        }
    }
}

impl From<FfiV2QueryResponse> for QueryResponse {
    fn from(resp: FfiV2QueryResponse) -> Self {
        Self {
            envelopes: resp.envelopes.into_iter().map(|env| env.into()).collect(),
            paging_info: resp.paging_info.map(|paging_info| {
                return PagingInfo {
                    limit: paging_info.limit,
                    direction: paging_info.direction as i32,
                    cursor: None, // TODO: fix me
                };
            }),
        }
    }
}

#[derive(uniffi::Record)]
pub struct FfiV2BatchQueryRequest {
    requests: Vec<FfiV2QueryRequest>,
}

#[derive(uniffi::Record)]
pub struct FfiV2BatchQueryResponse {
    responses: Vec<FfiV2QueryResponse>,
}

impl From<BatchQueryRequest> for FfiV2BatchQueryRequest {
    fn from(req: BatchQueryRequest) -> Self {
        Self {
            requests: req.requests.into_iter().map(|req| req.into()).collect(),
        }
    }
}

impl From<FfiV2BatchQueryRequest> for BatchQueryRequest {
    fn from(req: FfiV2BatchQueryRequest) -> Self {
        Self {
            requests: req.requests.into_iter().map(|req| req.into()).collect(),
        }
    }
}

impl From<BatchQueryResponse> for FfiV2BatchQueryResponse {
    fn from(resp: BatchQueryResponse) -> Self {
        Self {
            responses: resp.responses.into_iter().map(|resp| resp.into()).collect(),
        }
    }
}

impl From<FfiV2BatchQueryResponse> for BatchQueryResponse {
    fn from(resp: FfiV2BatchQueryResponse) -> Self {
        Self {
            responses: resp.responses.into_iter().map(|resp| resp.into()).collect(),
        }
    }
}

#[derive(uniffi::Object)]
pub struct FfiV2Client {
    auth_token: Arc<String>,
    inner_client: Arc<xmtp_api_grpc::grpc_api_helper::Client>,
}

#[uniffi::export(async_runtime = "tokio")]
impl FfiV2Client {
    pub async fn batch_query(
        &self,
        req: FfiV2BatchQueryRequest,
    ) -> Result<FfiV2BatchQueryResponse, GenericError> {
        let actual_req: BatchQueryRequest = req.into();
        let result = self.inner_client.batch_query(actual_req).await?;
        Ok(result.into())
    }

    pub fn set_app_version(&self, _version: String) {
        log::info!("Needs implementation")
    }

    pub async fn publish(
        &self,
        request: FfiPublishRequest,
    ) -> Result<FfiV2PublishResponse, GenericError> {
        let actual_publish_request: PublishRequest = request.into();
        let result = self
            .inner_client
            .publish(self.auth_token.to_string(), actual_publish_request)
            .await?;

        Ok(result.into())
    }

    pub async fn query(
        &self,
        request: FfiV2QueryRequest,
    ) -> Result<FfiV2QueryResponse, GenericError> {
        let result = self.inner_client.query(request.into()).await?;
        Ok(result.into())
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
        log::info!("creating group with account address: {}", _account_address);

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
    ) -> Result<Arc<FfiMessageStreamCloser>, GenericError> {
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
            log::debug!("closing stream");
        });

        Ok(Arc::new(FfiMessageStreamCloser {
            close_fn: Arc::new(Mutex::new(Some(close_sender))),
        }))
    }
}

#[uniffi::export]
impl FfiGroup {
    pub fn id(&self) -> Vec<u8> {
        self.group_id.clone()
    }
}

// #[derive(uniffi::Record)]
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
pub struct FfiMessageStreamCloser {
    close_fn: Arc<Mutex<Option<Sender<()>>>>,
}

#[uniffi::export]
impl FfiMessageStreamCloser {
    pub fn close(&self) {
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

pub trait FfiMessageCallback: Send + Sync {
    fn on_message(&self, message: FfiMessage);
}

#[cfg(test)]
mod tests {
    use std::{
        env,
        sync::{Arc, Mutex},
    };

    use crate::{
        create_client, inbox_owner::SigningError, logger::FfiLogger, FfiInboxOwner, FfiMessage,
        FfiMessageCallback, FfiXmtpClient,
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

    #[derive(Clone)]
    struct RustMessageCallback {
        num_messages: Arc<Mutex<u32>>,
    }

    impl RustMessageCallback {
        pub fn new() -> Self {
            Self {
                num_messages: Arc::new(Mutex::new(0)),
            }
        }

        pub fn message_count(&self) -> u32 {
            *self.num_messages.lock().unwrap()
        }
    }

    impl FfiMessageCallback for RustMessageCallback {
        fn on_message(&self, _: FfiMessage) {
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

    #[tokio::test(flavor = "multi_thread", worker_threads = 10)]
    async fn test_streaming() {
        let amal = new_test_client().await;
        let bola = new_test_client().await;

        let group = amal
            .conversations()
            .create_group(bola.account_address())
            .await
            .unwrap();

        let message_callback = RustMessageCallback::new();
        let stream_closer = group
            .stream(Box::new(message_callback.clone()))
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
        assert_eq!(message_callback.message_count(), 2);

        stream_closer.close();
        // Make sure nothing panics calling `close` twice
        stream_closer.close();
    }
}
