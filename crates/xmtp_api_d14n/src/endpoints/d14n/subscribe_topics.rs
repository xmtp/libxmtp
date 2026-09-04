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
        Cow::Borrowed("/xmtp.xmtpv4.message_api.ReplicationApi/SubscribeTopics")
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
