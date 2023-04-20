pub mod grpc_api_helper;

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use grpc_api_helper::Client;
    use xmtp_proto::xmtp::message_api::v1::Envelope;

    // Return the json serialization of an Envelope with bytes
    pub fn test_envelope(topic: String) -> Envelope {
        let time_since_epoch = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();

        return Envelope {
            timestamp_ns: time_since_epoch.as_nanos() as u64,
            content_topic: topic.to_string(),
            message: vec![65],
            ..Default::default()
        };
    }

    #[tokio::test]
    async fn grpc_query_test() {
        let mut client = Client::create("http://localhost:5556".to_string(), false)
            .await
            .unwrap();

        let result = client
            .query("test-query".to_string(), None, None, None)
            .await
            .unwrap();

        assert_eq!(result.envelopes.len(), 0);
    }

    #[tokio::test]
    async fn publish_test() {
        let mut client = Client::create("http://localhost:5556".to_string(), false)
            .await
            .unwrap();

        let topic = uuid::Uuid::new_v4();
        let env = test_envelope(topic.to_string());

        let _result = client.publish("".to_string(), vec![env]).await.unwrap();

        let query_result = client
            .query(topic.to_string(), None, None, None)
            .await
            .unwrap();
        assert_eq!(query_result.envelopes.len(), 1);
    }

    #[tokio::test]
    async fn subscribe_test() {
        tokio::time::timeout(std::time::Duration::from_secs(5), async move {
            let mut client = Client::create("http://localhost:5556".to_string(), false)
                .await
                .unwrap();

            let topic = "test-subscribe";
            let mut stream_handler = client.subscribe(vec![topic.to_string()]).await.unwrap();

            // Skipping the auth token because we have authn disabled on the local
            // xmtp-node-go instance
            client
                .publish("".to_string(), vec![test_envelope(topic.to_string())])
                .await
                .unwrap();

            // Sleep to give the response time to come back
            std::thread::sleep(std::time::Duration::from_millis(100));

            let results = stream_handler.get_messages();
            println!("{}", results.len());
            assert!(results.len() == 1);

            let second_results = stream_handler.get_messages();
            assert!(second_results.len() == 0);

            stream_handler.close_stream();
        })
        .await
        .expect("Timed out");
    }

    #[tokio::test]
    async fn tls_test() {
        let mut client = Client::create("https://dev.xmtp.network:5556".to_string(), true)
            .await
            .unwrap();

        let result = client
            .query(uuid::Uuid::new_v4().to_string(), None, None, None)
            .await
            .unwrap();

        assert_eq!(result.envelopes.len(), 0);
    }
}
