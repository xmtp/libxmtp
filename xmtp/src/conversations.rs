use prost::Message;
use xmtp_proto::xmtp::{message_api::v1::QueryRequest, v3::message_contents::InvitationV1};

use crate::{
    conversation::{ConversationError, SecretConversation},
    invitation::Invitation,
    types::networking::XmtpApiClient,
    utils::build_user_invite_topic,
    Client,
};

pub struct Conversations<'c, A>
where
    A: XmtpApiClient,
{
    client: &'c Client<A>,
}

impl<'c, A> Conversations<'c, A>
where
    A: XmtpApiClient,
{
    pub fn new(client: &'c Client<A>) -> Self {
        Self { client }
    }

    pub async fn new_secret_conversation(
        &self,
        wallet_address: String,
    ) -> Result<SecretConversation<A>, ConversationError> {
        let contacts = self.client.get_contacts(wallet_address.as_str()).await?;
        let conversation = SecretConversation::new(self.client, wallet_address, contacts);

        Ok(conversation)
    }

    pub async fn load(&self) -> Result<Vec<SecretConversation<A>>, ConversationError> {
        let my_contact = self.client.account.contact();
        // TODO: Paginate results to allow for > 100 invites
        let response = self
            .client
            .api_client
            .query(QueryRequest {
                content_topics: vec![build_user_invite_topic(my_contact.installation_id())],
                start_time_ns: 0,
                end_time_ns: 0,
                paging_info: None,
            })
            .await
            .map_err(|_| ConversationError::Unknown)?;

        let mut conversations: Vec<SecretConversation<A>> = vec![];

        for envelope in response.envelopes {
            let invite: Invitation = envelope.message.try_into()?;
            let (_, plaintext) = self
                .client
                .create_inbound_session(invite.inviter.clone(), invite.ciphertext)?;

            let inner_invite = InvitationV1::decode(plaintext.as_slice())
                .map_err(|_| ConversationError::Unknown)?;

            if invite.inviter == my_contact {
                // TODO: Load all installations from peer
                let conversation = SecretConversation::new(
                    self.client,
                    inner_invite.invitee_wallet_address,
                    vec![invite.inviter],
                );
                conversations.push(conversation);
            } else {
                if inner_invite.invitee_wallet_address.clone() != my_contact.wallet_address {
                    println!("invitee_wallet_address does not match");
                    continue;
                }
                let conversation = SecretConversation::new(
                    self.client,
                    invite.inviter.clone().wallet_address,
                    vec![invite.inviter],
                );
                conversations.push(conversation);
            }

            // TODO: Fill me in
        }

        Ok(conversations)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        conversations::Conversations, mock_xmtp_api_client::MockXmtpApiClient, ClientBuilder,
    };

    #[tokio::test]
    async fn create_secret_conversation() {
        let mut alice_client = ClientBuilder::new_test().build().unwrap();
        alice_client.init().await.unwrap();
        let mut bob_client = ClientBuilder::new_test().build().unwrap();
        bob_client.init().await.unwrap();

        let conversations = Conversations::new(&alice_client);
        let conversation = conversations
            .new_secret_conversation(bob_client.wallet_address().to_string())
            .await
            .unwrap();

        assert_eq!(conversation.peer_address(), bob_client.wallet_address());
        conversation.initialize().await.unwrap();
    }

    #[tokio::test]
    async fn load_conversations() {
        let api_client = MockXmtpApiClient::new();
        let alice_client = ClientBuilder::new_test()
            .api_client(api_client)
            .build()
            .unwrap();
        let bob_client = ClientBuilder::new_test().build().unwrap();
        let alice_conversations = Conversations::new(&alice_client);
        let bob_conversations = Conversations::new(&bob_client);

        let alice_convo = alice_conversations
            .new_secret_conversation(bob_client.wallet_address().to_string())
            .await
            .unwrap();
        alice_convo.initialize().await.unwrap();

        let bob_convo_list = bob_conversations.load().await.unwrap();
        assert_eq!(bob_convo_list.len(), 1);
    }
}
