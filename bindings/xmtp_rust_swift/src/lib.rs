use types::QueryResponse;
use xmtp_networking::grpc_api_helper;
use xmtp_proto::xmtp::message_api::v1::{Envelope, PagingInfo};
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
    #[swift_bridge(swift_repr = "struct")]
    struct Envelope {
        content_topic: String,
        timestamp_ns: u64,
        message: Vec<u8>,
    }

    extern "Rust" {
        type RustSubscription;

        fn get_messages(&self) -> Result<Vec<Envelope>, String>;

        fn close(&mut self);
    }

    extern "Rust" {
        type QueryResponse;

        fn envelopes(self) -> Vec<Envelope>;
        fn paging_info(self) -> Option<PagingInfo>;

    }

    extern "Rust" {
        type RustClient;

        async fn create_client(host: String, is_secure: bool) -> Result<RustClient, String>;

        async fn query(
            &mut self,
            topic: String,
            start_time_ns: Option<u64>,
            end_time_ns: Option<u64>,
            paging_info: Option<PagingInfo>,
        ) -> Result<QueryResponse, String>;

        async fn publish(
            self: &mut RustClient,
            token: String,
            envelopes: Vec<Envelope>,
        ) -> Result<(), String>;

        async fn subscribe(&mut self, topics: Vec<String>) -> Result<RustSubscription, String>;
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

    async fn publish(
        &mut self,
        token: String,
        envelopes: Vec<ffi::Envelope>,
    ) -> Result<(), String> {
        let mut xmtp_envelopes = vec![];
        for envelope in envelopes {
            xmtp_envelopes.push(Envelope::from(envelope));
        }

        self.client
            .publish(token, xmtp_envelopes)
            .await
            .map_err(|e| format!("{}", e))?;

        Ok(())
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
    pub fn get_messages(&self) -> Result<Vec<ffi::Envelope>, String> {
        let new_messages = self.subscription.get_messages();
        // Return the last envelopes even if the stream is closed
        if !new_messages.is_empty() {
            return Ok(new_messages
                .iter()
                .map(|e| ffi::Envelope::from(e.clone()))
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

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};
    use uuid::Uuid;

    static ADDRESS: &str = "http://localhost:5556";

    pub fn test_envelope(topic: String) -> super::ffi::Envelope {
        let time_since_epoch = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();

        return super::ffi::Envelope {
            timestamp_ns: time_since_epoch.as_nanos() as u64,
            content_topic: topic.to_string(),
            message: vec![65],
        };
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

        assert_eq!(publish_result, ());

        let result = client
            .query(topic.to_string(), None, None, None)
            .await
            .unwrap();

        let envelopes = result.envelopes();
        assert_eq!(envelopes.len(), 1);

        let first_envelope = envelopes.get(0).unwrap();
        assert_eq!(first_envelope.content_topic, topic.to_string());
        assert!(first_envelope.timestamp_ns > 0);
        assert!(first_envelope.message.len() > 0);
    }

    #[tokio::test]
    async fn test_subscribe() {
        let topic = Uuid::new_v4();
        let mut client = super::create_client(ADDRESS.to_string(), false)
            .await
            .unwrap();
        let mut sub = client.subscribe(vec![topic.to_string()]).await.unwrap();
        std::thread::sleep(std::time::Duration::from_millis(100));
        let publish_result = client
            .publish("".to_string(), vec![test_envelope(topic.to_string())])
            .await
            .unwrap();
        assert_eq!(publish_result, ());
        std::thread::sleep(std::time::Duration::from_millis(200));

        let messages = sub.get_messages().unwrap();
        assert_eq!(messages.len(), 1);
        let messages = sub.get_messages().unwrap();
        assert_eq!(messages.len(), 0);

        sub.close();
        assert_eq!(sub.get_messages().is_err(), true);
    }
}
