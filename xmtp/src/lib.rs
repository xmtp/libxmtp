pub mod account;
pub mod association;
pub mod builder;
pub mod client;
mod codecs;
pub mod contact;
pub mod conversation;
pub mod conversations;
pub mod message;
pub mod mock_xmtp_api_client;
pub mod owner;
pub mod persistence;
pub mod session;
pub mod storage;
mod test_utils;
#[cfg(feature = "types")]
pub mod types;
mod utils;
pub mod vmac_protos;

pub use builder::ClientBuilder;
pub use client::{Client, Network};
use storage::StorageError;
use xmtp_cryptography::signature::{RecoverableSignature, SignatureError};

pub use codecs::{text::TextCodec, ContentCodec};

pub trait Signable {
    fn bytes_to_sign(&self) -> Vec<u8>;
}

// Inserts a model to the underlying data store
pub trait Store<I> {
    fn store(&self, into: &mut I) -> Result<(), StorageError>;
}

pub trait Fetch<T> {
    type Key<'a>;
    // Fetches all instances of a model from the underlying data store
    fn fetch_all(&mut self) -> Result<Vec<T>, StorageError>;

    // Fetches a single instance by key of a model from the underlying data store
    #[allow(clippy::needless_lifetimes)]
    fn fetch_one<'a>(&mut self, key: Self::Key<'a>) -> Result<Option<T>, StorageError>;
}

// Updates an existing instance of the model in the data store
pub trait Save<I> {
    fn save(&self, into: &mut I) -> Result<(), StorageError>;
}

pub trait InboxOwner {
    fn get_address(&self) -> String;
    fn sign(&self, text: &str) -> Result<RecoverableSignature, SignatureError>;
}

#[cfg(test)]
mod tests {
    use crate::builder::ClientBuilder;
    use std::time::{SystemTime, UNIX_EPOCH};
    use uuid::Uuid;
    use xmtp_proto::api_client::XmtpApiClient;
    use xmtp_proto::xmtp::message_api::v1::{Envelope, PublishRequest, QueryRequest};

    fn gen_test_envelope(topic: String) -> Envelope {
        let time_since_epoch = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();

        Envelope {
            timestamp_ns: time_since_epoch.as_nanos() as u64,
            content_topic: topic,
            message: vec![65],
        }
    }

    #[tokio::test]
    async fn can_network() {
        let client = ClientBuilder::new_test().build().unwrap();
        let topic = Uuid::new_v4();

        client
            .api_client
            .publish(
                "".to_string(),
                PublishRequest {
                    envelopes: vec![gen_test_envelope(topic.to_string())],
                },
            )
            .await
            .unwrap();

        let result = client
            .api_client
            .query(QueryRequest {
                content_topics: vec![topic.to_string()],
                start_time_ns: 0,
                end_time_ns: 0,
                paging_info: None,
            })
            .await
            .unwrap();

        let envelopes = result.envelopes;
        assert_eq!(envelopes.len(), 1);

        let first_envelope = envelopes.get(0).unwrap();
        assert_eq!(first_envelope.content_topic, topic.to_string());
        assert!(first_envelope.timestamp_ns > 0);
        assert!(!first_envelope.message.is_empty());
    }
}
