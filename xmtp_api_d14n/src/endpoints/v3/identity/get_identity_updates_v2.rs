use derive_builder::Builder;
use prost::Message;
use prost::bytes::Bytes;
use std::borrow::Cow;
use xmtp_proto::api::{BodyError, Endpoint};
use xmtp_proto::identity_v1::GetIdentityUpdatesResponse;
use xmtp_proto::xmtp::identity::api::v1::GetIdentityUpdatesRequest;
use xmtp_proto::xmtp::identity::api::v1::get_identity_updates_request::Request;

#[derive(Debug, Builder, Default)]
#[builder(setter(strip_option), build_fn(error = "BodyError"))]
pub struct GetIdentityUpdatesV2 {
    #[builder(setter(into, each = "request"))]
    pub requests: Vec<Request>,
}

impl GetIdentityUpdatesV2 {
    pub fn builder() -> GetIdentityUpdatesV2Builder {
        Default::default()
    }
}

impl Endpoint for GetIdentityUpdatesV2 {
    type Output = GetIdentityUpdatesResponse;
    fn grpc_endpoint(&self) -> Cow<'static, str> {
        xmtp_proto::path_and_query::<GetIdentityUpdatesRequest>()
    }

    fn body(&self) -> Result<Bytes, BodyError> {
        Ok(GetIdentityUpdatesRequest {
            requests: self.requests.clone(),
        }
        .encode_to_vec()
        .into())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use xmtp_proto::{identity_v1::GetIdentityUpdatesResponse, prelude::*};

    #[xmtp_common::test]
    fn test_file_descriptor() {
        let pnq = xmtp_proto::path_and_query::<GetIdentityUpdatesRequest>();
        println!("{}", pnq);
    }

    #[xmtp_common::test]
    fn test_grpc_endpoint_returns_correct_path() {
        let endpoint = GetIdentityUpdatesV2::default();
        assert_eq!(
            endpoint.grpc_endpoint(),
            "/xmtp.identity.api.v1.IdentityApi/GetIdentityUpdates"
        );
    }

    #[xmtp_common::test]
    async fn test_get_identity_updates_v2() {
        let client = crate::TestGrpcClient::create_local();
        let client = client.build().unwrap();
        let mut endpoint = GetIdentityUpdatesV2::builder()
            .requests(vec![Request {
                inbox_id: "".to_string(),
                sequence_id: 0,
            }])
            .build()
            .unwrap();

        let result: GetIdentityUpdatesResponse = endpoint.query(&client).await.unwrap();
        assert_eq!(result.responses.len(), 1);
    }
}
