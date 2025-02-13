use derive_builder::Builder;
use prost::Message;
use std::borrow::Cow;
use xmtp_proto::traits::{BodyError, Endpoint};
use xmtp_proto::xmtp::mls::api::v1::FILE_DESCRIPTOR_SET;
use xmtp_proto::xmtp::mls::api::v1::{SendWelcomeMessagesRequest, WelcomeMessageInput};

#[derive(Debug, Builder, Default)]
#[builder(setter(strip_option))]
pub struct SendWelcomeMessages {
    #[builder(setter(into))]
    messages: Vec<WelcomeMessageInput>,
}

impl SendWelcomeMessages {
    pub fn builder() -> SendWelcomeMessagesBuilder {
        Default::default()
    }
}

impl Endpoint for SendWelcomeMessages {
    type Output = ();
    fn http_endpoint(&self) -> Cow<'static, str> {
        todo!()
    }

    fn grpc_endpoint(&self) -> Cow<'static, str> {
        crate::path_and_query::<SendWelcomeMessagesRequest>(FILE_DESCRIPTOR_SET)
    }

    fn body(&self) -> Result<Vec<u8>, BodyError> {
        Ok(SendWelcomeMessagesRequest {
            messages: self.messages.clone(),
        }
        .encode_to_vec())
    }
}
