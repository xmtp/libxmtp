use tonic::transport::Channel;

use crate::xmtp::message_api::v1::message_api_client::MessageApiClient;

include!("gen/mod.rs");

pub async fn create_client(url: String) -> MessageApiClient<Channel> {
    return MessageApiClient::connect(url).await.unwrap();
}
