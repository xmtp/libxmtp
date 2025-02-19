use derive_builder::Builder;
use prost::Message;
use std::borrow::Cow;
use xmtp_proto::traits::{BodyError, Endpoint};
use xmtp_proto::xmtp::identity::api::v1::PublishIdentityUpdateRequest;
use xmtp_proto::xmtp::identity::associations::IdentityUpdate;
use xmtp_proto::xmtp::mls::api::v1::FILE_DESCRIPTOR_SET;

#[derive(Debug, Builder, Default)]
#[builder(setter(strip_option))]
pub struct PublishIdentityUpdate {
    #[builder(setter(strip_option))]
    pub identity_update: Option<IdentityUpdate>,
}

impl PublishIdentityUpdate {
    pub fn builder() -> PublishIdentityUpdateBuilder {
        Default::default()
    }
}

impl Endpoint for PublishIdentityUpdate {
    type Output = ();
    fn http_endpoint(&self) -> Cow<'static, str> {
        todo!()
    }

    fn grpc_endpoint(&self) -> Cow<'static, str> {
        crate::path_and_query::<PublishIdentityUpdateRequest>(FILE_DESCRIPTOR_SET)
    }

    fn body(&self) -> Result<Vec<u8>, BodyError> {
        Ok(PublishIdentityUpdateRequest {
            identity_update: self.identity_update.clone(),
        }
        .encode_to_vec())
    }
}
