use derive_builder::Builder;
use prost::Message;
use std::borrow::Cow;
use xmtp_proto::traits::{BodyError, Endpoint};
use xmtp_proto::xmtp::mls::api::v1::FILE_DESCRIPTOR_SET;
use xmtp_proto::xmtp::mls::api::v1::{GroupMessageInput, SendGroupMessagesRequest};

#[derive(Debug, Builder, Default)]
#[builder(setter(strip_option))]
pub struct SendGroupMessages {
    #[builder(setter(into))]
    messages: Vec<GroupMessageInput>,
}

impl SendGroupMessages {
    pub fn builder() -> SendGroupMessagesBuilder {
        Default::default()
    }
}

impl Endpoint for SendGroupMessages {
    type Output = ();
    fn http_endpoint(&self) -> Cow<'static, str> {
        Cow::Borrowed("/mls/v1/send-group-messages")
    }

    fn grpc_endpoint(&self) -> Cow<'static, str> {
        crate::path_and_query::<SendGroupMessagesRequest>(FILE_DESCRIPTOR_SET)
    }

    fn body(&self) -> Result<Vec<u8>, BodyError> {
        Ok(SendGroupMessagesRequest {
            messages: self.messages.clone(),
        }
        .encode_to_vec())
    }
}

#[cfg(test)]
mod test {
    use crate::v3::SendGroupMessages;
    use xmtp_proto::prelude::*;
    use xmtp_proto::xmtp::mls::api::v1::*;

    #[xmtp_common::test]
    fn test_file_descriptor() {
        let pnq = crate::path_and_query::<SendGroupMessagesRequest>(FILE_DESCRIPTOR_SET);
        println!("{}", pnq);
    }

    #[xmtp_common::test]
    async fn test_send_group_messages() {
        let client = crate::TestClient::create_local();
        let client = client.build().await.unwrap();
        let endpoint = SendGroupMessages::builder()
            .messages(vec![GroupMessageInput::default()])
            .build()
            .unwrap();

        let result = endpoint.query(&client).await;
        assert!(result.is_err());
    }
}
