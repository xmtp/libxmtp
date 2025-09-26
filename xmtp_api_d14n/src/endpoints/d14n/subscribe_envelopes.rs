use derive_builder::Builder;
use prost::Message;
use prost::bytes::Bytes;
use std::borrow::Cow;
use xmtp_proto::api::{BodyError, Endpoint};
use xmtp_proto::xmtp::xmtpv4::message_api::EnvelopesQuery;
use xmtp_proto::xmtp::xmtpv4::message_api::{
    SubscribeEnvelopesRequest, SubscribeEnvelopesResponse,
};

/// Query a single thing
#[derive(Debug, Builder, Default, Clone)]
#[builder(build_fn(error = "BodyError"))]
pub struct SubscribeEnvelopes {
    envelopes: EnvelopesQuery,
}

impl SubscribeEnvelopes {
    pub fn builder() -> SubscribeEnvelopesBuilder {
        Default::default()
    }
}

impl Endpoint for SubscribeEnvelopes {
    type Output = SubscribeEnvelopesResponse;

    fn http_endpoint(&self) -> Cow<'static, str> {
        Cow::from("/mls/v2/subscribe-envelopes")
    }

    fn grpc_endpoint(&self) -> Cow<'static, str> {
        xmtp_proto::path_and_query::<SubscribeEnvelopesRequest>()
    }

    fn body(&self) -> Result<Bytes, BodyError> {
        let query = SubscribeEnvelopesRequest {
            query: Some(self.envelopes.clone()),
        };
        tracing::debug!("{:?}", query);
        Ok(query.encode_to_vec().into())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use xmtp_proto::prelude::*;

    #[xmtp_common::test]
    fn test_file_descriptor() {
        use xmtp_proto::xmtp::xmtpv4::message_api::SubscribeEnvelopesRequest;
        let pnq = xmtp_proto::path_and_query::<SubscribeEnvelopesRequest>();
        println!("{}", pnq);
    }

    #[xmtp_common::test]
    async fn test_subscribe_envelopes() {
        use crate::d14n::SubscribeEnvelopes;

        let client = crate::TestClient::create_local();
        let client = client.build().await.unwrap();

        let mut endpoint = SubscribeEnvelopes::builder()
            .envelopes(EnvelopesQuery {
                topics: vec![vec![]],
                originator_node_ids: vec![],
                last_seen: None,
            })
            .build()
            .unwrap();
        assert!(endpoint.stream(&client).await.is_ok());
    }
}
