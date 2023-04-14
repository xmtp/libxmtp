pub mod grpc_api_helper;
pub mod proto_helper;

#[cfg(test)]
mod tests {
    use super::*;
    use grpc_api_helper::{query};

    #[test]
    fn grpc_query_test() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let resp = query(
                "http://localhost:15555".to_string(),
                "test".to_string(),
                None,
            )
            .await;
            println!("{:?}", resp);
            assert!(resp.is_ok());
            // Check that the response has some messages
            assert!(resp.unwrap().envelopes.len() == 0);
        });
    }
}
