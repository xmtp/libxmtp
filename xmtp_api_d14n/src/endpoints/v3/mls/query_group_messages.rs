use derive_builder::Builder;
use prost::Message;
use prost::bytes::Bytes;
use std::borrow::Cow;
use xmtp_proto::traits::{BodyError, Endpoint};
use xmtp_proto::xmtp::mls::api::v1::{
    PagingInfo, QueryGroupMessagesRequest, QueryGroupMessagesResponse,
};

#[derive(Debug, Builder, Default)]
#[builder(setter(strip_option), build_fn(error = "BodyError"))]
pub struct QueryGroupMessages {
    #[builder(setter(into))]
    group_id: Vec<u8>,
    #[builder(setter(skip))]
    paging_info: Option<PagingInfo>,
}

impl QueryGroupMessages {
    pub fn builder() -> QueryGroupMessagesBuilder {
        Default::default()
    }
}

impl Endpoint for QueryGroupMessages {
    type Output = QueryGroupMessagesResponse;
    fn http_endpoint(&self) -> Cow<'static, str> {
        Cow::Borrowed("/mls/v1/query-group-messages")
    }

    fn grpc_endpoint(&self) -> Cow<'static, str> {
        crate::path_and_query::<QueryGroupMessagesRequest>()
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

#[cfg(test)]
mod test {
    use crate::v3::QueryGroupMessages;
    use xmtp_proto::prelude::*;
    use xmtp_proto::xmtp::mls::api::v1::*;

    #[xmtp_common::test]
    fn test_file_descriptor() {
        let pnq = crate::path_and_query::<QueryGroupMessagesRequest>();
        println!("{}", pnq);
    }

    #[xmtp_common::test]
    async fn test_get_identity_updates_v2() {
        let client = crate::TestClient::create_local();
        let client = client.build().await.unwrap();
        let endpoint = QueryGroupMessages::builder()
            .group_id(vec![1, 2, 3])
            .build()
            .unwrap();

        let result: QueryGroupMessagesResponse = endpoint.query(&client).await.unwrap();
        assert_eq!(result.messages.len(), 0);
    }
}
