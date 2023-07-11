use crate::{
    client::ClientError,
    contact::Contact,
    invitation::{Invitation, InvitationError},
    storage::{now, ConversationState, StorageError, StoredConversation, StoredUser},
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
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
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
        let mut members = [self.client.account.addr(), self.peer_address()];
        members.sort();
        format!(":{}:{}", members[0], members[1])
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
