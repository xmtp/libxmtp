use derive_builder::Builder;
use prost::Message;
use std::borrow::Cow;
use xmtp_proto::traits::{BodyError, Endpoint};
use xmtp_proto::xmtp::mls::api::v1::FILE_DESCRIPTOR_SET;
use xmtp_proto::xmtp::mls::api::v1::{
    PagingInfo, QueryGroupMessagesRequest, QueryGroupMessagesResponse,
};

#[derive(Debug, Builder, Default)]
#[builder(setter(strip_option))]
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
        todo!()
    }

    fn grpc_endpoint(&self) -> Cow<'static, str> {
        crate::path_and_query::<QueryGroupMessagesRequest>(FILE_DESCRIPTOR_SET)
    }

    fn body(&self) -> Result<Vec<u8>, BodyError> {
        Ok(QueryGroupMessagesRequest {
            group_id: self.group_id.clone(),
            paging_info: self.paging_info,
        }
        .encode_to_vec())
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod test {
    use crate::v3::QueryGroupMessages;
    use xmtp_api_grpc::grpc_client::GrpcClient;
    use xmtp_api_grpc::LOCALHOST_ADDRESS;
    use xmtp_proto::api_client::ApiBuilder;
    use xmtp_proto::traits::Query;
    use xmtp_proto::xmtp::mls::api::v1::{
        QueryGroupMessagesRequest, QueryGroupMessagesResponse, FILE_DESCRIPTOR_SET,
    };

    #[test]
    fn test_file_descriptor() {
        let pnq = crate::path_and_query::<QueryGroupMessagesRequest>(FILE_DESCRIPTOR_SET);
        println!("{}", pnq);
    }

    #[tokio::test]
    async fn test_get_identity_updates_v2() {
        let mut client = GrpcClient::builder();
        client.set_app_version("0.0.0".into()).unwrap();
        client.set_tls(false);
        client.set_host(LOCALHOST_ADDRESS.to_string());
        let client = client.build().await.unwrap();
        let endpoint = QueryGroupMessages::builder()
            .group_id(vec![1, 2, 3])
            .build()
            .unwrap();

        let result: QueryGroupMessagesResponse = endpoint.query(&client).await.unwrap();
        assert_eq!(result.messages.len(), 0);
    }
}
