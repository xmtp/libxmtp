use derive_builder::Builder;
use std::borrow::Cow;
use prost::Message;
use xmtp_proto::traits::{BodyError, Endpoint};
use xmtp_proto::xmtp::identity::api::v1::{GetIdentityUpdatesRequest, FILE_DESCRIPTOR_SET};
use xmtp_proto::xmtp::identity::api::v1::get_identity_updates_request::Request;

#[derive(Debug, Builder, Default)]
#[builder(setter(strip_option))]
pub struct GetIdentityUpdatesV2 {
    #[builder(setter(into))]
    pub requests: Vec<Request>
}
impl GetIdentityUpdatesV2 {
    pub fn builder() -> GetIdentityUpdatesV2Builder {
        Default::default()
    }
}
impl Endpoint for GetIdentityUpdatesV2 {
    type Output = ();
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
