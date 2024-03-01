use crate::GenericError;
use futures::StreamExt;
use std::sync::Arc;
use xmtp_proto::api_client::{
    BatchQueryResponse, MutableApiSubscription, PagingInfo, QueryResponse, SubscribeRequest,
    XmtpApiClient,
};
use xmtp_proto::xmtp::message_api::v1::IndexCursor;
use xmtp_v2::{hashes, k256_helper};

use xmtp_api_grpc::grpc_api_helper::Client as GrpcClient;
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
                cursor: None,
            }),
        }
    }
}

impl From<FfiV2QueryResponse> for QueryResponse {
    fn from(resp: FfiV2QueryResponse) -> Self {
        Self {
            envelopes: resp.envelopes.into_iter().map(|env| env.into()).collect(),
            paging_info: resp.paging_info.map(|paging_info| {
                PagingInfo {
                    limit: paging_info.limit,
                    direction: paging_info.direction as i32,
                    cursor: None, // TODO: fix me
                }
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

#[derive(uniffi::Object)]
pub struct FfiV2Subscription {
    inner_subscription:
        Arc<futures::lock::Mutex<xmtp_api_grpc::grpc_api_helper::GrpcMutableSubscription>>,
}

impl From<xmtp_api_grpc::grpc_api_helper::GrpcMutableSubscription> for FfiV2Subscription {
    fn from(subscription: xmtp_api_grpc::grpc_api_helper::GrpcMutableSubscription) -> Self {
        Self {
            inner_subscription: Arc::new(futures::lock::Mutex::new(subscription)),
        }
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl FfiV2Subscription {
    pub async fn next(&self) -> Result<FfiEnvelope, GenericError> {
        let mut sub = self.inner_subscription.lock().await;

        let result = sub.next().await;
        match result {
            Some(Ok(envelope)) => Ok(envelope.into()),
            Some(Err(err)) => Err(GenericError::Generic {
                err: err.to_string(),
            }),
            None => Err(GenericError::Generic {
                err: "stream closed".to_string(),
            }),
        }
    }

    pub async fn update(&self, req: FfiV2SubscribeRequest) -> Result<(), GenericError> {
        let mut sub = self.inner_subscription.lock().await;
        sub.update(req.into()).await?;
        Ok(())
    }

    pub async fn end(&self) {
        let sub = self.inner_subscription.lock().await;
        sub.close();
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
    ) -> Result<Arc<FfiV2Subscription>, GenericError> {
        let result = self.inner_client.subscribe2(request.into()).await?;
        Ok(Arc::new(result.into()))
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
    use std::sync::Arc;

    use futures::stream;
    use xmtp_proto::api_client::{Envelope, Error as ApiError};

    use crate::v2::FfiV2Subscription;

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
        let stream_handler = FfiV2Subscription {
            inner_subscription: Arc::new(futures::lock::Mutex::new(
                xmtp_api_grpc::grpc_api_helper::GrpcMutableSubscription::new(Box::pin(stream), tx),
            )),
        };

        let first = stream_handler.next().await.unwrap();
        assert_eq!(first.content_topic, "test1");
        let second = stream_handler.next().await.unwrap();
        assert_eq!(second.content_topic, "test2");
        let third = stream_handler.next().await;
        assert!(third.is_err());
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
        let stream_handler = FfiV2Subscription {
            inner_subscription: Arc::new(futures::lock::Mutex::new(
                xmtp_api_grpc::grpc_api_helper::GrpcMutableSubscription::new(Box::pin(stream), tx),
            )),
        };

        let first = stream_handler.next().await.unwrap();
        assert_eq!(first.content_topic, "test1");

        // Close the subscription
        stream_handler.end().await;
        let second = stream_handler.next().await;
        assert!(second.is_err());
    }
}
