use derive_builder::Builder;
use prost::Message;
use prost::bytes::Bytes;
use std::borrow::Cow;
use xmtp_proto::api::{BodyError, Endpoint};
use xmtp_proto::types::{GlobalCursor, Topic};
use xmtp_proto::xmtp::xmtpv4::message_api::{
    EnvelopesQuery, QueryEnvelopesRequest, QueryEnvelopesResponse,
};

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
        Cow::Borrowed("/xmtp.xmtpv4.message_api.ReplicationApi/QueryEnvelopes")
    }

    fn body(&self) -> Result<Bytes, BodyError> {
        let query = QueryEnvelopesRequest {
            query: Some(EnvelopesQuery {
                topics: self.topics.iter().map(Topic::cloned_vec).collect(),
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
        Cow::Borrowed("/xmtp.xmtpv4.message_api.ReplicationApi/QueryEnvelopes")
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
