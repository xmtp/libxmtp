use async_trait::async_trait;
use xmtp::{
    account::Account, networking::XmtpApiClient,
    persistence::in_memory_persistence::InMemoryPersistence,
};
use xmtp_networking::grpc_api_helper::{self, Subscription};
use xmtp_proto::xmtp::message_api::v1::{Envelope, PagingInfo, PublishResponse, QueryResponse};

pub type FfiXmtpClient = xmtp::Client<FfiApiClient, InMemoryPersistence>;

#[swift_bridge::bridge]
mod ffi {
    extern "Rust" {
        type Account;
        type FfiXmtpClient;

        async fn create_client(
            account: Account,
            host: &str,
            is_secure: bool,
        ) -> Result<FfiXmtpClient, String>;
    }
}

async fn create_client(
    account: Account,
    host: &str,
    is_secure: bool,
    // TODO proper error handling
) -> Result<xmtp::Client<FfiApiClient, InMemoryPersistence>, String> {
    let api_client = FfiApiClient::new(host, is_secure).await?;
    let persistence = InMemoryPersistence::new();

    let xmtp_client = xmtp::ClientBuilder::new()
        .wallet_address(&account.addr())
        .account(account)
        .api_client(api_client)
        .persistence(persistence)
        .build()
        .map_err(|e| format!("{:?}", e))?;

    Ok(xmtp_client)
}

pub struct FfiApiClient {
    client: grpc_api_helper::Client,
}

impl FfiApiClient {
    async fn new(host: &str, is_secure: bool) -> Result<Self, String> {
        let client = grpc_api_helper::Client::create(host.to_string(), is_secure)
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
    ) -> Result<PublishResponse, String> {
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
    ) -> Result<QueryResponse, String> {
        self.client
            .query(topic, start_time, end_time, paging_info)
            .await
            .map_err(|e| format!("{}", e))
    }

    async fn subscribe(&mut self, topics: Vec<String>) -> Result<Subscription, String> {
        self.client
            .subscribe(topics)
            .await
            .map_err(|e| format!("{}", e))
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use ethers::signers::{LocalWallet, Signer};
    use uuid::Uuid;
    use xmtp::{
        account::{Account, AccountCreator},
        networking::XmtpApiClient,
    };
    use xmtp_cryptography::{signature::h160addr_to_string, utils::rng};

    static ADDRESS: &str = "http://localhost:5556";

    fn test_envelope(topic: String) -> super::Envelope {
        let time_since_epoch = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();

        super::Envelope {
            timestamp_ns: time_since_epoch.as_nanos() as u64,
            content_topic: topic,
            message: vec![65],
        }
    }

    async fn gen_test_account() -> Account {
        let wallet = LocalWallet::new(&mut rng());
        let addr = h160addr_to_string(wallet.address());

        let ac = AccountCreator::new(addr);
        let msg = ac.text_to_sign();
        let sig = wallet
            .sign_message(msg)
            .await
            .expect("Bad Signature in test");
        ac.finalize(sig.to_vec()).unwrap()
    }

    // Try a query on a test topic, and make sure we get a response
    #[tokio::test]
    async fn test_publish_query() {
        let account = gen_test_account().await;
        let mut client = super::create_client(account, ADDRESS, false).await.unwrap();
        let topic = Uuid::new_v4();
        client
            .api_client
            .publish("".to_string(), vec![test_envelope(topic.to_string())])
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

    #[tokio::test]
    async fn test_subscribe() {
        let account = gen_test_account().await;
        let topic = Uuid::new_v4();
        let mut client = super::create_client(account, ADDRESS, false).await.unwrap();
        let mut sub = client
            .api_client
            .subscribe(vec![topic.to_string()])
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        client
            .api_client
            .publish("".to_string(), vec![test_envelope(topic.to_string())])
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let messages = sub.get_messages();
        assert_eq!(messages.len(), 1);
        let messages = sub.get_messages();
        assert_eq!(messages.len(), 0);

        sub.close_stream();
        assert!(sub.is_closed());
    }
}
