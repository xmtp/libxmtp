use crate::{
    client::ClientError,
    codecs::{text::TextCodec, CodecError, ContentCodec},
    contact::Contact,
    conversations::Conversations,
    invitation::{Invitation, InvitationError},
    message::PayloadError,
    session::SessionError,
    storage::{
        now, ConversationState, DbConnection, MessageState, NewStoredMessage, StorageError,
        StoredConversation, StoredMessage, StoredUser,
    },
    types::networking::PublishRequest,
    types::networking::XmtpApiClient,
    types::Address,
    utils::{build_envelope, build_user_invite_topic},
    Client, Save, Store,
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
    Decode(#[from] DecodeError),
    #[error("vmacdecode error {0}")]
    DecodeVmac(#[from] vodozemac::DecodeError),
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("diesel error: {0}")]
    Diesel(#[from] diesel::result::Error),
    #[error("No sessions for user: {0}")]
    NoSessions(String),
    #[error("Session: {0}")]
    Session(#[from] SessionError),
    #[error("Network error: {0}")]
    Networking(#[from] crate::types::networking::Error),
    #[error("Payload:{0}")]
    Payload(#[from] PayloadError),
    #[error("error:{0}")]
    Generic(String),
}

pub fn convo_id(self_addr: String, peer_addr: String) -> String {
    let mut members = [self_addr, peer_addr];
    members.sort();
    format!(":{}:{}", members[0], members[1])
}

pub fn peer_addr_from_convo_id(
    convo_id: &str,
    self_addr: &str,
) -> Result<String, ConversationError> {
    let segments = convo_id.split(':').collect::<Vec<&str>>();
    if segments.len() != 3 {
        return Err(ConversationError::Decode(DecodeError::new(format!(
            "Invalid convo ID {}",
            convo_id
        ))));
    }
    if segments[1] == self_addr {
        Ok(segments[2].to_string())
    } else {
        Ok(segments[1].to_string())
    }
}

#[derive(Default)]
pub struct ListMessagesOptions {
    pub start_time_ns: Option<i64>,
    pub end_time_ns: Option<i64>,
    pub limit: Option<i64>,
}

impl ListMessagesOptions {
    pub fn new(start_time_ns: Option<i64>, end_time_ns: Option<i64>, limit: Option<i64>) -> Self {
        Self {
            start_time_ns,
            end_time_ns,
            limit,
        }
    }
}

// I had to pick a name for this, and it seems like we are hovering around SecretConversation ATM
// May very well change
pub struct SecretConversation<'c, A>
where
    A: XmtpApiClient,
{
    peer_address: Address,
    client: &'c Client<A>,
}

impl<'c, A> SecretConversation<'c, A>
where
    A: XmtpApiClient,
{
    pub fn new(client: &'c Client<A>, peer_address: Address) -> Self {
        Self {
            client,
            peer_address,
        }
    }

    // Instantiate the conversation and insert all the necessary records into the database
    pub(crate) fn create(
        client: &'c Client<A>,
        peer_address: Address,
    ) -> Result<Self, ConversationError> {
        let obj = Self::new(client, peer_address);
        let conn = &mut client.store.conn()?;

        obj.client.store.insert_or_ignore_user_with_conn(
            conn,
            StoredUser {
                user_address: obj.peer_address(),
                created_at: now(),
                last_refreshed: 0,
            },
        )?;

        obj.client.store.insert_or_ignore_conversation_with_conn(
            conn,
            StoredConversation {
                peer_address: obj.peer_address(),
                convo_id: obj.convo_id(),
                created_at: now(),
                convo_state: ConversationState::Uninitialized as i32,
            },
        )?;

        Ok(obj)
    }

    pub fn convo_id(&self) -> String {
        convo_id(self.client.account.addr(), self.peer_address())
    }

    pub fn peer_address(&self) -> Address {
        self.peer_address.clone()
    }

    pub async fn send(&self, content_bytes: Vec<u8>) -> Result<(), ConversationError> {
        NewStoredMessage::new(
            self.convo_id(),
            self.client.account.addr(),
            content_bytes,
            MessageState::Unprocessed as i32,
            now(),
        )
        .store(&mut self.client.store.conn().unwrap())?;

        let conversations = Conversations::new(&self.client);
        if let Err(err) = conversations.process_outbound_messages().await {
            log::error!("Could not process outbound messages on init: {:?}", err)
        }

        Ok(())
    }

    pub async fn send_text(&self, text: &str) -> Result<(), ConversationError> {
        // TODO support other codecs
        let encoded_content = TextCodec::encode(text.to_string())?;
        let content_bytes = encoded_content.encode_to_vec();

        self.send(content_bytes).await
    }

    pub async fn list_messages(
        &self,
        opts: &ListMessagesOptions,
    ) -> Result<Vec<StoredMessage>, ConversationError> {
        let conn = &mut self.client.store.conn()?;
        let messages = self.client.store.get_stored_messages(
            conn,
            Some(vec![MessageState::Received, MessageState::LocallyCommitted]),
            Some(self.convo_id().as_str()),
            opts.start_time_ns,
            opts.end_time_ns,
            opts.limit,
        )?;

        Ok(messages)
    }

    fn members(&self, conn: &mut DbConnection) -> Result<Vec<Contact>, ConversationError> {
        let my_installations = self.client.my_other_devices(conn)?;
        let peer_installations = self
            .client
            .get_contacts_from_db(conn, self.peer_address().as_str())?;

        Ok(vec![my_installations, peer_installations].concat())
    }

    pub async fn initialize(&self) -> Result<(), ConversationError> {
        self.client
            .refresh_user_installations(self.peer_address().as_str())
            .await?;
        let inner_invite_bytes = Invitation::build_inner_invite_bytes(self.peer_address.clone())?;
        let conn = &mut self.client.store.conn()?;
        for contact in self.members(conn)?.iter() {
            let id = contact.installation_id();

            let mut session = self.client.get_session(conn, contact)?;
            let invitation = Invitation::build(
                self.client.account.contact(),
                &mut session,
                &inner_invite_bytes,
            )?;

            let envelope = build_envelope(build_user_invite_topic(id), invitation.try_into()?);

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
                .map_err(|e| ConversationError::Generic(format!("initialize:{}", e)))?;

            session.save(conn)?;
        }

        self.client.store.set_conversation_state(
            conn,
            self.convo_id().as_str(),
            ConversationState::Invited,
        )?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use prost::Message;
    use xmtp_proto::xmtp::message_contents::EncodedContent;

    use crate::{
        codecs::{text::TextCodec, ContentCodec},
        conversation::ListMessagesOptions,
        conversations::Conversations,
        mock_xmtp_api_client::MockXmtpApiClient,
        test_utils::test_utils::{
            gen_test_client, gen_test_client_internal, gen_test_conversation,
        },
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
    async fn test_send_text() {
        let client = gen_test_client().await;
        let conversations = Conversations::new(&client);
        let conversation = gen_test_conversation(&conversations, "0x000").await;
        conversation.send_text("Hello, world!").await.unwrap();

        let message = &client.store.get_unprocessed_messages().unwrap()[0];
        let content = EncodedContent::decode(&message.content[..]).unwrap();
        assert!(TextCodec::decode(content).unwrap() == "Hello, world!");
    }

    #[tokio::test]
    async fn test_list_messages() {
        let api_client = MockXmtpApiClient::new();
        let client = gen_test_client_internal(api_client.clone()).await;
        let recipient = gen_test_client_internal(api_client.clone()).await;
        let conversations = Conversations::new(&client);
        let conversation =
            gen_test_conversation(&conversations, recipient.account.addr().as_str()).await;
        conversation.initialize().await.unwrap();
        conversation.send_text("Hello, world!").await.unwrap();
        conversation.send_text("Hello, again").await.unwrap();

        conversations.process_outbound_messages().await.unwrap();

        let results = conversation
            .list_messages(&ListMessagesOptions::default())
            .await
            .unwrap();

        assert_eq!(results.len(), 2);
    }
}
