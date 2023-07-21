#[cfg(test)]
pub mod test_utils {
    use crate::{
        conversation::SecretConversation, conversations::Conversations,
        mock_xmtp_api_client::MockXmtpApiClient, types::networking::XmtpApiClient, Client,
        ClientBuilder,
    };

    pub async fn gen_test_client() -> Client<MockXmtpApiClient> {
        let mut client = ClientBuilder::new_test().build().unwrap();
        client.init().await.expect("BadReg");
        client
    }

    pub async fn gen_test_conversation<'c, A: XmtpApiClient>(
        conversations: &'c Conversations<'c, A>,
        peer_address: &str,
    ) -> SecretConversation<'c, A> {
        conversations
            .new_secret_conversation(peer_address.to_string())
            .await
            .unwrap()
    }
}
