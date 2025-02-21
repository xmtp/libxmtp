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

#[cfg(test)]
mod test {
    use crate::SendWelcomeMessages;
    use xmtp_api_grpc::grpc_client::GrpcClient;
    use xmtp_api_grpc::LOCALHOST_ADDRESS;
    use xmtp_proto::api_client::ApiBuilder;
    use xmtp_proto::traits::Query;
    use xmtp_proto::xmtp::mls::api::v1::{welcome_message_input, SendWelcomeMessagesRequest, WelcomeMessageInput, FILE_DESCRIPTOR_SET};

    #[test]
    fn test_file_descriptor() {
        let pnq = crate::path_and_query::<SendWelcomeMessagesRequest>(FILE_DESCRIPTOR_SET);
        println!("{}", pnq);
    }

    #[tokio::test]
    async fn test_get_identity_updates_v2() {
        let welcome_message = WelcomeMessageInput {
            version: Some(welcome_message_input::Version::V1(Default::default())),
        };
        let mut client = GrpcClient::builder();
        client.set_app_version("0.0.0".into()).unwrap();
        client.set_tls(false);
        client.set_host(LOCALHOST_ADDRESS.to_string());
        let client = client.build().await.unwrap();
        let endpoint = SendWelcomeMessages::builder()
            .messages(vec![welcome_message])
            .build()
            .unwrap();

        // let result: () = endpoint.query(&client).await.unwrap();
        // assert_eq!(result, ());
    }
}
