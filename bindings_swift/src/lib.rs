use types::BatchQueryResponse;
use types::QueryResponse;
use xmtp_crypto::{hashes, k256_helper};
use xmtp_networking::grpc_api_helper;
use xmtp_proto::xmtp::message_api::v1::{Envelope as EnvelopeProto, PagingInfo, QueryRequest};
pub mod types;

#[swift_bridge::bridge]
mod ffi {
    enum SortDirection {
        Unspecified = 0,
        Ascending = 1,
        Descending = 2,
    }

    #[swift_bridge(swift_repr = "struct")]
    struct IndexCursor {
        digest: Vec<u8>,
        sender_time_ns: u64,
    }
    #[swift_bridge(swift_repr = "struct")]
    struct PagingInfo {
        limit: u32,
        cursor: Option<IndexCursor>,
        direction: SortDirection,
    }

    extern "Rust" {
        type Envelope;

        fn create_envelope(topic: String, sender_time_ns: u64, payload: Vec<u8>) -> Envelope;

        fn get_topic(&self) -> String;
        fn get_sender_time_ns(&self) -> u64;
        fn get_payload(&self) -> Vec<u8>;
    }

    extern "Rust" {
        type RustSubscription;

        fn get_messages(&self) -> Result<Vec<Envelope>, String>;

        fn close(&mut self);
    }

    extern "Rust" {
        type QueryRequest;
    }

    extern "Rust" {
        type QueryResponse;

        fn envelopes(&self) -> Vec<Envelope>;
        fn paging_info(&self) -> Option<PagingInfo>;

    }

    extern "Rust" {
        type BatchQueryResponse;

        fn responses(&self) -> Vec<QueryResponse>;
    }

    extern "Rust" {
        type RustClient;

        async fn create_client(host: String, is_secure: bool) -> Result<RustClient, String>;

        async fn batch_query(
            &mut self,
            requests: Vec<QueryRequest>,
        ) -> Result<BatchQueryResponse, String>;

        async fn query(
            &mut self,
            topic: String,
            start_time_ns: Option<u64>,
            end_time_ns: Option<u64>,
            paging_info: Option<PagingInfo>,
        ) -> Result<QueryResponse, String>;

        async fn publish(
            &mut self,
            token: String,
            envelopes: Vec<Envelope>,
        ) -> Result<String, String>;

        async fn subscribe(&mut self, topics: Vec<String>) -> Result<RustSubscription, String>;
    }

    extern "Rust" {
        fn sha256(data: Vec<u8>) -> Vec<u8>;
        fn keccak256(data: Vec<u8>) -> Vec<u8>;
        fn verify_k256_sha256(
            public_key_bytes: Vec<u8>,
            message: Vec<u8>,
            signature: Vec<u8>,
            recovery_id: u8,
        ) -> Result<String, String>;
        fn diffie_hellman_k256(
            private_key_bytes: Vec<u8>,
            public_key_bytes: Vec<u8>,
        ) -> Result<Vec<u8>, String>;
        fn public_key_from_private_key_k256(private_key_bytes: Vec<u8>) -> Result<Vec<u8>, String>;
        fn recover_public_key_k256_sha256(
            message: Vec<u8>,
            signature: Vec<u8>,
        ) -> Result<Vec<u8>, String>;
        fn recover_public_key_k256_keccak256(
            message: Vec<u8>,
            signature: Vec<u8>,
        ) -> Result<Vec<u8>, String>;
    }
}

pub struct RustClient {
    client: grpc_api_helper::Client,
}

async fn create_client(host: String, is_secure: bool) -> Result<RustClient, String> {
    let client = grpc_api_helper::Client::create(host, is_secure)
        .await
        .map_err(|e| format!("{}", e))?;

    Ok(RustClient { client })
}

