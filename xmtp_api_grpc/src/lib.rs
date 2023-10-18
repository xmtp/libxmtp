pub mod grpc_api_helper;

pub const LOCALHOST_ADDRESS: &str = "http://localhost:5556";
pub const DEV_ADDRESS: &str = "https://dev.xmtp.network:5556";

pub use grpc_api_helper::XmtpGrpcClient;

#[cfg(test)]
mod tests {
    use futures_util::{pin_mut, StreamExt};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    use tokio::sync::oneshot;
    use tokio::time::timeout;

    use super::*;
    use xmtp_proto::api_client::XmtpApiClient;
    use xmtp_proto::xmtp::message_api::v1::{
        BatchQueryRequest, Envelope, PublishRequest, QueryRequest, SubscribeRequest,
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
        let mut client = XmtpGrpcClient::create(LOCALHOST_ADDRESS.to_string(), false)
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
        let client = XmtpGrpcClient::create(LOCALHOST_ADDRESS.to_string(), false)
            .await
            .unwrap();
        let req = BatchQueryRequest { requests: vec![] };
        let result = client.batch_query(req).await.unwrap();
        assert_eq!(result.responses.len(), 0);
    }

    #[tokio::test]
    async fn publish_test() {
        let client = XmtpGrpcClient::create(LOCALHOST_ADDRESS.to_string(), false)
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

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn subscribe_test() {
        // Subscribes to a topic (and awaits a message in a background task).
        // Publishes to that topic (and awaits the background task to confirm receipt).
        let client = XmtpGrpcClient::create(LOCALHOST_ADDRESS.to_string(), false)
            .await
            .unwrap();
        let topic = uuid::Uuid::new_v4();
        let stream = client
            .subscribe(SubscribeRequest {
                content_topics: vec![topic.to_string()],
            })
            .await
            .expect("subscribed");

        // This is how the subscription task tells the foreground task that it got a message.
        let (tx, rx) = oneshot::channel();

        tokio::task::spawn(async move {
            pin_mut!(stream);
            let env = stream.next().await.expect("received message");
            tx.send(env).expect("notified that we got the message");
        });

        client
            .publish(
                "".to_string(),
                PublishRequest {
                    envelopes: vec![test_envelope(topic.to_string())],
                },
            )
            .await
            .expect("published");

        if let Ok(Ok(env)) = timeout(Duration::from_secs(5), rx).await {
            assert_eq!(env.content_topic, topic.to_string());
        } else {
            panic!("timed out without receiving a message");
        }
    }

    #[tokio::test]
    async fn tls_test() {
        let client = XmtpGrpcClient::create(DEV_ADDRESS.to_string(), true)
            .await
            .unwrap();

        let result = client
            .query(QueryRequest {
                content_topics: vec![uuid::Uuid::new_v4().to_string()],
                ..QueryRequest::default()
            })
            .await
            .unwrap();

        assert_eq!(result.envelopes.len(), 0);
    }
}
