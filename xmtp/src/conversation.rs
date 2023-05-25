use crate::{
    client::ClientError,
    contact::Contact,
    networking::XmtpApiClient,
    persistence::Persistence,
    types::Address,
    utils::{build_envelope, build_user_invite_topic},
    Client,
};
use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConversationError {
    #[error("unknown error")]
    Unknown,
}

#[async_trait]
pub trait OneToOneConversation<A, P, S>
where
    A: XmtpApiClient,
    P: Persistence,
{
    fn peer_address(&self) -> Address;
    async fn initialize(&self, client: Client<A, P, S>) -> Result<(), ClientError>;
    // fn send_message(&self, client: &Client<A, P, S>, message: Vec<u8>) -> Result<(), ClientError>;
}

// I had to pick a name for this, and it seems like we are hovering around SecretConversation ATM
// May very well change
pub struct SecretConversation {
    peer_address: Address,
    members: Vec<Contact>,
}

impl SecretConversation {
    pub fn new(peer_address: Address, members: Vec<Contact>) -> Self {
        Self {
            peer_address,
            members,
        }
    }
}

#[async_trait]
impl<'a, A, P, S> OneToOneConversation<A, P, S> for SecretConversation
where
    A: XmtpApiClient,
    P: Persistence,
{
    fn peer_address(&self) -> Address {
        self.peer_address.clone()
    }

    async fn initialize(&self, client: Client<A, P, S>) -> Result<(), ConversationError> {
        for contact in self.members {
            let id = contact.id();
            let mut session = client.account.create_outbound_session(contact);
            let invite_message = session.encrypt("invite".as_bytes());
            let envelope = build_envelope(
                build_user_invite_topic(id),
                // TODO: Wrap in XMTP type
                invite_message.message().to_vec(),
            );
            // TODO: Replace with real token
            client
                .api_client
                .publish("".to_string(), vec![envelope])
                .await
                .map_err(|_| ConversationError::Unknown)?;
        }

        Ok(())
    }
}
