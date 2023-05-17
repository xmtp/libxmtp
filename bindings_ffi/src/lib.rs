use async_trait::async_trait;
// use types::QueryResponse;
use xmtp::{
    account::AccountCreator, networking::XmtpApiClient,
    persistence::in_memory_persistence::InMemoryPersistence,
};
use xmtp_crypto::{hashes, k256_helper};
use xmtp_networking::grpc_api_helper;
use xmtp_proto::xmtp::message_api::v1::{Envelope, PagingInfo};
pub mod types;

type FfiXmtpClient = xmtp::Client<FfiApiClient, InMemoryPersistence>;

#[swift_bridge::bridge]
mod ffi {
    extern "Rust" {
        type FfiXmtpClient;

        async fn create_client(
            wallet_address: &str,
            host: &str,
            is_secure: bool,
        ) -> Result<FfiXmtpClient, String>;
    }
}

async fn create_client(
    wallet_address: &str,
    host: &str,
    is_secure: bool,
) -> Result<xmtp::Client<FfiApiClient, InMemoryPersistence>, String> {
    let account = AccountCreator::new().finalize_key(Vec::new()); // TODO sign key with wallet address
    let api_client = FfiApiClient::new(host, is_secure)
        .await
        .map_err(|e| format!("{}", e))?;
    let persistence = InMemoryPersistence::new();

    let xmtp_client = xmtp::ClientBuilder::new()
        .account(account)
        .api_client(api_client)
        .persistence(persistence)
        .wallet_address(wallet_address)
        .build()
        .unwrap();

    Ok(xmtp_client)
}

struct FfiApiClient {
    client: grpc_api_helper::Client,
}

impl FfiApiClient {
    async fn new(host: &str, is_secure: bool) -> Result<Self, String> {
        let client = grpc_api_helper::Client::create(host, is_secure)
            .await
            .map_err(|e| format!("{}", e))?;

        Ok(Self { client })
    }
}

#[async_trait]
impl XmtpApiClient for FfiApiClient {
    async fn publish(
        &mut self,
        token: String,
        envelopes: Vec<Envelope>,
        // TODO: use error enums
    ) -> Result<xmtp_proto::xmtp::message_api::v1::PublishResponse, String> {
        self.client
            .publish(token, envelopes)
            .await
            .map_err(|e| format!("{}", e))
    }

    async fn query(
        &mut self,
        topic: String,
        start_time: Option<u64>,
        end_time: Option<u64>,
        paging_info: Option<PagingInfo>,
        // TODO: use error enums
    ) -> Result<xmtp_proto::xmtp::message_api::v1::QueryResponse, String> {
        self.client
            .query(topic, start_time, end_time, paging_info)
            .await
            .map_err(|e| format!("{}", e))
    }

    async fn subscribe(
        &mut self,
        topics: Vec<String>,
    ) -> Result<grpc_api_helper::Subscription, String> {
        self.client
            .subscribe(topics)
            .await
            .map_err(|e| format!("{}", e))
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};
    use uuid::Uuid;

    static ADDRESS: &str = "http://localhost:5556";

    // pub fn test_envelope(topic: String) -> super::Envelope {
    //     let time_since_epoch = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();

    //     super::Envelope {
    //         timestamp_ns: time_since_epoch.as_nanos() as u64,
    //         content_topic: topic,
    //         message: vec![65],
    //     }
    // }

    // Try a query on a test topic, and make sure we get a response
    #[tokio::test]
    async fn test_publish_query() {
        let mut client = super::create_client("0xABCD", ADDRESS, false)
            .await
            .unwrap();
        let topic = Uuid::new_v4();
        let publish_result = client
            .api_client
            .publish("".to_string(), vec![test_envelope(topic.to_string())])
            .await
            .unwrap();

        assert_eq!(publish_result, "".to_string());

        let result = client
            .api_client
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
        let mut client = super::create_client("0xABCD", ADDRESS, false)
            .api_client
            .await
            .unwrap();
        let mut sub = client
            .api_client
            .subscribe(vec![topic.to_string()])
            .await
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(100));
        let publish_result = client
            .api_client
            .publish("".to_string(), vec![test_envelope(topic.to_string())])
            .await
            .unwrap();
        assert_eq!(publish_result, "".to_string());
        std::thread::sleep(std::time::Duration::from_millis(200));

        let messages = sub.get_messages().unwrap();
        assert_eq!(messages.len(), 1);
        let messages = sub.get_messages().unwrap();
        assert_eq!(messages.len(), 0);

        sub.close();
        assert!(sub.get_messages().is_err());
    }
}
