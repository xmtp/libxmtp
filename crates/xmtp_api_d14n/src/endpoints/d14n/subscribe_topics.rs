use derive_builder::Builder;
use prost::Message;
use prost::bytes::Bytes;
use std::borrow::Cow;
use xmtp_proto::api::{BodyError, Endpoint};
use xmtp_proto::types::TopicCursor;
use xmtp_proto::xmtp::xmtpv4::message_api::{
    SubscribeTopicsRequest, SubscribeTopicsResponse,
    subscribe_topics_request::TopicFilter as TopicFilterProto,
};

/// Subscribe to topics with per-topic cursors.
///
/// Subscribe to topics with per-topic cursors, replacing the old `SubscribeEnvelopes`
/// endpoint which only supported a single shared cursor across all topics.
#[derive(Debug, Builder, Default, Clone)]
#[builder(build_fn(error = "BodyError"))]
pub struct SubscribeTopics {
    #[builder(setter(into), default)]
    topics: TopicCursor,
}

impl SubscribeTopics {
    pub fn builder() -> SubscribeTopicsBuilder {
        Default::default()
    }
}

impl Endpoint for SubscribeTopics {
    type Output = SubscribeTopicsResponse;

    fn grpc_endpoint(&self) -> Cow<'static, str> {
        xmtp_proto::path_and_query::<SubscribeTopicsRequest>()
    }

    fn body(&self) -> Result<Bytes, BodyError> {
        let filters = self
            .topics
            .iter()
            .map(|(topic, cursor)| {
                tracing::info!("subscribing to {}", topic);
                TopicFilterProto {
                    topic: topic.cloned_vec(),
                    last_seen: Some(cursor.clone().into()),
                }
            })
            .collect();

        let request = SubscribeTopicsRequest { filters };
        Ok(request.encode_to_vec().into())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use xmtp_api_grpc::test::XmtpdClient;
    use xmtp_proto::types::{GlobalCursor, TopicKind};
    use xmtp_proto::{api::QueryStreamExt as _, prelude::*};

    #[xmtp_common::test]
    fn test_grpc_endpoint_returns_correct_path() {
        let endpoint = SubscribeTopics::default();
        assert_eq!(
            endpoint.grpc_endpoint(),
            "/xmtp.xmtpv4.message_api.ReplicationApi/SubscribeTopics"
        );
    }

    #[xmtp_common::test]
    fn test_body_encodes_per_topic_filters() {
        let topic = TopicKind::GroupMessagesV1.create(vec![1, 2, 3]);
        let cursor = GlobalCursor::default();

        let mut topics = TopicCursor::default();
        topics.add(topic.clone(), cursor);

        let endpoint = SubscribeTopics::builder().topics(topics).build().unwrap();

        let body = endpoint.body().unwrap();
        let decoded = SubscribeTopicsRequest::decode(body).unwrap();

        assert_eq!(decoded.filters.len(), 1);
        assert_eq!(decoded.filters[0].topic, topic.cloned_vec());
        assert!(decoded.filters[0].last_seen.is_some());
    }

    #[xmtp_common::test]
    fn test_empty_filters() {
        let endpoint = SubscribeTopics::builder().build().unwrap();

        let body = endpoint.body().unwrap();
        let decoded = SubscribeTopicsRequest::decode(body).unwrap();

        assert!(decoded.filters.is_empty());
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_subscribe_topics() {
        let client = XmtpdClient::create();
        let client = client.build().unwrap();

        let mut endpoint = SubscribeTopics::builder().build().unwrap();
        let rsp = endpoint
            .subscribe(&client)
            .await
            .inspect_err(|e| tracing::info!("{:?}", e));
        assert!(rsp.is_ok());
    }
}
