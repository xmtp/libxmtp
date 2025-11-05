use derive_builder::Builder;
use prost::Message;
use prost::bytes::Bytes;
use std::borrow::Cow;
use xmtp_proto::api::{BodyError, Endpoint, Pageable};
use xmtp_proto::mls_v1::QueryGroupMessagesResponse;
use xmtp_proto::xmtp::mls::api::v1::{PagingInfo, QueryGroupMessagesRequest};

#[derive(Debug, Builder, Default)]
#[builder(setter(strip_option), build_fn(error = "BodyError"))]
pub struct QueryGroupMessages {
    #[builder(setter(into))]
    group_id: Vec<u8>,
    paging_info: Option<PagingInfo>,
}

impl QueryGroupMessages {
    pub fn builder() -> QueryGroupMessagesBuilder {
        Default::default()
    }
}

impl Endpoint for QueryGroupMessages {
    type Output = QueryGroupMessagesResponse;
    fn grpc_endpoint(&self) -> Cow<'static, str> {
        xmtp_proto::path_and_query::<QueryGroupMessagesRequest>()
    }

    fn body(&self) -> Result<Bytes, BodyError> {
        Ok(QueryGroupMessagesRequest {
            group_id: self.group_id.clone(),
            paging_info: self.paging_info,
        }
        .encode_to_vec()
        .into())
    }
}

impl Pageable for QueryGroupMessages {
    fn set_cursor(&mut self, cursor: u64) {
        if let Some(ref mut p) = self.paging_info {
            p.id_cursor = cursor;
        }
    }
}

#[cfg(test)]
mod test {
    use crate::v3::QueryGroupMessages;
    use xmtp_proto::prelude::*;
    use xmtp_proto::xmtp::mls::api::v1::*;

    #[xmtp_common::test]
    fn test_file_descriptor() {
        let pnq = xmtp_proto::path_and_query::<QueryGroupMessagesRequest>();
        println!("{}", pnq);
    }

    #[xmtp_common::test]
    fn test_grpc_endpoint_returns_correct_path() {
        let endpoint = QueryGroupMessages::default();
        assert_eq!(
            endpoint.grpc_endpoint(),
            "/xmtp.mls.api.v1.MlsApi/QueryGroupMessages"
        );
    }

    #[xmtp_common::test]
    async fn test_query_group_messages() {
        let client = crate::TestGrpcClient::create_local();
        let client = client.build().unwrap();
        let mut endpoint = QueryGroupMessages::builder()
            .group_id(vec![1, 2, 3])
            .paging_info(PagingInfo::default())
            .build()
            .unwrap();

        let result: QueryGroupMessagesResponse = endpoint.query(&client).await.unwrap();
        assert_eq!(result.messages.len(), 0);
    }
}
