#[cfg(test)]
pub mod test_utils {
    use xmtp_proto::api_client::XmtpApiClient;

    use crate::{
        conversation::Conversation, mock_xmtp_api_client::MockXmtpApiClient, Client, ClientBuilder,
    };

    async fn gen_test_client_internal(api_client: MockXmtpApiClient) -> Client<MockXmtpApiClient> {
        let mut client = ClientBuilder::new_test()
            .api_client(api_client)
            .build()
            .unwrap();
        client.init().await.expect("BadReg");
        client
    }

    pub async fn gen_test_client() -> Client<MockXmtpApiClient> {
        gen_test_client_internal(MockXmtpApiClient::new()).await
    }

    // Generate test clients pointing to the same network
    pub async fn gen_two_test_clients() -> (Client<MockXmtpApiClient>, Client<MockXmtpApiClient>) {
        let api_client_1 = MockXmtpApiClient::new();
        let api_client_2 = api_client_1.clone();
        (
            gen_test_client_internal(api_client_1).await,
            gen_test_client_internal(api_client_2).await,
        )
    }

    pub async fn gen_test_conversation<'c, A: XmtpApiClient>(
        client: &'c Client<A>,
        peer_address: &str,
    ) -> Conversation<'c, A> {
        Conversation::new(client, peer_address.to_string()).unwrap()
    }
}
