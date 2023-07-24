#[cfg(test)]
pub mod test_utils {
    use crate::{
        conversation::SecretConversation, mock_xmtp_api_client::MockXmtpApiClient,
        types::networking::XmtpApiClient, Client, ClientBuilder,
    };

    pub async fn gen_test_client<'c>() -> Client<'c, MockXmtpApiClient> {
        let mut client = ClientBuilder::new_test().build().unwrap();
        client.init().await.expect("BadReg");
        client
    }

    pub async fn gen_test_conversation<'c, A: XmtpApiClient>(
        client: &'c Client<'c, A>,
        peer_address: &str,
    ) -> SecretConversation<'c, A> {
        client
            .conversations()
            .new_secret_conversation(peer_address.to_string())
            .await
            .unwrap()
    }
}
