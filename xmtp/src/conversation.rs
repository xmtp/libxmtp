use crate::{
    client::ClientError,
    codecs::{text::TextCodec, CodecError, ContentCodec},
    contact::Contact,
    invitation::{Invitation, InvitationError},
    storage::{
        now, ConversationState, MessageState, NewStoredMessage, StorageError, StoredConversation,
        StoredUser,
    },
    types::networking::PublishRequest,
    types::networking::XmtpApiClient,
    types::Address,
    utils::{build_envelope, build_user_invite_topic},
    Client, Store,
};

use prost::{DecodeError, Message};
// use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConversationError {
    #[error("client error {0}")]
    Client(#[from] ClientError),
    #[error("invitation error {0}")]
    Invitation(#[from] InvitationError),
    #[error("codec error {0}")]
    Codec(#[from] CodecError),
    #[error("decode error {0}")]
    Decode(DecodeError),
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("unknown error")]
    Unknown,
}

pub fn convo_id(self_addr: String, peer_addr: String) -> String {
    let mut members = [self_addr, peer_addr];
    members.sort();
    format!(":{}:{}", members[0], members[1])
}

// I had to pick a name for this, and it seems like we are hovering around SecretConversation ATM
// May very well change
pub struct SecretConversation<'c, A>
where
    A: XmtpApiClient,
{
    peer_address: Address,
    members: Vec<Contact>,
    client: &'c Client<A>,
}

impl<'c, A> SecretConversation<'c, A>
where
    A: XmtpApiClient,
{
    pub(crate) fn new(
        client: &'c Client<A>,
        peer_address: Address,
        // TODO: Add user's own contacts as well
        members: Vec<Contact>,
    ) -> Result<Self, ConversationError> {
        let obj = SecretConversation {
            client,
            peer_address: peer_address.clone(),
            members,
        };
        obj.client.store.insert_or_ignore_user(StoredUser {
            user_address: obj.peer_address(),
            created_at: now(),
            last_refreshed: 0,
        })?;
        obj.client
            .store
            .insert_or_ignore_conversation(StoredConversation {
                peer_address: obj.peer_address(),
                convo_id: obj.convo_id(),
                created_at: now(),
                convo_state: ConversationState::Uninitialized as i32,
            })?;
        Ok(obj)
    }

    pub fn convo_id(&self) -> String {
        convo_id(self.client.account.addr(), self.peer_address())
    }

    pub fn peer_address(&self) -> Address {
        self.peer_address.clone()
    }

    pub fn send_message(&self, text: &str) -> Result<(), ConversationError> {
        // TODO support other codecs
        let encoded_content = TextCodec::encode(text.to_string())?;
        let content_bytes = encoded_content.encode_to_vec();
        NewStoredMessage::new(
            self.convo_id(),
            self.client.account.addr(),
            content_bytes,
            MessageState::Unprocessed as i32,
        )
        .store(&self.client.store)?;
        Ok(())
    }

    pub async fn initialize(&self) -> Result<(), ConversationError> {
        let inner_invite_bytes = Invitation::build_inner_invite_bytes(self.peer_address.clone())?;
        for contact in self.members.iter() {
            let id = contact.installation_id();

            // TODO: Persist session to database
            let session = self.client.create_outbound_session(contact)?;
            let invitation =
                Invitation::build(self.client.account.contact(), session, &inner_invite_bytes)?;

            let envelope = build_envelope(build_user_invite_topic(id), invitation.try_into()?);

            // TODO: Replace with real token
            self.client
                .api_client
                // TODO: API authentication
                .publish(
                    "".to_string(),
                    PublishRequest {
                        envelopes: vec![envelope],
                    },
                )
                .await
                .map_err(|_| ConversationError::Unknown)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use prost::Message;
    use xmtp_proto::xmtp::message_contents::EncodedContent;

    use crate::{
        codecs::{text::TextCodec, ContentCodec},
        conversations::Conversations,
        test_utils::test_utils::{gen_test_client, gen_test_conversation},
    };

    #[tokio::test]
    async fn test_local_conversation_creation() {
        let client = gen_test_client().await;
        let peer_address = "0x000";
        let convo_id = format!(":{}:{}", peer_address, client.wallet_address());
        assert!(client.store.get_conversation(&convo_id).unwrap().is_none());

        let conversations = Conversations::new(&client);
        let conversation = gen_test_conversation(&conversations, peer_address).await;
        assert!(conversation.peer_address() == peer_address);
        assert!(client.store.get_conversation(&convo_id).unwrap().is_some());
    }

    #[tokio::test]
    async fn test_send_message() {
        let client = gen_test_client().await;
        let conversations = Conversations::new(&client);
        let conversation = gen_test_conversation(&conversations, "0x000").await;
        conversation.send_message("Hello, world!").unwrap();

        let message = &client.store.get_unprocessed_messages().unwrap()[0];
        let content = EncodedContent::decode(&message.content[..]).unwrap();
        assert!(TextCodec::decode(content).unwrap() == "Hello, world!");
    }
}
