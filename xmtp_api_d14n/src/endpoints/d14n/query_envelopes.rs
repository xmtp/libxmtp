use derive_builder::Builder;
use prost::Message;
use std::borrow::Cow;
use xmtp_proto::traits::{BodyError, Endpoint};
use xmtp_proto::xmtp::xmtpv4::message_api::EnvelopesQuery;
use xmtp_proto::xmtp::xmtpv4::message_api::FILE_DESCRIPTOR_SET;
use xmtp_proto::xmtp::xmtpv4::message_api::{QueryEnvelopesRequest, QueryEnvelopesResponse};

/// Query a single thing
#[derive(Debug, Builder, Default, Clone)]
#[builder(build_fn(error = "BodyError"))]
pub struct QueryEnvelope {
    #[builder(setter(each(name = "topic", into)))]
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
        Cow::from("/mls/v2/query-envelopes")
    }

    fn grpc_endpoint(&self) -> Cow<'static, str> {
        crate::path_and_query::<QueryEnvelopesRequest>(FILE_DESCRIPTOR_SET)
    }

    fn body(&self) -> Result<Vec<u8>, BodyError> {
        let query = QueryEnvelopesRequest {
            query: Some(EnvelopesQuery {
                topics: self.topics.clone(),
                originator_node_ids: self.originator_node_ids.clone(),
                last_seen: None,
            }),
            limit: 0,
        };
        tracing::debug!("{:?}", query);
        Ok(query.encode_to_vec())
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
        crate::path_and_query::<QueryEnvelopesRequest>(FILE_DESCRIPTOR_SET)
    }

    fn body(&self) -> Result<Vec<u8>, BodyError> {
        let query = QueryEnvelopesRequest {
            query: Some(self.envelopes.clone()),
            limit: self.limit,
        };
        tracing::debug!("QUERY: {:?}", query);
        Ok(query.encode_to_vec())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use xmtp_proto::prelude::*;

    #[xmtp_common::test]
    fn test_file_descriptor() {
        use xmtp_proto::xmtp::xmtpv4::message_api::{QueryEnvelopesRequest, FILE_DESCRIPTOR_SET};
        let pnq = crate::path_and_query::<QueryEnvelopesRequest>(FILE_DESCRIPTOR_SET);
        println!("{}", pnq);
    }

    #[xmtp_common::test]
    async fn test_query_envelopes() {
        use crate::d14n::QueryEnvelopes;

        let client = crate::TestClient::create_local_d14n();
        let client = client.build().await.unwrap();

        let endpoint = QueryEnvelopes::builder()
            .envelopes(EnvelopesQuery {
                topics: vec![vec![]],
                originator_node_ids: vec![],
                last_seen: None,
            })
            .build()
            .unwrap();
        if cfg!(any(feature = "http-api", target_arch = "wasm32")) {
            assert!(endpoint.query(&client).await.is_err());
            // TODO: Investigate why fails with http topic
        } else {
            assert!(endpoint.query(&client).await.is_ok());
        }
    }
}
