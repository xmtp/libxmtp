use derive_builder::Builder;
use prost::Message;
use prost::bytes::Bytes;
use std::borrow::Cow;
use xmtp_proto::api::{BodyError, Endpoint};
use xmtp_proto::types::{GlobalCursor, Topic};
use xmtp_proto::xmtp::xmtpv4::message_api::QueryEnvelopesRequest;
use xmtp_proto::xmtp::xmtpv4::message_api::{EnvelopesQuery, QueryEnvelopesResponse};

/// Query a single thing
#[derive(Debug, Builder, Default, Clone)]
#[builder(build_fn(error = "BodyError"))]
pub struct QueryEnvelope {
    #[builder(setter(each(name = "topic", into)))]
    topics: Vec<Topic>,
    last_seen: GlobalCursor,
    limit: u32,
}

impl QueryEnvelope {
    pub fn builder() -> QueryEnvelopeBuilder {
        Default::default()
    }
}

impl Endpoint for QueryEnvelope {
    type Output = QueryEnvelopesResponse;
    fn grpc_endpoint(&self) -> Cow<'static, str> {
        xmtp_proto::path_and_query::<QueryEnvelopesRequest>()
    }

    fn body(&self) -> Result<Bytes, BodyError> {
        let query = QueryEnvelopesRequest {
            query: Some(EnvelopesQuery {
                topics: self.topics.iter().map(Topic::bytes).collect(),
                originator_node_ids: vec![],
                last_seen: Some(self.last_seen.clone().into()),
            }),
            limit: self.limit,
        };
        Ok(query.encode_to_vec().into())
    }
}

/// Batch Query
#[derive(Debug, Builder, Default)]
#[builder(setter(strip_option), build_fn(error = "BodyError"))]
pub struct QueryEnvelopes {
    #[builder(setter(into))]
    envelopes: EnvelopesQuery,
    #[builder(setter(into), default)]
    limit: u32,
}

impl QueryEnvelopes {
    pub fn builder() -> QueryEnvelopesBuilder {
        Default::default()
    }
}

impl Endpoint for QueryEnvelopes {
    type Output = QueryEnvelopesResponse;
    fn grpc_endpoint(&self) -> Cow<'static, str> {
        xmtp_proto::path_and_query::<QueryEnvelopesRequest>()
    }

    fn body(&self) -> Result<Bytes, BodyError> {
        Ok(QueryEnvelopesRequest {
            query: Some(self.envelopes.clone()),
            limit: self.limit,
        }
        .encode_to_vec()
        .into())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use xmtp_api_grpc::{error::GrpcError, test::XmtpdClient};
    use xmtp_proto::{api, prelude::*, types::TopicKind};

    #[xmtp_common::test]
    fn test_file_descriptor() {
        use xmtp_proto::xmtp::xmtpv4::message_api::QueryEnvelopesRequest;
        let pnq = xmtp_proto::path_and_query::<QueryEnvelopesRequest>();
        println!("{}", pnq);
    }

    #[xmtp_common::test]
    fn test_grpc_endpoint_returns_correct_path() {
        let endpoint = QueryEnvelopes::default();
        assert_eq!(
            endpoint.grpc_endpoint(),
            "/xmtp.xmtpv4.message_api.ReplicationApi/QueryEnvelopes"
        );
    }

    #[xmtp_common::test]
    fn test_query_envelope_grpc_endpoint_returns_correct_path() {
        let endpoint = QueryEnvelope::default();
        assert_eq!(
            endpoint.grpc_endpoint(),
            "/xmtp.xmtpv4.message_api.ReplicationApi/QueryEnvelopes"
        );
    }

    #[xmtp_common::test]
    async fn test_query_envelopes() {
        use crate::d14n::QueryEnvelopes;

        let client = XmtpdClient::create();
        let client = client.build().unwrap();

        let endpoint = QueryEnvelopes::builder()
            .envelopes(EnvelopesQuery {
                topics: vec![vec![]],
                originator_node_ids: vec![],
                last_seen: None,
            })
            .build()
            .unwrap();
        let err = api::ignore(endpoint).query(&client).await.unwrap_err();
        tracing::info!("{}", err);
        // the request will fail b/c we're using dummy data but
        // we just care if the endpoint is working
        match err {
            ApiClientError::<GrpcError>::ClientWithEndpoint {
                source: GrpcError::Status(ref s),
                ..
            } => assert!(s.message().contains("invalid topic"), "{}", err),
            _ => panic!("request failed"),
        }
    }

    #[xmtp_common::test]
    async fn test_query_envelope() {
        use crate::d14n::QueryEnvelope;

        let client = XmtpdClient::create();
        let client = client.build().unwrap();

        let endpoint = QueryEnvelope::builder()
            .last_seen(Default::default())
            .topic(TopicKind::GroupMessagesV1.create(vec![]))
            .limit(0)
            .build()
            .unwrap();
        api::ignore(endpoint).query(&client).await.unwrap();
    }
}
