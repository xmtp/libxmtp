use derive_builder::Builder;
use prost::Message;
use prost::bytes::Bytes;
use std::borrow::Cow;
use xmtp_proto::api::{BodyError, Endpoint};
use xmtp_proto::xmtp::identity::api::v1::get_identity_updates_request::Request;
use xmtp_proto::xmtp::identity::api::v1::{GetIdentityUpdatesRequest, GetIdentityUpdatesResponse};

#[derive(Debug, Builder, Default)]
#[builder(setter(strip_option), build_fn(error = "BodyError"))]
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
        Cow::Borrowed("/identity/v1/get-identity-updates")
    }

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
    use xmtp_proto::prelude::*;

    #[xmtp_common::test]
    fn test_file_descriptor() {
        let pnq = xmtp_proto::path_and_query::<GetIdentityUpdatesRequest>();
        println!("{}", pnq);
    }

    #[xmtp_common::test]
    async fn test_get_identity_updates_v2() {
        let client = crate::TestClient::create_local();
        let client = client.build().await.unwrap();
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
