use derive_builder::Builder;
use prost::Message;
use prost::bytes::Bytes;
use std::borrow::Cow;
use xmtp_proto::api::{BodyError, Endpoint};
use xmtp_proto::xmtp::mls::api::v1::{GroupMessageInput, SendGroupMessagesRequest};

#[derive(Debug, Builder, Default)]
#[builder(setter(strip_option), build_fn(error = "BodyError"))]
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
    fn grpc_endpoint(&self) -> Cow<'static, str> {
        xmtp_proto::path_and_query::<SendGroupMessagesRequest>()
    }

    fn body(&self) -> Result<Bytes, BodyError> {
        Ok(SendGroupMessagesRequest {
            messages: self.messages.clone(),
        }
        .encode_to_vec()
        .into())
    }
}

#[cfg(test)]
mod test {
    use crate::v3::SendGroupMessages;
    use xmtp_api_grpc::test::NodeGoClient;
    use xmtp_proto::xmtp::mls::api::v1::*;
    use xmtp_proto::{api, prelude::*};

    #[xmtp_common::test]
    fn test_file_descriptor() {
        let pnq = xmtp_proto::path_and_query::<SendGroupMessagesRequest>();
        println!("{}", pnq);
    }

    #[xmtp_common::test]
    fn test_grpc_endpoint_returns_correct_path() {
        let endpoint = SendGroupMessages::default();
        assert_eq!(
            endpoint.grpc_endpoint(),
            "/xmtp.mls.api.v1.MlsApi/SendGroupMessages"
        );
    }

    #[xmtp_common::test]
    async fn test_send_group_messages() {
        let client = NodeGoClient::create();
        let client = client.build().unwrap();
        let endpoint = SendGroupMessages::builder()
            .messages(vec![GroupMessageInput::default()])
            .build()
            .unwrap();

        let result = api::ignore(endpoint).query(&client).await;
        assert!(result.is_err());
    }
}
