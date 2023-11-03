use prost::{DecodeError, Message};
// use async_trait::async_trait;
use thiserror::Error;
use xmtp_proto::api_client::XmtpApiClient;

use crate::{
    client::ClientError,
    codecs::{text::TextCodec, CodecError, ContentCodec},
    conversations::Conversations,
    message::PayloadError,
    session::SessionError,
    storage::{
        now, DbConnection, MessageState, NewStoredMessage, StorageError, StoredConversation,
        StoredMessage, StoredUser,
    },
    types::Address,
    Client, Store,
};

#[derive(Debug, Error)]
pub enum ConversationError {
    #[error("client error {0}")]
    Client(#[from] ClientError),
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
    Networking(#[from] xmtp_proto::api_client::Error),
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

pub struct Conversation<'c, A>
where
    A: XmtpApiClient,
{
    peer_address: Address,
    client: &'c Client<A>,
}

impl<'c, A> Conversation<'c, A>
where
    A: XmtpApiClient,
{
    // Instantiate the conversation and insert all the necessary records into the database
    pub fn new(client: &'c Client<A>, peer_address: Address) -> Result<Self, ConversationError> {
        let obj = Self {
            client,
            peer_address,
        };
        let conn = &mut client.store.conn()?;
        // TODO: lazy create conversation on message insertion
        Conversation::ensure_conversation_exists(client, conn, &obj.convo_id())?;
        Ok(obj)
    }

    pub fn ensure_conversation_exists(
        client: &'c Client<A>,
        conn: &mut DbConnection,
        convo_id: &str,
    ) -> Result<(), ConversationError> {
        let peer_address = peer_addr_from_convo_id(convo_id, &client.wallet_address())?;
        let created_at = now();
        client.store.insert_user(
            conn,
            StoredUser {
                user_address: peer_address.clone(),
                created_at,
                last_refreshed: 0,
            },
        )?;

        client.store.insert_conversation(
            conn,
            StoredConversation {
                peer_address,
                convo_id: convo_id.to_string(),
                created_at,
            },
        )?;
        Ok(())
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

        if let Err(err) = Conversations::process_outbound_messages(self.client).await {
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
}

#[cfg(test)]
mod tests {
    use prost::Message;
    use xmtp_proto::xmtp::message_contents::EncodedContent;

    use crate::{
        codecs::{text::TextCodec, ContentCodec},
        conversation::ListMessagesOptions,
        conversations::Conversations,
        test_utils::test_utils::{gen_test_client, gen_test_conversation, gen_two_test_clients},
    };

    #[tokio::test]
    async fn test_local_conversation_creation() {
        let client = gen_test_client().await;
        let peer_address = "0x000";
        let convo_id = format!(":{}:{}", peer_address, client.wallet_address());
        assert!(client
            .store
            .get_conversation(&mut client.store.conn().unwrap(), &convo_id)
            .unwrap()
            .is_none());

        let conversation = gen_test_conversation(&client, peer_address).await;
        assert!(conversation.peer_address() == peer_address);
        assert!(client
            .store
            .get_conversation(&mut client.store.conn().unwrap(), &convo_id)
            .unwrap()
            .is_some());
    }

    #[tokio::test]
    async fn test_send_text() {
        let client = gen_test_client().await;
        let conversation = gen_test_conversation(&client, "0x000").await;
        conversation.send_text("Hello, world!").await.unwrap();

        let message = &client
            .store
            .get_unprocessed_messages(&mut client.store.conn().unwrap())
            .unwrap()[0];
        let content = EncodedContent::decode(&message.content[..]).unwrap();
        assert!(TextCodec::decode(content).unwrap() == "Hello, world!");
    }

    #[tokio::test]
    async fn test_list_messages() {
        let (client, recipient) = gen_two_test_clients().await;
        let conversation = gen_test_conversation(&client, recipient.account.addr().as_str()).await;
        conversation.send_text("Hello, world!").await.unwrap();
        conversation.send_text("Hello, again").await.unwrap();

        Conversations::process_outbound_messages(&client)
            .await
            .unwrap();

        let results = conversation
            .list_messages(&ListMessagesOptions::default())
            .await
            .unwrap();

        assert_eq!(results.len(), 2);
    }
}
