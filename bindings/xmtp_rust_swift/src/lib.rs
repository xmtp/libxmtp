use serde_json::json;
use uuid::Uuid;
use xmtp_networking::grpc_api_helper;

pub mod subscriptions;

#[swift_bridge::bridge]
mod ffi {
    #[swift_bridge(swift_repr = "struct")]
    struct ResponseJson {
        error: String,
        json: String,
    }

    extern "Rust" {
        async fn query(host: String, topic: String, json_paging_info: String) -> ResponseJson;
        async fn publish(host: String, token: String, json_envelopes: String) -> ResponseJson;
        async fn subscribe(host: String, topics: Vec<String>) -> ResponseJson;
        async fn poll_subscription(subscription_id: String) -> ResponseJson;
    }
}

async fn query(host: String, topic: String, json_paging_info: String) -> ffi::ResponseJson {
    println!(
        "Received a request to query host: {}, topic: {}, paging info: {}",
        host, topic, json_paging_info
    );
    let query_result = grpc_api_helper::query_serialized(host, topic, json_paging_info).await;
    match query_result {
        Ok(json) => ffi::ResponseJson {
            error: "".to_string(),
            json,
        },
        Err(e) => ffi::ResponseJson {
            error: e,
            json: "".to_string(),
        },
    }
}

async fn publish(host: String, token: String, json_envelopes: String) -> ffi::ResponseJson {
    println!(
        "Received a request to publish host: {}, token: {}, envelopes: {}",
        host, token, json_envelopes
    );
    let publish_result = grpc_api_helper::publish_serialized(host, token, json_envelopes).await;
    match publish_result {
        Ok(json) => ffi::ResponseJson {
            error: "".to_string(),
            json,
        },
        Err(e) => ffi::ResponseJson {
            error: e,
            json: "".to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    static ADDRESS: &str = "http://localhost:5556";
    // Try a query on a test topic, and make sure we get a response
    #[test]
    fn test_query() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let result = runtime.block_on(super::query(
            ADDRESS.to_string(),
            "test-query".to_string(),
            "".to_string(),
        ));
        assert_eq!(result.error, "");
    }

    #[tokio::test]
    async fn test_subscribe() {
        let topic = "test-subscribe-binding";
        // Create a subscription
        let result = super::subscribe(ADDRESS.to_string(), vec![topic.to_string()]).await;
        assert_eq!(result.error, "");
        let v: serde_json::Value = serde_json::from_str(&result.json).unwrap();
        let subscription_id = v
            .get("subscription_id")
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();
        assert!(!subscription_id.is_empty());

        std::thread::sleep(std::time::Duration::from_millis(100));
        // Send a message
        let publish_result = super::publish(
            ADDRESS.to_string(),
            "test".to_string(),
            xmtp_networking::grpc_api_helper::test_envelope(topic.to_string()),
        )
        .await;
        assert_eq!(publish_result.error, "");

        std::thread::sleep(std::time::Duration::from_millis(200));
        // Poll the subscription
        let new_message_result = super::poll_subscription(subscription_id.to_string()).await;
        assert_eq!(new_message_result.error, "");
        // Ensure messages are present
        let new_message_result_json: serde_json::Value =
            serde_json::from_str(&new_message_result.json).unwrap();
        let messages = new_message_result_json.get("messages").unwrap();
        assert_eq!(messages.as_array().unwrap().len(), 1);
    }
}
