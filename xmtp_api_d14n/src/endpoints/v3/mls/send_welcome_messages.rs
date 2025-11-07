use derive_builder::Builder;
use prost::Message;
use prost::bytes::Bytes;
use std::borrow::Cow;
use xmtp_proto::api::{BodyError, Endpoint};
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
    fn grpc_endpoint(&self) -> Cow<'static, str> {
        xmtp_proto::path_and_query::<SendWelcomeMessagesRequest>()
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
    use xmtp_api_grpc::test::NodeGoClient;
    use xmtp_proto::xmtp::mls::api::v1::{
        SendWelcomeMessagesRequest, WelcomeMessageInput, welcome_message_input,
    };
    use xmtp_proto::{api, prelude::*};

    #[xmtp_common::test]
    fn test_file_descriptor() {
        let pnq = xmtp_proto::path_and_query::<SendWelcomeMessagesRequest>();
        println!("{}", pnq);
    }

    #[xmtp_common::test]
    fn test_grpc_endpoint_returns_correct_path() {
        let endpoint = SendWelcomeMessages::default();
        assert_eq!(
            endpoint.grpc_endpoint(),
            "/xmtp.mls.api.v1.MlsApi/SendWelcomeMessages"
        );
    }

    #[xmtp_common::test]
    async fn test_send_welcome_messages() {
        let welcome_message = WelcomeMessageInput {
            version: Some(welcome_message_input::Version::V1(Default::default())),
        };
        let client = NodeGoClient::create();
        let client = client.build().unwrap();
        let endpoint = SendWelcomeMessages::builder()
            .messages(vec![welcome_message])
            .build()
            .unwrap();

        let result = api::ignore(endpoint).query(&client).await;
        assert!(result.is_err())
    }
}
