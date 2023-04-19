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

        async fn create_client(host: String) -> Result<RustClient, String>;

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

        // async fn subscribe(&mut self, topics: Vec<String>) -> Result<Subscription, String>;

    }
}

pub struct RustClient {
    client: grpc_api_helper::Client,
}

async fn create_client(host: String) -> Result<RustClient, String> {
    let client = grpc_api_helper::Client::create(host)
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
        let info = match paging_info {
            Some(info) => Some(PagingInfo::from(info)),
            None => None,
        };

        let result = self
            .client
            .query(topic, start_time_ns, end_time_ns, info)
            .await
            .map_err(|e| format!("{}", e))?;

        return Ok(QueryResponse::from(result));
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

        return Ok(());
    }
}

pub struct RustSubscription {
    subscription: grpc_api_helper::Subscription,
}

impl RustSubscription {
    pub fn get_messages(&self) -> Result<Vec<ffi::Envelope>, String> {
        let new_messages = self.subscription.get_messages();
        // TODO: Figure out how to return an error if the stream is closed
        if new_messages.len() > 0 {
            return Ok(new_messages
                .iter()
                .map(|e| ffi::Envelope::from(e.clone()))
                .collect());
        }

        return Ok(vec![]);
    }

    pub fn close(&mut self) {
        self.subscription.close_stream();
        // Think I am going to have to do some manual memory management to ensure everything gets dropped
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

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
        let mut client = super::create_client(ADDRESS.to_string()).await.unwrap();
        let topic = uuid::Uuid::new_v4();
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

    // #[tokio::test]
    // async fn test_subscribe() {
    //     let topic = "test-subscribe-binding";
    //     // Create a subscription
    //     let result = super::subscribe(ADDRESS.to_string(), vec![topic.to_string()]).await;
    //     assert_eq!(result.error, "");
    //     let v: serde_json::Value = serde_json::from_str(&result.json).unwrap();
    //     let subscription_id = v
    //         .get("subscription_id")
    //         .unwrap()
    //         .as_str()
    //         .unwrap()
    //         .to_string();
    //     assert!(!subscription_id.is_empty());

    //     std::thread::sleep(std::time::Duration::from_millis(100));
    //     // Send a message
    //     let publish_result = super::publish(
    //         ADDRESS.to_string(),
    //         "test".to_string(),
    //         xmtp_networking::grpc_api_helper::test_envelope(topic.to_string()),
    //     )
    //     .await;
    //     assert_eq!(publish_result.error, "");

    //     std::thread::sleep(std::time::Duration::from_millis(200));
    //     // Poll the subscription
    //     let new_message_result = super::poll_subscription(subscription_id.to_string());
    //     assert_eq!(new_message_result.error, "");
    //     // Ensure messages are present
    //     let new_message_result_json: serde_json::Value =
    //         serde_json::from_str(&new_message_result.json).unwrap();
    //     let messages = new_message_result_json.get("messages").unwrap();
    //     assert_eq!(messages.as_array().unwrap().len(), 1);
    // }
}
