use derive_builder::Builder;
use prost::Message;
use prost::bytes::Bytes;
use std::borrow::Cow;
use xmtp_proto::mls_v1::PagingInfo;
use xmtp_proto::traits::{BodyError, Endpoint};
use xmtp_proto::xmtp::xmtpv4::envelopes::Cursor;
use xmtp_proto::xmtp::xmtpv4::message_api::EnvelopesQuery;
use xmtp_proto::xmtp::xmtpv4::message_api::{QueryEnvelopesRequest, QueryEnvelopesResponse};

/// Query a single thing
#[derive(Debug, Builder, Default, Clone)]
#[builder(build_fn(error = "BodyError"))]
pub struct QueryEnvelope {
    #[builder(setter(each(name = "topic", into)))]
    topics: Vec<Vec<u8>>,
    #[builder(default = None)]
    paging_info: Option<PagingInfo>,
}

impl QueryEnvelope {
    pub fn builder() -> QueryEnvelopeBuilder {
        Default::default()
    }
}

impl Endpoint for QueryEnvelope {
    type Output = QueryEnvelopesResponse;

    fn http_endpoint(&self) -> Cow<'static, str> {
        Cow::from("/mls/v2/query-envelopes")
    }

    fn grpc_endpoint(&self) -> Cow<'static, str> {
        crate::path_and_query::<QueryEnvelopesRequest>()
    }

    fn body(&self) -> Result<Bytes, BodyError> {
        let limit = self.paging_info.map_or(0, |info| info.limit);
        //todo: replace with returned node_id
        let node_id = 100;
        let last_seen = self.paging_info.map(|info| Cursor {
            node_id_to_sequence_id: [(node_id, info.id_cursor)].into(),
        });

        let query = QueryEnvelopesRequest {
            query: Some(EnvelopesQuery {
                topics: self.topics.clone(),
                originator_node_ids: vec![],
                last_seen,
            }),
            limit,
        };
        tracing::debug!("{:?}", query);
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

    fn http_endpoint(&self) -> Cow<'static, str> {
        Cow::Borrowed("/mls/v2/query-envelopes")
    }

    fn grpc_endpoint(&self) -> Cow<'static, str> {
        crate::path_and_query::<QueryEnvelopesRequest>()
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
    use xmtp_proto::prelude::*;

    #[xmtp_common::test]
    fn test_file_descriptor() {
        use xmtp_proto::xmtp::xmtpv4::message_api::QueryEnvelopesRequest;
        let pnq = crate::path_and_query::<QueryEnvelopesRequest>();
        println!("{}", pnq);
    }

    #[xmtp_common::test]
    async fn test_query_envelopes() {
        use crate::d14n::QueryEnvelopes;

        let client = crate::TestClient::create_local_d14n();
        let client = client.build().unwrap();

        let endpoint = QueryEnvelopes::builder()
            .envelopes(EnvelopesQuery {
                topics: vec![vec![]],
                originator_node_ids: vec![],
                last_seen: None,
            })
            .build()
            .unwrap();
        assert!(endpoint.query(&client).await.is_err());
    }
}
