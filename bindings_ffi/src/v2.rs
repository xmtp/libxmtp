use crate::GenericError;
use futures::StreamExt;
use std::sync::Arc;
use xmtp_proto::api_client::{
    BatchQueryResponse, MutableApiSubscription, PagingInfo, QueryResponse, SubscribeRequest,
    XmtpApiClient,
};
use xmtp_proto::xmtp::message_api::v1::IndexCursor;
use xmtp_v2::{hashes, k256_helper};

use tokio::{
    sync::{mpsc, Mutex},
    task::{AbortHandle, JoinHandle},
};
use xmtp_api_grpc::grpc_api_helper::{Client as GrpcClient, GrpcMutableSubscription};
use xmtp_proto::xmtp::message_api::v1::{
    cursor::Cursor as InnerCursor, BatchQueryRequest, Cursor, Envelope, PublishRequest,
    QueryRequest, SortDirection,
};

#[uniffi::export(async_runtime = "tokio")]
pub async fn create_v2_client(
    host: String,
    is_secure: bool,
) -> Result<Arc<FfiV2ApiClient>, GenericError> {
    let client = GrpcClient::create(host, is_secure).await?;

    let client = FfiV2ApiClient {
        inner_client: Arc::new(client),
    };

    Ok(client.into())
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

#[derive(uniffi::Record, Debug)]
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
                PagingInfo {
                    limit: paging_info.limit,
                    direction: paging_info.direction as i32,
                    cursor: paging_info.cursor.map(|c| c.into()), // TODO: fix me
                }
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
            paging_info: req.paging_info.map(|paging_info| FfiPagingInfo {
                limit: paging_info.limit,
                direction: FfiSortDirection::from_i32(paging_info.direction),
                cursor: proto_cursor_to_ffi(paging_info.cursor),
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
            paging_info: resp.paging_info.map(|paging_info| FfiPagingInfo {
                limit: paging_info.limit,
                direction: FfiSortDirection::from_i32(paging_info.direction),
                cursor: proto_cursor_to_ffi(paging_info.cursor),
            }),
        }
    }
}

impl From<FfiV2QueryResponse> for QueryResponse {
    fn from(resp: FfiV2QueryResponse) -> Self {
        Self {
            envelopes: resp.envelopes.into_iter().map(|env| env.into()).collect(),
            paging_info: resp.paging_info.map(|paging_info| PagingInfo {
                limit: paging_info.limit,
                direction: paging_info.direction as i32,
                cursor: paging_info.cursor.map(|c| c.into()),
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

#[derive(uniffi::Record)]
pub struct FfiV2SubscribeRequest {
    pub content_topics: Vec<String>,
}

impl From<FfiV2SubscribeRequest> for SubscribeRequest {
    fn from(req: FfiV2SubscribeRequest) -> Self {
        Self {
            content_topics: req.content_topics,
        }
    }
}

#[uniffi::export(callback_interface)]
pub trait FfiV2SubscriptionCallback: Send + Sync {
    fn on_message(&self, message: FfiEnvelope);
}

/// Subscription to a stream of V2 Messages
#[derive(uniffi::Object)]
pub struct FfiV2Subscription {
    tx: mpsc::Sender<FfiV2SubscribeRequest>,
    abort: AbortHandle,
    // we require Arc<Mutex<>> here because uniffi doesn't like &mut self, or the owned version of self on exported methods 
    #[allow(clippy::type_complexity)]
    handle: Arc<Mutex<Option<JoinHandle<()>>>>,
}

#[uniffi::export(async_runtime = "tokio")]
impl FfiV2Subscription {
    /// End the subscription, waiting for the subscription to close entirely.
    /// # Errors
    /// * Errors if subscription event task encounters join error
    pub async fn end(&self) -> Result<(), GenericError> {
        if self.abort.is_finished() {
            return Ok(());
        }

        let mut handle = self.handle.lock().await;
        let handle = handle.take();
        if let Some(h) = handle {
            h.abort();
            h.await.map_err(|_| GenericError::Generic {
                err: "subscription event loop join error".into(),
            })?;
        }
        Ok(())
    }

    /// Check if the subscription is closed
    pub fn is_closed(&self) -> bool {
        self.abort.is_finished()
    }

    /// Update subscription with new topics
    pub async fn update(&self, req: FfiV2SubscribeRequest) -> Result<(), GenericError> {
        self.tx.send(req).await.map_err(|_| GenericError::Generic {
            err: "stream closed".into(),
        })?;
        Ok(())
    }
}

impl FfiV2Subscription {
    async fn subscribe(
        mut subscription: GrpcMutableSubscription,
        callback: Box<dyn FfiV2SubscriptionCallback>,
    ) -> Self {
        let (tx, mut rx): (_, mpsc::Receiver<FfiV2SubscribeRequest>) = mpsc::channel(10);

        let handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    item = subscription.next() => {
                        match item {
                            Some(Ok(envelope)) => callback.on_message(envelope.into()),
                            Some(Err(e)) => log::error!("Stream error {}", e),
                            None => {
                                log::debug!("stream closed");
                                break;
                            }
                        }
                    },
                    update = rx.recv() => {
                        if let Some(update) = update {
                            let _ = subscription.update(update.into()).await.map_err(|e| log::error!("{}", e)).ok();
                        }
                    },
                }
            }
        });

        Self {
            tx,
            abort: handle.abort_handle(),
            handle: Arc::new(Mutex::new(Some(handle))),
        }
    }
}

#[derive(uniffi::Object)]
pub struct FfiV2ApiClient {
    inner_client: Arc<xmtp_api_grpc::grpc_api_helper::Client>,
}

#[uniffi::export(async_runtime = "tokio")]
impl FfiV2ApiClient {
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
        auth_token: String,
    ) -> Result<(), GenericError> {
        let actual_publish_request: PublishRequest = request.into();
        self.inner_client
            .publish(auth_token, actual_publish_request)
            .await?;

        Ok(())
    }

