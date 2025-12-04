use derive_builder::Builder;
use prost::Message;
use prost::bytes::Bytes;
use std::borrow::Cow;
use xmtp_proto::api::{BodyError, Endpoint};
use xmtp_proto::types::{GlobalCursor, OriginatorId, Topic};
use xmtp_proto::xmtp::xmtpv4::message_api::SubscribeEnvelopesRequest;
use xmtp_proto::xmtp::xmtpv4::message_api::{EnvelopesQuery, SubscribeEnvelopesResponse};

/// Query a single thing
#[derive(Debug, Builder, Default, Clone)]
#[builder(build_fn(error = "BodyError"))]
pub struct SubscribeEnvelopes {
    #[builder(setter(each(name = "topic", into)))]
    topics: Vec<Topic>,
    #[builder(setter(into))]
    last_seen: Option<GlobalCursor>,
    #[builder(default)]
    originators: Vec<OriginatorId>,
}

impl SubscribeEnvelopes {
    pub fn builder() -> SubscribeEnvelopesBuilder {
        Default::default()
    }
}

impl Endpoint for SubscribeEnvelopes {
    type Output = SubscribeEnvelopesResponse;
    fn grpc_endpoint(&self) -> Cow<'static, str> {
        xmtp_proto::path_and_query::<SubscribeEnvelopesRequest>()
    }

    fn body(&self) -> Result<Bytes, BodyError> {
        for topic in &self.topics {
            tracing::info!("subscribing to {}", topic.clone());
        }
        let query = EnvelopesQuery {
            topics: self.topics.iter().map(Topic::cloned_vec).collect(),
            last_seen: self.last_seen.clone().map(Into::into),
            originator_node_ids: self.originators.clone(),
        };
        let query = SubscribeEnvelopesRequest { query: Some(query) };
        Ok(query.encode_to_vec().into())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use xmtp_api_grpc::test::XmtpdClient;
    use xmtp_proto::{api::QueryStreamExt as _, prelude::*};

    #[xmtp_common::test]
    fn test_file_descriptor() {
        use xmtp_proto::xmtp::xmtpv4::message_api::SubscribeEnvelopesRequest;
        let pnq = xmtp_proto::path_and_query::<SubscribeEnvelopesRequest>();
        println!("{}", pnq);
    }

    #[xmtp_common::test]
    fn test_grpc_endpoint_returns_correct_path() {
        let endpoint = SubscribeEnvelopes::default();
        assert_eq!(
            endpoint.grpc_endpoint(),
            "/xmtp.xmtpv4.message_api.ReplicationApi/SubscribeEnvelopes"
        );
    }

    #[xmtp_common::test]
    async fn test_subscribe_envelopes() {
        use crate::d14n::SubscribeEnvelopes;

        let client = XmtpdClient::create();
        let client = client.build().unwrap();

        let mut endpoint = SubscribeEnvelopes::builder()
            .topics(vec![])
            .last_seen(None)
            .build()
            .unwrap();
        let rsp = endpoint
            .subscribe(&client)
            .await
            .inspect_err(|e| tracing::info!("{:?}", e));
        assert!(rsp.is_ok());
    }
}
