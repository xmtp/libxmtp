pub mod grpc_api_helper;
pub mod proto_helper;

#[cfg(test)]
mod tests {
    use super::*;
    use grpc_api_helper::query_serialized;
    use grpc_api_helper::test_envelope;

    #[test]
    fn test() {
        let serialized = test_envelope();
        assert_eq!(
            serialized,
            "{\"content_topic\":\"\",\"timestamp_ns\":0,\"message\":[QQ==]}"
        );
    }

    #[test]
    fn grpc_query_test() {
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
}
