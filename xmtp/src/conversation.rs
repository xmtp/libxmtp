use crate::{
    api_utils::get_contacts,
    app_context::AppContext,
    client::ClientError,
    contact::Contact,
    invitation::{Invitation, InvitationError},
    session::SessionManager,
    storage::{
        now, ConversationState, MessageState, NewStoredMessage, StorageError, StoredConversation,
        StoredUser,
    },
    types::networking::PublishRequest,
    types::networking::XmtpApiClient,
    types::Address,
    utils::{build_envelope, build_user_contact_topic, build_user_invite_topic},
    Store,
};

use prost::DecodeError;
// use async_trait::async_trait;
use thiserror::Error;
use xmtp_proto::xmtp::message_api::v1::QueryRequest;

#[derive(Debug, Error)]
pub enum ConversationError {
    #[error("client error {0}")]
    Client(#[from] ClientError),
    #[error("invitation error {0}")]
    Invitation(#[from] InvitationError),
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
    app_context: &'c AppContext<A>,
}

impl<'c, A> SecretConversation<'c, A>
where
    A: XmtpApiClient,
{
    pub(crate) async fn new(
        app_context: &'c AppContext<A>,
        peer_address: Address,
    ) -> Result<SecretConversation<'c, A>, ConversationError> {
        // TODO: Add user's own contacts as well
        let members = get_contacts(app_context, peer_address.as_str()).await?;
        let obj = SecretConversation {
            app_context,
            peer_address: peer_address.clone(),
            members,
        };
        obj.app_context.store.insert_or_ignore_user(StoredUser {
            user_address: obj.peer_address(),
            created_at: now(),
            last_refreshed: 0,
        })?;
        obj.app_context
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
        convo_id(self.app_context.account.addr(), self.peer_address())
    }

    pub fn peer_address(&self) -> Address {
        self.peer_address.clone()
    }

    pub fn send_message(&self, text: &str) -> Result<(), ConversationError> {
        NewStoredMessage::new(
            self.convo_id(),
            self.app_context.account.addr(),
            text.as_bytes().to_vec(),
            MessageState::Unprocessed as i32,
        )
        .store(&self.app_context.store)?;
        Ok(())
    }

    pub async fn initialize(&self) -> Result<(), ConversationError> {
        let inner_invite_bytes = Invitation::build_inner_invite_bytes(self.peer_address.clone())?;
        for contact in self.members.iter() {
            let id = contact.installation_id();

            // TODO: Persist session to database
            let session = self.create_outbound_session(contact)?;
            let invitation = Invitation::build(
                self.app_context.account.contact(),
                session,
                &inner_invite_bytes,
            )?;

            let envelope = build_envelope(build_user_invite_topic(id), invitation.try_into()?);

            // TODO: Replace with real token
            self.app_context
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

    pub fn get_session(&self, contact: &Contact) -> Result<SessionManager, ClientError> {
        let existing_session = self
            .app_context
            .store
            .get_session(&contact.installation_id())?;
        match existing_session {
            Some(i) => Ok(SessionManager::try_from(&i)?),
            None => self.create_outbound_session(contact),
        }
    }

    pub fn create_outbound_session(
        &self,
        contact: &Contact,
    ) -> Result<SessionManager, ClientError> {
        let olm_session = self.app_context.account.create_outbound_session(contact);
        let session = SessionManager::from_olm_session(olm_session, contact)
            .map_err(|_| ClientError::Unknown)?;

        session.store(&self.app_context.store)?;

        Ok(session)
    }
}

#[cfg(test)]
mod tests {
    use crate::test_utils::test_utils::{gen_test_client, gen_test_conversation};

    #[tokio::test]
    async fn test_local_conversation_creation() {
        let client = gen_test_client().await;
        let peer_address = "0x000";
        let convo_id = format!(":{}:{}", peer_address, client.wallet_address());
        assert!(client
            .app_context
            .store
            .get_conversation(&convo_id)
            .unwrap()
            .is_none());

        let conversation = gen_test_conversation(&client, peer_address).await;
        assert!(conversation.peer_address() == peer_address);
        assert!(client
            .app_context
            .store
            .get_conversation(&convo_id)
            .unwrap()
            .is_some());
    }

    #[tokio::test]
    async fn test_send_message() {
        let client = gen_test_client().await;
        let conversation = gen_test_conversation(&client, "0x000").await;
        conversation.send_message("Hello, world!").unwrap();

        let message = &client.app_context.store.get_unprocessed_messages().unwrap()[0];
        assert!(message.content == "Hello, world!".as_bytes().to_vec());
    }
}
