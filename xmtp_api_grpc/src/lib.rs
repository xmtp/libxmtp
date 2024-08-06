pub mod auth_token;
pub mod grpc_api_helper;
mod identity;

pub const LOCALHOST_ADDRESS: &str = "http://localhost:5556";
pub const DEV_ADDRESS: &str = "https://grpc.dev.xmtp.network:443";

pub use grpc_api_helper::Client;

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use self::auth_token::Authenticator;

    use super::*;
    use futures::StreamExt;
    use xmtp_proto::{
        api_client::{MutableApiSubscription, XmtpApiClient, XmtpApiSubscription},
        xmtp::message_api::v1::{
            BatchQueryRequest, Envelope, PublishRequest, QueryRequest, SubscribeRequest,
        },
    };

    const PRIVATE_KEY_BUNDLE_HEX: &str = "0a88030ac20108eec0888ae33112220a201cd19d1d6e129cb8f8ba4bd85aae10ffcc97a3de939d85f9bc378d47e6ba83711a940108eec0888ae33112460a440a40130cfb1cd667f48585f90372fe4b529da318e83221a3bfd1446ef6cf00d173543fed831d1517d310b05bd5ab138fde22af50a3ffce1aa72da8c7084e9bab0e4910011a430a4104c4eb77c3b2eaacaca12e2b55c6c42dc33f4518a5690bb49cd6ae0e0a652e59fbc9defd98242d30a0737a13c3461cac1edc0f8e3007d65b1637382088ac1cd3d712c00108a4c1888ae33112220a2062e553bceac5247e7bebfdcc8c31959965603e442f79c6346028060ab2129e931a920108a4c1888ae33112440a420a40d12c6ab6eb1874edd3044fdc753543516130bd4d1db11024bd81cd9c2c4bb6b6138e85ed313f387ea7707e09090659b580ee22f42f022c4521e4a11ab7abddfc1a430a4104175097c31bbe1700729f1f1ede87b8bd21a5bc62e4bb4c963e0de885080048bd31138b657fd9146aa8255f1c57c4fa1f8cb7b30bed8803eed48d6a3e67e71ccf";
    const WALLET_ADDRESS: &str = "0xA38A1f04B29dea1de621E17447fB4efB11BFfBdf";

    // Return the json serialization of an Envelope with bytes
    pub fn test_envelope(topic: String) -> Envelope {
        let time_since_epoch = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();

        Envelope {
            timestamp_ns: time_since_epoch.as_nanos() as u64,
            content_topic: topic,
            message: vec![65],
        }
    }

    fn get_auth_token() -> String {
        // This is a private key bundle exported from the JS SDK and hex encoded
        let authenticator = Authenticator::from_hex(
            PRIVATE_KEY_BUNDLE_HEX.to_string(),
            WALLET_ADDRESS.to_string(),
        );

        authenticator.create_token()
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
        let req = BatchQueryRequest {
            requests: vec![QueryRequest {
                content_topics: vec!["some-random-topic-with-no-messages".to_string()],
                ..QueryRequest::default()
            }],
        };
        let result = client.batch_query(req).await.unwrap();
        assert_eq!(result.responses.len(), 1);
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
    #[cfg_attr(feature = "http-api", ignore)]
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

    #[tokio::test]
    #[cfg_attr(feature = "http-api", ignore)]
    async fn test_dev_publish() {
        let auth_token = get_auth_token();
        let dev_client = Client::create(DEV_ADDRESS.to_string(), true).await.unwrap();
        dev_client
            .publish(
                auth_token,
                PublishRequest {
                    envelopes: vec![Envelope {
                        content_topic: "/xmtp/0/foo/2".to_string(),
                        timestamp_ns: 3,
                        message: vec![1, 2, 3],
                    }],
                },
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    #[cfg_attr(feature = "http-api", ignore)]
    async fn long_lived_subscribe_test() {
        let auth_token = get_auth_token();
        tokio::time::timeout(std::time::Duration::from_secs(100), async move {
            let client = Client::create(DEV_ADDRESS.to_string(), true).await.unwrap();

            let topic = uuid::Uuid::new_v4();
            let mut subscription = client
                .subscribe2(SubscribeRequest {
                    content_topics: vec![topic.to_string()],
                })
                .await
                .unwrap();

            client
                .publish(
                    auth_token.to_string(),
                    PublishRequest {
                        envelopes: vec![test_envelope(topic.to_string())],
                    },
                )
                .await
                .unwrap();

            // Sleep to give the response time to come back
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            let mut next_message = subscription.next().await.unwrap();
            if let Err(err) = next_message {
                panic!("Message 1 Error: {}", err);
            }

            tokio::time::sleep(std::time::Duration::from_secs(50)).await;
            client
                .publish(
                    auth_token.to_string(),
                    PublishRequest {
                        envelopes: vec![test_envelope(topic.to_string())],
                    },
                )
                .await
                .unwrap();

            // Sleep to give the response time to come back
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            next_message = subscription.next().await.unwrap();
            if let Err(err) = next_message {
                panic!("Message 2 Error: {}", err);
            }
        })
        .await
        .expect("Timed out");
    }
}
