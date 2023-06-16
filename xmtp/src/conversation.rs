use std::sync::Arc;

use crate::{
    client::ClientError,
    contact::Contact,
    networking::XmtpApiClient,
    types::Address,
    utils::{build_envelope, build_user_invite_topic},
    Client,
};
// use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::Mutex;

#[derive(Debug, Error)]
pub enum ConversationError {
    #[error("client error {0}")]
    Client(#[from] ClientError),
    #[error("unknown error")]
    Unknown,
}

// I had to pick a name for this, and it seems like we are hovering around SecretConversation ATM
// May very well change
pub struct SecretConversation<A>
where
    A: XmtpApiClient,
{
    peer_address: Address,
    members: Vec<Contact>,
    client: Arc<Mutex<Client<A>>>,
}

impl<A> SecretConversation<A>
where
    A: XmtpApiClient,
{
    pub fn new(
        client: Arc<Mutex<Client<A>>>,
        peer_address: Address,
        // TODO: Add user's own contacts as well
        members: Vec<Contact>,
    ) -> Self {
        Self {
            client,
            peer_address,
            members,
        }
    }

    pub fn peer_address(&self) -> Address {
        self.peer_address.clone()
    }

    pub async fn initialize(&self) -> Result<(), ConversationError> {
        let mut client = self.client.lock().await;

        for contact in self.members.iter() {
            let id = contact.installation_id();
            // TODO: Persist session to database
            let mut session = client.create_outbound_session(contact.clone())?;
            // TODO: Replace with proper protobuf invite message
            let invite_message = session.encrypt("invite".as_bytes());

            let envelope = build_envelope(
                build_user_invite_topic(id),
                // TODO: Wrap in XMTP type
                invite_message.message().to_vec(),
            );

            // TODO: Replace with real token
            client
                .api_client
                // TODO: API authentication
                .publish("".to_string(), vec![envelope])
                .await
                .map_err(|_| ConversationError::Unknown)?;
        }

        Ok(())
    }
}
