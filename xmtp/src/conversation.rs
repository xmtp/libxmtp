use crate::{
    client::ClientError,
    contact::Contact,
    invitation::{Invitation, InvitationError},
    types::networking::PublishRequest,
    types::networking::XmtpApiClient,
    types::Address,
    utils::{build_envelope, build_user_invite_topic},
    Client,
};

use prost::DecodeError;
// use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConversationError {
    #[error("client error {0}")]
    Client(#[from] ClientError),
    #[error("invitation error {0}")]
    Invitation(#[from] InvitationError),
    #[error("decode error {0}")]
    Decode(DecodeError),
    #[error("unknown error")]
    Unknown,
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
    pub fn new(
        client: &'c Client<A>,
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