    pub async fn query(
        &self,
        request: FfiV2QueryRequest,
    ) -> Result<FfiV2QueryResponse, GenericError> {
        let result = self.inner_client.query(request.into()).await?;
        Ok(result.into())
    }

    pub async fn subscribe(
        &self,
        request: FfiV2SubscribeRequest,
        callback: Box<dyn FfiV2SubscriptionCallback>,
    ) -> Result<FfiV2Subscription, GenericError> {
        let subscription = self.inner_client.subscribe2(request.into()).await?;
        Ok(FfiV2Subscription::subscribe(subscription, callback).await)
    }
}

#[uniffi::export]
pub fn recover_address(
    signature_bytes: Vec<u8>,
    predigest_message: String,
) -> Result<String, GenericError> {
    let signature =
        xmtp_cryptography::signature::RecoverableSignature::Eip191Signature(signature_bytes);
    let recovered = signature.recover_address(&predigest_message)?;

    Ok(recovered)
}

#[uniffi::export]
pub fn sha256(input: Vec<u8>) -> Vec<u8> {
    hashes::sha256(input.as_slice()).to_vec()
}

#[uniffi::export]
pub fn keccak256(input: Vec<u8>) -> Vec<u8> {
    hashes::keccak256(input.as_slice()).to_vec()
}

#[uniffi::export]
pub fn public_key_from_private_key_k256(
    private_key_bytes: Vec<u8>,
) -> Result<Vec<u8>, GenericError> {
    k256_helper::get_public_key(private_key_bytes.as_slice())
        .map_err(|err| GenericError::Generic { err })
}

#[uniffi::export]
pub fn recover_public_key_k256_sha256(
    message: Vec<u8>,
    signature: Vec<u8>,
) -> Result<Vec<u8>, GenericError> {
    k256_helper::recover_public_key_predigest_sha256(message.as_slice(), signature.as_slice())
        .map_err(|err| GenericError::Generic { err })
}

#[uniffi::export]
fn recover_public_key_k256_keccak256(
    message: Vec<u8>,
    signature: Vec<u8>,
) -> Result<Vec<u8>, GenericError> {
    k256_helper::recover_public_key_predigest_keccak256(message.as_slice(), signature.as_slice())
        .map_err(|err| GenericError::Generic { err })
}

// Need to move xmtp_user_preferences into main
#[uniffi::export]
fn user_preferences_encrypt(
    public_key: Vec<u8>,
    private_key: Vec<u8>,
    message: Vec<u8>,
) -> Result<Vec<u8>, GenericError> {
    let ciphertext = xmtp_user_preferences::encrypt_message(
        public_key.as_slice(),
        private_key.as_slice(),
        message.as_slice(),
    )
    .map_err(|err| GenericError::Generic { err })?;

    Ok(ciphertext)
}

