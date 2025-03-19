use derive_builder::Builder;
use prost::bytes::Bytes;
use prost::Message;
use std::borrow::Cow;
use xmtp_proto::traits::{BodyError, Endpoint};
use xmtp_proto::xmtp::mls::api::v1::FILE_DESCRIPTOR_SET;
use xmtp_proto::xmtp::mls::api::v1::{SendWelcomeMessagesRequest, WelcomeMessageInput};

#[derive(Debug, Builder, Default)]
#[builder(setter(strip_option), build_fn(error = "BodyError"))]
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
        Cow::Borrowed("/mls/v1/send-welcome-messages")
    }

    fn grpc_endpoint(&self) -> Cow<'static, str> {
        crate::path_and_query::<SendWelcomeMessagesRequest>(FILE_DESCRIPTOR_SET)
    }

    fn body(&self) -> Result<Bytes, BodyError> {
        Ok(SendWelcomeMessagesRequest {
            messages: self.messages.clone(),
        }
        .encode_to_vec()
        .into())
    }
}

#[cfg(test)]
mod test {
    use crate::v3::SendWelcomeMessages;
    use xmtp_proto::prelude::*;
    use xmtp_proto::xmtp::mls::api::v1::{
        welcome_message_input, SendWelcomeMessagesRequest, WelcomeMessageInput, FILE_DESCRIPTOR_SET,
    };

    #[xmtp_common::test]
    fn test_file_descriptor() {
        let pnq = crate::path_and_query::<SendWelcomeMessagesRequest>(FILE_DESCRIPTOR_SET);
        println!("{}", pnq);
    }

    #[xmtp_common::test]
    async fn test_send_welcome_messages() {
        let welcome_message = WelcomeMessageInput {
            version: Some(welcome_message_input::Version::V1(Default::default())),
        };
        let client = crate::TestClient::create_local();
        let client = client.build().await.unwrap();
        let endpoint = SendWelcomeMessages::builder()
            .messages(vec![welcome_message])
            .build()
            .unwrap();

        let result = endpoint.query(&client).await;
        assert!(result.is_err())
    }
}
