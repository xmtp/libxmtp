use derive_builder::Builder;
use prost::Message;
use std::borrow::Cow;
use xmtp_proto::traits::{BodyError, Endpoint};
use xmtp_proto::xmtp::xmtpv4::message_api::EnvelopesQuery;
use xmtp_proto::xmtp::xmtpv4::message_api::FILE_DESCRIPTOR_SET;
use xmtp_proto::xmtp::xmtpv4::message_api::{QueryEnvelopesRequest, QueryEnvelopesResponse};

/// Query a single thing
#[derive(Debug, Builder, Default, Clone)]
pub struct QueryEnvelope {
    #[builder(setter(into))]
    topics: Vec<Vec<u8>>,
    #[builder(setter(into))]
    originator_node_ids: Vec<u32>,
}

impl QueryEnvelope {
    pub fn builder() -> QueryEnvelopeBuilder {
        Default::default()
    }
}

impl Endpoint for QueryEnvelope {
    type Output = QueryEnvelopesResponse;

    fn http_endpoint(&self) -> Cow<'static, str> {
        todo!()
    }

    fn grpc_endpoint(&self) -> Cow<'static, str> {
        crate::path_and_query::<QueryEnvelopesRequest>(FILE_DESCRIPTOR_SET)
    }

    fn body(&self) -> Result<Vec<u8>, BodyError> {
        Ok(QueryEnvelopesRequest {
            query: Some(EnvelopesQuery {
                topics: self.topics.clone(),
                originator_node_ids: self.originator_node_ids.clone(),
                last_seen: None,
            }),
            limit: 1,
        }
        .encode_to_vec())
    }
}

/// Batch Query
#[derive(Debug, Builder, Default)]
#[builder(setter(strip_option))]
pub struct QueryEnvelopes {
    #[builder(setter(into))]
    envelopes: EnvelopesQuery,
    #[builder(setter(into))]
    limit: u32,
}

impl QueryEnvelopes {
    pub fn builder() -> QueryEnvelopesBuilder {
        Default::default()
    }
}

impl Endpoint for QueryEnvelopes {
    type Output = QueryEnvelopesResponse;

    fn http_endpoint(&self) -> Cow<'static, str> {
        todo!()
    }

    fn grpc_endpoint(&self) -> Cow<'static, str> {
        crate::path_and_query::<QueryEnvelopesRequest>(FILE_DESCRIPTOR_SET)
    }

    fn body(&self) -> Result<Vec<u8>, BodyError> {
        Ok(QueryEnvelopesRequest {
            query: Some(self.envelopes.clone()),
            limit: self.limit,
        }
        .encode_to_vec())
    }
}
