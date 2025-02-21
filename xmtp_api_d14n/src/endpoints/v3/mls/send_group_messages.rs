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
        todo!()
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
    use xmtp_api_grpc::grpc_client::GrpcClient;
    use xmtp_api_grpc::LOCALHOST_ADDRESS;
    use xmtp_proto::api_client::ApiBuilder;
    use xmtp_proto::traits::Query;
    use xmtp_proto::xmtp::mls::api::v1::{GroupMessageInput, SendGroupMessagesRequest, FILE_DESCRIPTOR_SET};
    use crate::SendGroupMessages;

    #[test]
    fn test_file_descriptor() {
        let pnq = crate::path_and_query::<SendGroupMessagesRequest>(FILE_DESCRIPTOR_SET);
        println!("{}", pnq);
    }

    #[tokio::test]
    async fn test_get_identity_updates_v2() {
        let mut client = GrpcClient::builder();
        client.set_app_version("0.0.0".into()).unwrap();
        client.set_tls(false);
        client.set_host(LOCALHOST_ADDRESS.to_string());
        let client = client.build().await.unwrap();
        let endpoint = SendGroupMessages::builder()
            .messages(vec![GroupMessageInput::default()])
            .build()
            .unwrap();

        let result: () = endpoint.query(&client).await.unwrap();
        assert_eq!(result, ());
    }
}
