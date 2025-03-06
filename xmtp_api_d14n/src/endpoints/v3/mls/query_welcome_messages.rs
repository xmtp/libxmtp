use derive_builder::Builder;
use prost::Message;
use std::borrow::Cow;
use xmtp_proto::traits::{BodyError, Endpoint};
use xmtp_proto::xmtp::mls::api::v1::FILE_DESCRIPTOR_SET;
use xmtp_proto::xmtp::mls::api::v1::{
    PagingInfo, QueryWelcomeMessagesRequest, QueryWelcomeMessagesResponse,
};

#[derive(Debug, Builder, Default)]
#[builder(setter(strip_option))]
pub struct QueryWelcomeMessages {
    #[builder(setter(into))]
    installation_key: Vec<u8>,
    #[builder(setter(skip))]
    paging_info: Option<PagingInfo>,
}

impl QueryWelcomeMessages {
    pub fn builder() -> QueryWelcomeMessagesBuilder {
        Default::default()
    }
}

impl Endpoint for QueryWelcomeMessages {
    type Output = QueryWelcomeMessagesResponse;

    fn http_endpoint(&self) -> Cow<'static, str> {
        todo!()
    }

    fn grpc_endpoint(&self) -> Cow<'static, str> {
        crate::path_and_query::<QueryWelcomeMessagesRequest>(FILE_DESCRIPTOR_SET)
    }

    fn body(&self) -> Result<Vec<u8>, BodyError> {
        Ok(QueryWelcomeMessagesRequest {
            installation_key: self.installation_key.clone(),
            paging_info: self.paging_info,
        }
        .encode_to_vec())
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod test {
    use crate::v3::QueryWelcomeMessages;
    use xmtp_api_grpc::grpc_client::GrpcClient;
    use xmtp_api_grpc::LOCALHOST_ADDRESS;
    use xmtp_proto::api_client::ApiBuilder;
    use xmtp_proto::traits::Query;
    use xmtp_proto::xmtp::mls::api::v1::{
        QueryWelcomeMessagesRequest, QueryWelcomeMessagesResponse, FILE_DESCRIPTOR_SET,
    };

    #[test]
    fn test_file_descriptor() {
        let pnq = crate::path_and_query::<QueryWelcomeMessagesRequest>(FILE_DESCRIPTOR_SET);
        println!("{}", pnq);
    }

    #[cfg(feature = "grpc-api")]
    #[tokio::test]
    async fn test_get_identity_updates_v2() {
        let mut client = GrpcClient::builder();
        client.set_app_version("0.0.0".into()).unwrap();
        client.set_tls(false);
        client.set_host(LOCALHOST_ADDRESS.to_string());
        let client = client.build().await.unwrap();
        let endpoint = QueryWelcomeMessages::builder()
            .installation_key(vec![1, 2, 3])
            .build()
            .unwrap();

        let result: QueryWelcomeMessagesResponse = endpoint.query(&client).await.unwrap();
        assert_eq!(result.messages.len(), 0);
    }
}