#[uniffi::export]
fn user_preferences_decrypt(
    public_key: Vec<u8>,
    private_key: Vec<u8>,
    message: Vec<u8>,
) -> Result<Vec<u8>, GenericError> {
    let ciphertext = xmtp_user_preferences::decrypt_message(
        public_key.as_slice(),
        private_key.as_slice(),
        message.as_slice(),
    )
    .map_err(|err| GenericError::Generic { err })?;

    Ok(ciphertext)
}

#[uniffi::export]
fn generate_private_preferences_topic_identifier(
    private_key: Vec<u8>,
) -> Result<String, GenericError> {
    xmtp_user_preferences::topic::generate_private_preferences_topic_identifier(
        private_key.as_slice(),
    )
    .map_err(|err| GenericError::Generic { err })
}

#[uniffi::export]
pub fn diffie_hellman_k256(
    private_key_bytes: Vec<u8>,
    public_key_bytes: Vec<u8>,
) -> Result<Vec<u8>, GenericError> {
    let shared_secret = k256_helper::diffie_hellman_byte_params(
        private_key_bytes.as_slice(),
        public_key_bytes.as_slice(),
    )
    .map_err(|err| GenericError::Generic { err })?;

    Ok(shared_secret)
}

#[uniffi::export]
pub fn verify_k256_sha256(
    signed_by: Vec<u8>,
    message: Vec<u8>,
    signature: Vec<u8>,
    recovery_id: u8,
) -> Result<bool, GenericError> {
    let result = xmtp_v2::k256_helper::verify_sha256(
        signed_by.as_slice(),
        message.as_slice(),
        signature.as_slice(),
        recovery_id,
    )
    .map_err(|err| GenericError::Generic { err })?;

    Ok(result)
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Mutex,
    };
    use tokio::sync::Notify;

    use futures::stream;
    use xmtp_proto::api_client::{Envelope, Error as ApiError};

    use crate::v2::{
        create_v2_client, FfiEnvelope, FfiPublishRequest, FfiV2SubscribeRequest, FfiV2Subscription,
    };

    use super::FfiV2SubscriptionCallback;

    #[derive(Default, Clone)]
    pub struct TestStreamCallback {
        message_count: Arc<AtomicU32>,
        messages: Arc<Mutex<Vec<FfiEnvelope>>>,
        notify: Arc<Notify>,
    }

    impl FfiV2SubscriptionCallback for TestStreamCallback {
        fn on_message(&self, message: FfiEnvelope) {
            self.message_count.fetch_add(1, Ordering::SeqCst);
            let mut messages = self.messages.lock().unwrap();
            messages.push(message);
            self.notify.notify_one();
        }
    }

    // Try a query on a test topic, and make sure we get a response
    #[tokio::test]
    async fn test_recover_public_key_keccak256() {
        // This test was generated using Etherscans Signature tool: https://etherscan.io/verifySig/18959
        let addr = "0x1B2a516d691aBb8f08a75B2C73c95c62A1632431";
        let msg = "TestVector1";
        let sig_hash = "19d6bec562518e365d07ba3cce26d08a5fffa2cbb1e7fe03c1f2d6a722fd3a5e544097b91f8f8cd11d43b032659f30529139ab1a9ecb6c81ed4a762179e87db81c";

        let sig_bytes = ethers_core::utils::hex::decode(sig_hash).unwrap();
        let recovered_addr = crate::v2::recover_address(sig_bytes, msg.to_string()).unwrap();
        assert_eq!(recovered_addr, addr.to_lowercase());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_subscribe() {
        let items: Vec<Result<Envelope, ApiError>> = vec![
            Ok(Envelope {
                content_topic: "test1".to_string(),
                timestamp_ns: 0,
                message: vec![],
            }),
            Ok(Envelope {
                content_topic: "test2".to_string(),
                timestamp_ns: 0,
                message: vec![],
            }),
        ];
        let stream = stream::iter(items);
        let (tx, _) = futures::channel::mpsc::unbounded();

        let callback = TestStreamCallback::default();
        let local_data = callback.clone();
        FfiV2Subscription::subscribe(
            xmtp_api_grpc::grpc_api_helper::GrpcMutableSubscription::new(Box::pin(stream), tx),
            Box::new(callback),
        )
        .await;

        for _ in 0..2 {
            local_data.notify.notified().await;
        }

        let messages = local_data.messages.lock().unwrap();
        let message_count = local_data.message_count.clone();
        assert_eq!(message_count.load(Ordering::SeqCst), 2);
        assert_eq!(messages[0].content_topic, "test1");
        assert_eq!(messages[1].content_topic, "test2");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_subscription_close() {
        let items: Vec<Result<Envelope, ApiError>> = vec![
            Ok(Envelope {
                content_topic: "test1".to_string(),
                timestamp_ns: 0,
                message: vec![],
            }),
            Ok(Envelope {
                content_topic: "test2".to_string(),
                timestamp_ns: 0,
                message: vec![],
            }),
        ];
        let stream = stream::iter(items);
        let (tx, _) = futures::channel::mpsc::unbounded();

        let callback = TestStreamCallback::default();
        let local_data = callback.clone();
        let sub = FfiV2Subscription::subscribe(
            xmtp_api_grpc::grpc_api_helper::GrpcMutableSubscription::new(Box::pin(stream), tx),
            Box::new(callback),
        )
        .await;

        for _ in 0..2 {
            local_data.notify.notified().await;
        }

        {
            let messages = local_data.messages.lock().unwrap();
            assert_eq!(messages[0].content_topic, "test1");
        }
        // Close the subscription
        sub.end().await.unwrap();
        assert!(sub.is_closed());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_e2e() {
        let client = create_v2_client("http://localhost:5556".to_string(), false)
            .await
            .unwrap();
        let content_topic = format!("/xmtp/0/{}", uuid::Uuid::new_v4());

        let callback = TestStreamCallback::default();
        let local_data = callback.clone();
        let subscription = client
            .subscribe(
                FfiV2SubscribeRequest {
                    content_topics: vec![content_topic.to_string()],
                },
                Box::new(callback),
            )
            .await
            .unwrap();

        client
            .publish(
                FfiPublishRequest {
                    envelopes: vec![FfiEnvelope {
                        content_topic: content_topic.to_string(),
                        timestamp_ns: 3,
                        message: vec![1, 2, 3],
                    }],
                },
                "".to_string(),
            )
            .await
            .unwrap();

        local_data.notify.notified().await;
        {
            let messages = local_data.messages.lock().unwrap();
            let message_count = local_data.message_count.load(Ordering::SeqCst);
            assert_eq!(message_count, 1);
            assert_eq!(messages[0].content_topic, content_topic);
            assert_eq!(messages[0].timestamp_ns, 3);
            assert_eq!(messages[0].message, vec![1, 2, 3]);
        }
        println!("ENDING SUB");
        let _ = subscription.end().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_update_e2e() {
        let client = create_v2_client("http://localhost:5556".to_string(), false)
            .await
            .unwrap();

        let content_topic = format!("/xmtp/0/{}", uuid::Uuid::new_v4());
        let other_topic = format!("/xmtp/0/{}", uuid::Uuid::new_v4());

        let callback = TestStreamCallback::default();
        let local_data = callback.clone();
        let sub = client
            .subscribe(
                FfiV2SubscribeRequest {
                    content_topics: vec![content_topic.to_string()],
                },
                Box::new(callback),
            )
            .await
            .unwrap();

        client
            .publish(
                FfiPublishRequest {
                    envelopes: vec![FfiEnvelope {
                        content_topic: content_topic.to_string(),
                        timestamp_ns: 3,
                        message: vec![1, 2, 3],
                    }],
                },
                "".to_string(),
            )
            .await
            .unwrap();

        local_data.notify.notified().await;

        {
            let messages = local_data.messages.lock().unwrap();
            let message_count = local_data.message_count.load(Ordering::SeqCst);
            assert_eq!(message_count, 1);
            assert_eq!(messages[0].content_topic, content_topic);
            assert_eq!(messages[0].timestamp_ns, 3);
            assert_eq!(messages[0].message, vec![1, 2, 3]);
        }

        sub.update(FfiV2SubscribeRequest {
            content_topics: vec![other_topic.to_string()],
        })
        .await
        .unwrap();

        client
            .publish(
                FfiPublishRequest {
                    envelopes: vec![FfiEnvelope {
                        content_topic: other_topic.to_string(),
                        timestamp_ns: 3,
                        message: vec![1, 2, 3],
                    }],
                },
                "".to_string(),
            )
            .await
            .unwrap();

        local_data.notify.notified().await;

        {
            let messages = local_data.messages.lock().unwrap();
            let message_count = local_data.message_count.load(Ordering::SeqCst);
            assert_eq!(message_count, 2);
            assert_eq!(messages[1].content_topic, other_topic);
            assert_eq!(messages[1].timestamp_ns, 3);
            assert_eq!(messages[1].message, vec![1, 2, 3]);
        }
    }
}
