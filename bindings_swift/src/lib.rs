use prost::Message;
use xmtp::types::networking::{XmtpApiClient, XmtpApiSubscription};
use xmtp_crypto::{hashes, k256_helper};
use xmtp_networking::grpc_api_helper;
use xmtp_proto::xmtp::message_api::v1::{
    BatchQueryRequest, BatchQueryResponse, PublishRequest, PublishResponse, QueryRequest,
    QueryResponse, SubscribeRequest,
};

#[swift_bridge::bridge]
mod ffi {
    extern "Rust" {
        type RustSubscription;

        fn get_envelopes_as_query_response(&self) -> Result<Vec<u8>, String>;

        fn close(&mut self);
    }

    extern "Rust" {
        type RustClient;

        async fn create_client(host: String, is_secure: bool) -> Result<RustClient, String>;
        async fn batch_query(&mut self, req: Vec<u8>) -> Result<Vec<u8>, String>;
        async fn query(&mut self, req: Vec<u8>) -> Result<Vec<u8>, String>;
        async fn publish(&mut self, token: String, req: Vec<u8>) -> Result<Vec<u8>, String>;
        async fn subscribe(&mut self, req: Vec<u8>) -> Result<RustSubscription, String>;
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
    async fn batch_query(&mut self, req: Vec<u8>) -> Result<Vec<u8>, String> {
        let request: BatchQueryRequest = Message::decode(&req[..]).map_err(|e| format!("{}", e))?;
        let result: BatchQueryResponse = self
            .client
            .batch_query(request)
            .await
            .map_err(|e| format!("{}", e))?;
        Ok(result.encode_to_vec())
    }

    async fn query(&mut self, req: Vec<u8>) -> Result<Vec<u8>, String> {
        let request: QueryRequest = Message::decode(&req[..]).map_err(|e| format!("{}", e))?;
        let result: QueryResponse = self
            .client
            .query(request)
            .await
            .map_err(|e| format!("{}", e))?;
        Ok(result.encode_to_vec())
    }

    async fn publish(&mut self, token: String, req: Vec<u8>) -> Result<Vec<u8>, String> {
        let request: PublishRequest = Message::decode(&req[..]).map_err(|e| format!("{}", e))?;
        let result: PublishResponse = self
            .client
            .publish(token, request)
            .await
            .map_err(|e| format!("{}", e))?;
        Ok(result.encode_to_vec())
    }

    async fn subscribe(&mut self, req: Vec<u8>) -> Result<RustSubscription, String> {
        let request: SubscribeRequest = Message::decode(&req[..]).map_err(|e| format!("{}", e))?;
        let subscription = self
            .client
            .subscribe(request)
            .await
            .map_err(|e| format!("{}", e))?;

        Ok(RustSubscription { subscription })
    }
}

pub struct RustSubscription {
    subscription: grpc_api_helper::Subscription,
}

impl RustSubscription {
    // Returns a serialized `QueryResponse` as a convenient envelopes wrapper.
    pub fn get_envelopes_as_query_response(&self) -> Result<Vec<u8>, String> {
        let new_messages = self.subscription.get_messages();
        if new_messages.is_empty() {
            // If the stream is closed AND empty, return an error
            if self.subscription.is_closed() {
                return Err("subscription_closed".to_string());
            }
        }
        return Ok(QueryResponse {
            envelopes: new_messages,
            ..QueryResponse::default()
        }
        .encode_to_vec());
    }

    pub fn close(&mut self) {
        self.subscription.close_stream();
        // Think I am going to have to do some manual memory management to ensure everything gets dropped
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
    use prost::Message;
    use std::time::{SystemTime, UNIX_EPOCH};
    use uuid::Uuid;
    use xmtp_proto::xmtp::message_api::v1::{
        BatchQueryRequest, BatchQueryResponse, Envelope, PublishRequest, QueryRequest,
        QueryResponse, SubscribeRequest,
    };

    pub fn test_envelope(topic: String) -> Envelope {
        let time_since_epoch = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();

        Envelope {
            timestamp_ns: time_since_epoch.as_nanos() as u64,
            content_topic: topic,
            message: vec![65],
        }
    }

    #[tokio::test]
    async fn test_publish_batch_query() {
        let mut client =
            super::create_client(xmtp_networking::LOCALHOST_ADDRESS.to_string(), false)
                .await
                .unwrap();

        let topic = Uuid::new_v4();
        client
            .publish(
                "".to_string(),
                PublishRequest {
                    envelopes: vec![test_envelope(topic.to_string())],
                }
                .encode_to_vec(),
            )
            .await
            .unwrap();

        let q = QueryRequest {
            content_topics: vec![topic.to_string()],
            ..QueryRequest::default()
        };
        let req = BatchQueryRequest { requests: vec![q] }.encode_to_vec();

        let result = client.batch_query(req).await.unwrap();

        let res = BatchQueryResponse::decode(&result[..]).unwrap();
        assert_eq!(res.responses.len(), 1);
    }

    // Try a query on a test topic, and make sure we get a response
    #[tokio::test]
    async fn test_publish_query() {
        let mut client =
            super::create_client(xmtp_networking::LOCALHOST_ADDRESS.to_string(), false)
                .await
                .unwrap();
        let topic = Uuid::new_v4();
        client
            .publish(
                "".to_string(),
                PublishRequest {
                    envelopes: vec![test_envelope(topic.to_string())],
                }
                .encode_to_vec(),
            )
            .await
            .unwrap();

        let result = client
            .query(
                QueryRequest {
                    content_topics: vec![topic.to_string()],
                    ..QueryRequest::default()
                }
                .encode_to_vec(),
            )
            .await
            .unwrap();

        let q: QueryResponse = Message::decode(&result[..]).unwrap();
        assert_eq!(q.envelopes.len(), 1);

        let first_envelope = q.envelopes.get(0).unwrap();
        assert_eq!(first_envelope.content_topic, topic.to_string());
        assert!(first_envelope.timestamp_ns > 0);
        assert!(!first_envelope.message.is_empty());
    }

    #[tokio::test]
    async fn test_subscribe() {
        let topic = Uuid::new_v4();
        let mut client =
            super::create_client(xmtp_networking::LOCALHOST_ADDRESS.to_string(), false)
                .await
                .unwrap();
        let mut sub = client
            .subscribe(
                SubscribeRequest {
                    content_topics: vec![topic.to_string()],
                    ..SubscribeRequest::default()
                }
                .encode_to_vec(),
            )
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        client
            .publish(
                "".to_string(),
                PublishRequest {
                    envelopes: vec![test_envelope(topic.to_string())],
                }
                .encode_to_vec(),
            )
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // At first there's the one envelope we got.
        let result = sub.get_envelopes_as_query_response().unwrap();
        let res: QueryResponse = Message::decode(&result[..]).unwrap();
        assert_eq!(res.envelopes.len(), 1);

        // But the next time it's empty.
        let result = sub.get_envelopes_as_query_response().unwrap();
        let res: QueryResponse = Message::decode(&result[..]).unwrap();
        assert_eq!(res.envelopes.len(), 0);

        // And now since it is closed, it should error.
        sub.close();
        assert!(sub.get_envelopes_as_query_response().is_err());
    }
}
