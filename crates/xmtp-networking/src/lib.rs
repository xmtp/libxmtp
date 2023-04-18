pub mod grpc_api_helper;

// Custom patching of protobuf serialization for bytes -> base64
// https://github.com/tokio-rs/prost/issues/75#issuecomment-1383233271
pub mod serialize_utils;

#[cfg(test)]
mod tests {
    use super::*;
    use grpc_api_helper::test_envelope;
    use grpc_api_helper::{publish, query_serialized, subscribe_stream};
    use tracing::Level;
    use tracing_subscriber;

    #[test]
    fn test() {
        let serialized = test_envelope();
        assert_eq!(
            serialized,
            // NOTE: I removed the empty content_topic and timestamp_ns fields, since
            // the serializer I am using doesn't include them
            "{\"message\":\"QQ==\"}"
        );
    }

    #[tokio::test]
    async fn grpc_query_test() {
        let resp = query_serialized(
            "http://localhost:5556".to_string(),
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
    }

    #[tokio::test]
    async fn subscribe_test() {
        // Enable debug logging
        tracing_subscriber::fmt()
            .with_max_level(Level::DEBUG) // Change this to the desired log level
            .init();

        tokio::time::timeout(std::time::Duration::from_secs(5), async move {
            let host = "http://localhost:5556";
            let topic = "test";
            let mut stream_handler = subscribe_stream(host.to_string(), vec![topic.to_string()])
                .await
                .unwrap();
            println!("Got stream");
            std::thread::sleep(std::time::Duration::from_millis(100));
            publish(host.to_string(), topic.to_string(), test_envelope())
                .await
                .unwrap();

            println!("Got results");
            let results = stream_handler.get_and_reset_pending();
            assert!(results.len() == 2);
            stream_handler.close_stream();
        })
        .await
        .expect("Timed out");
    }
}
