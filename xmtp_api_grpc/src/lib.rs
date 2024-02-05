pub mod grpc_api_helper;

pub const LOCALHOST_ADDRESS: &str = "http://localhost:5556";
pub const DEV_ADDRESS: &str = "https://dev.xmtp.network:5556";

pub use grpc_api_helper::Client;

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use futures::StreamExt;
    use xmtp_proto::{
        api_client::{MutableApiSubscription, XmtpApiClient, XmtpApiSubscription},
        xmtp::message_api::v1::{
            BatchQueryRequest, Envelope, PublishRequest, QueryRequest, SubscribeRequest,
        },
    };

    // Return the json serialization of an Envelope with bytes
    pub fn test_envelope(topic: String) -> Envelope {
        let time_since_epoch = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();

        Envelope {
            timestamp_ns: time_since_epoch.as_nanos() as u64,
            content_topic: topic,
            message: vec![65],
        }
    }

    #[tokio::test]
    async fn grpc_query_test() {
        let mut client = Client::create(LOCALHOST_ADDRESS.to_string(), false)
            .await
            .unwrap();

        client.set_app_version("test/0.1.0".to_string());

        let result = client
            .query(QueryRequest {
                content_topics: vec!["test-query".to_string()],
                ..QueryRequest::default()
            })
            .await
            .unwrap();

        assert_eq!(result.envelopes.len(), 0);
    }

    #[tokio::test]
    async fn grpc_batch_query_test() {
        let client = Client::create(LOCALHOST_ADDRESS.to_string(), false)
            .await
            .unwrap();
        let req = BatchQueryRequest { requests: vec![] };
        let result = client.batch_query(req).await.unwrap();
        assert_eq!(result.responses.len(), 0);
    }

    #[tokio::test]
    async fn publish_test() {
        let client = Client::create(LOCALHOST_ADDRESS.to_string(), false)
            .await
            .unwrap();

        let topic = uuid::Uuid::new_v4();
        let env = test_envelope(topic.to_string());

        let _result = client
            .publish(
                "".to_string(),
                PublishRequest {
                    envelopes: vec![env],
                },
            )
            .await
            .unwrap();

        let query_result = client
            .query(QueryRequest {
                content_topics: vec![topic.to_string()],
                ..QueryRequest::default()
            })
            .await
            .unwrap();
        assert_eq!(query_result.envelopes.len(), 1);
    }

    #[tokio::test]
    async fn subscribe_test() {
        tokio::time::timeout(std::time::Duration::from_secs(5), async move {
            let client = Client::create(LOCALHOST_ADDRESS.to_string(), false)
                .await
                .unwrap();

            let topic = uuid::Uuid::new_v4();
            let mut stream_handler = client
                .subscribe(SubscribeRequest {
                    content_topics: vec![topic.to_string()],
                })
                .await
                .unwrap();

            assert!(!stream_handler.is_closed());
            // Skipping the auth token because we have authn disabled on the local
            // xmtp-node-go instance
            client
                .publish(
                    "".to_string(),
                    PublishRequest {
                        envelopes: vec![test_envelope(topic.to_string())],
                    },
                )
                .await
                .unwrap();

            // Sleep to give the response time to come back
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            // Ensure that messages appear
            let results = stream_handler.get_messages();
            println!("{}", results.len());
            assert!(results.len() == 1);

            // Ensure that the messages array has been cleared
            let second_results = stream_handler.get_messages();
            assert!(second_results.is_empty());

            // Ensure the is_closed status is propagated
            stream_handler.close_stream();
            assert!(stream_handler.is_closed());
        })
        .await
        .expect("Timed out");
    }

    #[tokio::test]
    async fn tls_test() {
        let client = Client::create(DEV_ADDRESS.to_string(), true).await.unwrap();

        let result = client
            .query(QueryRequest {
                content_topics: vec![uuid::Uuid::new_v4().to_string()],
                ..QueryRequest::default()
            })
            .await
            .unwrap();

        assert_eq!(result.envelopes.len(), 0);
    }

    #[tokio::test]
    async fn bidrectional_streaming_test() {
        let client = Client::create(LOCALHOST_ADDRESS.to_string(), false)
            .await
            .unwrap();

        let topic = uuid::Uuid::new_v4();
        let mut stream = client
            .subscribe2(SubscribeRequest {
                content_topics: vec![topic.to_string()],
            })
            .await
            .unwrap();

        // Publish an envelope to the topic of the stream
        let env = test_envelope(topic.to_string());
        client
            .publish(
                "".to_string(),
                PublishRequest {
                    envelopes: vec![env],
                },
            )
            .await
            .unwrap();

        let value = stream.next().await.unwrap().unwrap();
        assert_eq!(value.content_topic, topic.to_string());

        // Change the topic of the stream to something else
        let topic_2 = uuid::Uuid::new_v4();
        stream
            .update(SubscribeRequest {
                content_topics: vec![topic_2.to_string()],
            })
            .await
            .unwrap();

        // Sleep 100ms to ensure subscription is updated
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        // Publish an envelope to the new topic
        let env_2 = test_envelope(topic_2.to_string());
        client
            .publish(
                "".to_string(),
                PublishRequest {
                    envelopes: vec![env_2],
                },
            )
            .await
            .unwrap();

        let value_2 = stream.next().await.unwrap().unwrap();
        assert_eq!(value_2.content_topic, topic_2.to_string());
    }
}