impl RustClient {
    async fn batch_query(
        &mut self,
        requests: Vec<QueryRequest>,
    ) -> Result<BatchQueryResponse, String> {
        let result = self
            .client
            .batch_query(requests)
            .await
            .map_err(|e| format!("{}", e))?;

        Ok(BatchQueryResponse::from(result))
    }

    async fn query(
        &mut self,
        topic: String,
        start_time_ns: Option<u64>,
        end_time_ns: Option<u64>,
        paging_info: Option<ffi::PagingInfo>,
    ) -> Result<QueryResponse, String> {
        let info = paging_info.map(PagingInfo::from);

        let result = self
            .client
            .query(topic, start_time_ns, end_time_ns, info)
            .await
            .map_err(|e| format!("{}", e))?;

        Ok(QueryResponse::from(result))
    }

    // Left a u32 in there to either 1) return the number of envelopes published or 2) return an error code
    async fn publish(&mut self, token: String, envelopes: Vec<Envelope>) -> Result<String, String> {
        let mut xmtp_envelopes = vec![];
        for envelope in envelopes {
            xmtp_envelopes.push(EnvelopeProto::from(envelope));
        }

        self.client
            .publish(token, xmtp_envelopes)
            .await
            .map_err(|e| format!("{}", e))?;

        // Swift-Bridge needs this to be a type that has an initializer in Swift
        // after conversion, such as RustString
        Ok("".to_string())
    }

    async fn subscribe(&mut self, topics: Vec<String>) -> Result<RustSubscription, String> {
        let subscription = self
            .client
            .subscribe(topics)
            .await
            .map_err(|e| format!("{}", e))?;

        Ok(RustSubscription { subscription })
    }
}

pub struct RustSubscription {
    subscription: grpc_api_helper::Subscription,
}

impl RustSubscription {
    pub fn get_messages(&self) -> Result<Vec<Envelope>, String> {
        let new_messages = self.subscription.get_messages();
        // Return the last envelopes even if the stream is closed
        if !new_messages.is_empty() {
            return Ok(new_messages
                .iter()
                .map(|e| Envelope::from(e.clone()))
                .collect());
        }
        // If the stream is closed AND empty, return an error
        if self.subscription.is_closed() {
            return Err("subscription_closed".to_string());
        }

        Ok(vec![])
    }

    pub fn close(&mut self) {
        self.subscription.close_stream();
        // Think I am going to have to do some manual memory management to ensure everything gets dropped
    }
}

// Define as an opaque type so we can make it Vectorizable
#[derive(Clone)]
pub struct Envelope {
    pub content_topic: String,
    pub timestamp_ns: u64,
    pub message: Vec<u8>,
}

pub fn create_envelope(topic: String, timestamp_ns: u64, message: Vec<u8>) -> Envelope {
    Envelope {
        content_topic: topic,
        timestamp_ns,
        message,
    }
}

impl Envelope {
    pub fn get_topic(&self) -> String {
        self.content_topic.clone()
    }

    pub fn get_sender_time_ns(&self) -> u64 {
        self.timestamp_ns
    }

    pub fn get_payload(&self) -> Vec<u8> {
        self.message.clone()
    }
}

impl Clone for ffi::IndexCursor {
    fn clone(&self) -> Self {
        ffi::IndexCursor {
            digest: self.digest.clone(),
            sender_time_ns: self.sender_time_ns,
        }
    }
}

impl Clone for ffi::PagingInfo {
    fn clone(&self) -> Self {
        ffi::PagingInfo {
            limit: self.limit,
            cursor: self.cursor.clone(),
            direction: self.direction,
        }
    }
}

// Cryptography helper functions
fn sha256(data: Vec<u8>) -> Vec<u8> {
    let result = hashes::sha256(data.as_slice());
    result.to_vec()
}

fn keccak256(data: Vec<u8>) -> Vec<u8> {
    let result = hashes::keccak256(data.as_slice());
    result.to_vec()
}

