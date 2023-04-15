pub mod grpc_api_helper;

// Custom patching of protobuf serialization for bytes -> base64
// https://github.com/tokio-rs/prost/issues/75#issuecomment-1383233271
pub mod serialize_utils;

#[cfg(test)]
mod tests {
    use super::*;
    use grpc_api_helper::test_envelope;
    use grpc_api_helper::{publish, publish_serialized, query_serialized, subscribe};

    #[tokio::test]
    async fn grpc_query_test() {
        let resp = query_serialized(
            "http://localhost:5556".to_string(),
            "test".to_string(),
            "".to_string(),
        )
        .await;
        assert!(resp.is_ok());
        // Check that the response has some messages
        // Assert response is a string that isn't empty and starts with a { like JSON
        let resp_str = resp.unwrap();
        assert!(!resp_str.is_empty());
        assert!(resp_str.starts_with('{'));
    }

    #[tokio::test]
    async fn subscribe_test() {
        tokio::time::timeout(std::time::Duration::from_secs(5), async move {
            let host = "http://localhost:5556";
            let topic = "test-subscribe";
            let mut stream_handler = subscribe(host.to_string(), vec![topic.to_string()])
                .await
                .unwrap();

            // Skipping the auth token because we have authn disabled on the local
            // xmtp-node-go instance
            publish_serialized(
                host.to_string(),
                "".to_string(),
                test_envelope(String::from(topic)),
            )
            .await
            .unwrap();
            // Sleep to give the response time to come back
            std::thread::sleep(std::time::Duration::from_millis(100));

            let results = stream_handler.get_and_reset_pending();
            println!("{}", results.len());
            assert!(results.len() == 1);
            stream_handler.close_stream();
        })
        .await
        .expect("Timed out");
    }

    #[test]
    fn grpc_query_local_test() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let resp = query_serialized(
                "http://localhost:15555".to_string(),
                "test".to_string(),
                "".to_string(),
            )
            .await;
            println!("{:?}", resp);
            assert!(resp.is_ok());
            // Check that the response has some messages
            // Assert response is a string that isn't empty and starts with a { like JSON
            let resp_str = resp.unwrap();
            assert!(!resp_str.is_empty());
            assert!(resp_str.starts_with('{'));
        });
    }

    #[test]
    fn test_publish_simple() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let resp = publish_serialized(
                "https://dev.xmtp.network:5556".to_string(),
                "token".to_string(),
                "[]".to_string(),
            )
            .await;
            println!("{:?}", resp);
            assert!(resp.is_ok());
        });
    }
}
