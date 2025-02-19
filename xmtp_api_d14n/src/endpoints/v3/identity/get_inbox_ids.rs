use derive_builder::Builder;
use std::borrow::Cow;
use prost::Message;
use xmtp_proto::traits::{BodyError, Endpoint};
use xmtp_proto::xmtp::identity::api::v1::{GetIdentityUpdatesResponse, GetInboxIdsRequest};
use xmtp_proto::xmtp::identity::api::v1::get_inbox_ids_request::Request;
use xmtp_proto::xmtp::mls::api::v1::FILE_DESCRIPTOR_SET;

#[derive(Debug, Builder, Default)]
#[builder(setter(strip_option))]
pub struct GetInboxIds {
    #[builder(setter(into))]
    requests: Vec<Request>
}

impl GetInboxIds {
    pub fn builder() -> GetInboxIdsBuilder {
        Default::default()
    }
}

impl Endpoint for GetInboxIds {
    type Output = GetIdentityUpdatesResponse;
    fn http_endpoint(&self) -> Cow<'static, str> {
        todo!()
    }

    fn grpc_endpoint(&self) -> Cow<'static, str> {
        crate::path_and_query::<GetInboxIdsRequest>(FILE_DESCRIPTOR_SET)
    }

    fn body(&self) -> Result<Vec<u8>, BodyError> {
        Ok(GetInboxIdsRequest {
            requests: self.requests.clone(),
        }
        .encode_to_vec())
    }
}
