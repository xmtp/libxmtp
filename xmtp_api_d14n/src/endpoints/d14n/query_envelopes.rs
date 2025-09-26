use derive_builder::Builder;
use prost::Message;
use prost::bytes::Bytes;
use std::borrow::Cow;
use std::collections::HashMap;
use xmtp_proto::api::{BodyError, Endpoint};
use xmtp_proto::xmtp::xmtpv4::envelopes::Cursor;
use xmtp_proto::xmtp::xmtpv4::message_api::QueryEnvelopesRequest;
use xmtp_proto::xmtp::xmtpv4::message_api::{EnvelopesQuery, QueryEnvelopesResponse};

/// Query a single thing
#[derive(Debug, Builder, Default, Clone)]
#[builder(build_fn(error = "BodyError"))]
pub struct QueryEnvelope {
    #[builder(setter(each(name = "topic", into)))]
    topics: Vec<Vec<u8>>,
    last_seen: Vec<xmtp_proto::types::Cursor>,
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
        let last_seen = self
            .last_seen
            .iter()
            .map(|info| (info.originator_id, info.sequence_id))
            .collect::<HashMap<_, _>>();
        let cursor = Cursor {
            node_id_to_sequence_id: last_seen,
        };

        let query = QueryEnvelopesRequest {
            query: Some(EnvelopesQuery {
                topics: self.topics.clone(),
                originator_node_ids: vec![],
                last_seen: Some(cursor),
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
    use xmtp_api_grpc::error::GrpcError;
    use xmtp_proto::{api, prelude::*};

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

        let client = crate::TestGrpcClient::create_d14n();
        let client = client.build().await.unwrap();

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

        let client = crate::TestGrpcClient::create_d14n();
        let client = client.build().await.unwrap();

        let endpoint = QueryEnvelope::builder()
            .last_seen(vec![])
            .topic(vec![])
            .limit(0)
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
}
