use derive_builder::Builder;
use prost::Message;
use prost::bytes::Bytes;
use std::borrow::Cow;
use xmtp_proto::api::{BodyError, Endpoint};
use xmtp_proto::xmtp::xmtpv4::message_api::SubscribeEnvelopesRequest;
use xmtp_proto::xmtp::xmtpv4::message_api::{EnvelopesQuery, SubscribeEnvelopesResponse};

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
    use xmtp_proto::{
        api::QueryStreamExt as _, prelude::*, xmtp::xmtpv4::message_api::SubscribeEnvelopesResponse,
    };

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

        let client = crate::TestClient::create_d14n();
        let client = client.build().unwrap();

        let mut endpoint = SubscribeEnvelopes::builder()
            .envelopes(EnvelopesQuery {
                topics: vec![vec![]],
                originator_node_ids: vec![],
                last_seen: None,
            })
            .build()
            .unwrap();
        let rsp = endpoint
            .subscribe::<SubscribeEnvelopesResponse>(&client)
            .await
            .inspect_err(|e| tracing::info!("{:?}", e));
        assert!(rsp.is_ok());
    }
}
