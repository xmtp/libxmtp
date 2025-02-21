use derive_builder::Builder;
use prost::Message;
use std::borrow::Cow;
use xmtp_proto::traits::{BodyError, Endpoint};
use xmtp_proto::xmtp::identity::api::v1::get_identity_updates_request::Request;
use xmtp_proto::xmtp::identity::api::v1::{
    GetIdentityUpdatesRequest, GetIdentityUpdatesResponse, FILE_DESCRIPTOR_SET,
};

#[derive(Debug, Builder, Default)]
#[builder(setter(strip_option))]
pub struct GetIdentityUpdatesV2 {
    #[builder(setter(into))]
    pub requests: Vec<Request>,
}
impl GetIdentityUpdatesV2 {
    pub fn builder() -> GetIdentityUpdatesV2Builder {
        Default::default()
    }
}
impl Endpoint for GetIdentityUpdatesV2 {
    type Output = GetIdentityUpdatesResponse;
    fn http_endpoint(&self) -> Cow<'static, str> {
        todo!()
    }

    fn grpc_endpoint(&self) -> Cow<'static, str> {
        crate::path_and_query::<GetIdentityUpdatesRequest>(FILE_DESCRIPTOR_SET)
    }

    fn body(&self) -> Result<Vec<u8>, BodyError> {
        Ok(GetIdentityUpdatesRequest {
            requests: self.requests.clone(),
        }
        .encode_to_vec())
    }
}
#[cfg(test)]
mod test {
    use crate::GetIdentityUpdatesV2;
    use xmtp_api_grpc::grpc_client::GrpcClient;
    use xmtp_api_grpc::LOCALHOST_ADDRESS;
    use xmtp_proto::api_client::ApiBuilder;
    use xmtp_proto::traits::Query;
    use xmtp_proto::xmtp::identity::api::v1::{get_identity_updates_request::Request, GetIdentityUpdatesRequest, GetIdentityUpdatesResponse, FILE_DESCRIPTOR_SET};

    #[test]
    fn test_file_descriptor() {
        let pnq = crate::path_and_query::<GetIdentityUpdatesRequest>(FILE_DESCRIPTOR_SET);
        println!("{}", pnq);
    }

    #[tokio::test]
    async fn test_get_identity_updates_v2() {
        let mut client = GrpcClient::builder();
        client.set_app_version("0.0.0".into()).unwrap();
        client.set_tls(false);
        client.set_host(LOCALHOST_ADDRESS.to_string());
        let client = client.build().await.unwrap();
        let endpoint = GetIdentityUpdatesV2::builder()
            .requests(vec![Request {
                inbox_id: "".to_string(),
                sequence_id: 0,
            }])
            .build()
            .unwrap();

        let result:GetIdentityUpdatesResponse  = endpoint.query(&client).await.unwrap();
        assert_eq!(result.responses.len(), 0);
    }
}
