pub mod account;
pub mod association;
pub mod builder;
pub mod client;
pub mod contact;
pub mod conversation;
pub mod conversations;
pub mod networking;
pub mod owner;
pub mod persistence;
pub mod session;
pub mod storage;
mod types;
mod utils;
pub mod vmac_protos;

use association::AssociationText;
pub use builder::ClientBuilder;
pub use client::{Client, Network};
use diesel::{
    r2d2::{ConnectionManager, PooledConnection},
    SqliteConnection,
};
use storage::StorageError;
use xmtp_cryptography::signature::{RecoverableSignature, SignatureError};

pub trait Signable {
    fn bytes_to_sign(&self) -> Vec<u8>;
}

pub trait Errorer {
    type Error;
}

pub trait Store<I> {
    fn store(&self, into: I) -> Result<(), StorageError>;
}

pub trait Fetch<C, T> {
    fn fetch(conn: C) -> Result<Vec<T>, StorageError>;
}

pub trait InboxOwner {
    fn get_address(&self) -> String;
    fn sign(&self, text: AssociationText) -> Result<RecoverableSignature, SignatureError>;
}

pub type PooledSqliteConnection = PooledConnection<ConnectionManager<SqliteConnection>>;

#[cfg(test)]
mod tests {
    use crate::{builder::ClientBuilder, networking::XmtpApiClient};
    use std::time::{SystemTime, UNIX_EPOCH};
    use uuid::Uuid;
    use xmtp_proto::xmtp::message_api::v1::Envelope;

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
        let mut client = ClientBuilder::new_test().build().unwrap();
        let topic = Uuid::new_v4();

        client
            .api_client
            .publish("".to_string(), vec![gen_test_envelope(topic.to_string())])
            .await
            .unwrap();

        let result = client
            .api_client
            .query(topic.to_string(), None, None, None)
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