fn verify_k256_sha256(
    public_key_bytes: Vec<u8>,
    message: Vec<u8>,
    signature: Vec<u8>,
    recovery_id: u8,
) -> Result<String, String> {
    k256_helper::verify_sha256(
        public_key_bytes.as_slice(),
        message.as_slice(),
        signature.as_slice(),
        recovery_id,
    )
    .map(|_| "ok".to_string())
    .map_err(|e| format!("VerifyError: {}", e))
}

fn diffie_hellman_k256(
    private_key_bytes: Vec<u8>,
    public_key_bytes: Vec<u8>,
) -> Result<Vec<u8>, String> {
    k256_helper::diffie_hellman_byte_params(
        private_key_bytes.as_slice(),
        public_key_bytes.as_slice(),
    )
    .map_err(|e| format!("ECDHError: {}", e))
}

fn public_key_from_private_key_k256(private_key_bytes: Vec<u8>) -> Result<Vec<u8>, String> {
    k256_helper::get_public_key(private_key_bytes.as_slice())
        .map_err(|e| format!("PublicKeyError: {}", e))
}

// Expects signature to be 65 bytes (last byte is recovery id 0-3)
fn recover_public_key_k256_sha256(message: Vec<u8>, signature: Vec<u8>) -> Result<Vec<u8>, String> {
    k256_helper::recover_public_key_predigest_sha256(message.as_slice(), signature.as_slice())
        .map_err(|e| format!("RecoverError: k256_sha256: {}", e))
}

fn recover_public_key_k256_keccak256(
    message: Vec<u8>,
    signature: Vec<u8>,
) -> Result<Vec<u8>, String> {
    k256_helper::recover_public_key_predigest_keccak256(message.as_slice(), signature.as_slice())
        .map_err(|e| format!("RecoverError k256_keccak256: {}", e))
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};
    use uuid::Uuid;

    static ADDRESS: &str = "http://localhost:5556";

    pub fn test_envelope(topic: String) -> super::Envelope {
        let time_since_epoch = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();

        super::Envelope {
            timestamp_ns: time_since_epoch.as_nanos() as u64,
            content_topic: topic,
            message: vec![65],
        }
    }

    #[tokio::test]
    async fn test_batch_query() {
        let mut client = super::create_client(ADDRESS.to_string(), false)
            .await
            .unwrap();
        let result = client.batch_query(vec![]).await.unwrap();
        assert_eq!(result.responses().len(), 0);
    }

    // Try a query on a test topic, and make sure we get a response
    #[tokio::test]
    async fn test_publish_query() {
        let mut client = super::create_client(ADDRESS.to_string(), false)
            .await
            .unwrap();
        let topic = Uuid::new_v4();
        let publish_result = client
            .publish("".to_string(), vec![test_envelope(topic.to_string())])
            .await
            .unwrap();

        assert_eq!(publish_result, "".to_string());

        let result = client
            .query(topic.to_string(), None, None, None)
            .await
            .unwrap();

        let envelopes = result.envelopes();
        assert_eq!(envelopes.len(), 1);

        let first_envelope = envelopes.get(0).unwrap();
        assert_eq!(first_envelope.content_topic, topic.to_string());
        assert!(first_envelope.timestamp_ns > 0);
        assert!(!first_envelope.message.is_empty());
    }

    #[tokio::test]
    async fn test_subscribe() {
        let topic = Uuid::new_v4();
        let mut client = super::create_client(ADDRESS.to_string(), false)
            .await
            .unwrap();
        let mut sub = client.subscribe(vec![topic.to_string()]).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let publish_result = client
            .publish("".to_string(), vec![test_envelope(topic.to_string())])
            .await
            .unwrap();
        assert_eq!(publish_result, "".to_string());
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let messages = sub.get_messages().unwrap();
        assert_eq!(messages.len(), 1);
        let messages = sub.get_messages().unwrap();
        assert_eq!(messages.len(), 0);

        sub.close();
        assert!(sub.get_messages().is_err());
    }
}
