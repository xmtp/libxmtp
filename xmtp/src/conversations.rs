use diesel::Connection;
use vodozemac::olm::OlmMessage;
use xmtp_proto::xmtp::v3::message_contents::InvitationV1;

use crate::{
    contact::Contact,
    conversation::{ConversationError, SecretConversation},
    invitation::Invitation,
    session::SessionManager,
    storage::{
        DbConnection, InboundInvite, InboundInviteStatus, RefreshJob, RefreshJobKind, StorageError,
    },
    types::networking::XmtpApiClient,
    vmac_protos::ProtoWrapper,
    Client,
};

const PADDING_TIME_NS: i64 = 30 * 1000 * 1000 * 1000;

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
        SecretConversation::new(self.client, wallet_address, contacts)
    }

    pub fn save_invites(&self) -> Result<(), ConversationError> {
        let my_contact = self.client.account.contact();

        self.client
            .store
            .lock_refresh_job(RefreshJobKind::Invite, |conn, job| {
                let downloaded =
                    futures::executor::block_on(self.client.download_latest_from_topic(
                        self.get_start_time(job).unsigned_abs(),
                        crate::utils::build_user_invite_topic(my_contact.installation_id()),
                    ))
                    .map_err(|_| StorageError::Unknown)?;
                // Save all invites
                for envelope in downloaded {
                    self.client
                        .store
                        .save_inbound_invite(conn, envelope.into())?;
                }

                Ok(())
            })?;

        Ok(())
    }

    pub fn process_invites(&self) -> Result<(), ConversationError> {
        let conn = &mut self.client.store.conn()?;
        conn.transaction::<_, StorageError, _>(|transaction_manager| {
            let invites = self
                .client
                .store
                .get_inbound_invites(transaction_manager, InboundInviteStatus::Pending)?;
            for invite in invites {
                let invite_id = invite.id.clone();
                match self.process_inbound_invite(transaction_manager, invite) {
                    Ok(status) => {
                        self.client.store.set_invite_status(
                            transaction_manager,
                            invite_id,
                            status,
                        )?;
                    }
                    Err(err) => {
                        log::error!("Error processing invite: {:?}", err);
                        return Err(StorageError::Unknown);
                    }
                }
            }

            Ok(())
        })?;

        Ok(())
    }

    fn process_inbound_invite(
        &self,
        conn: &mut DbConnection,
        invite: InboundInvite,
    ) -> Result<InboundInviteStatus, ConversationError> {
        let invitation: Invitation = match invite.payload.try_into() {
            Ok(invitation) => invitation,
            Err(_) => {
                return Ok(InboundInviteStatus::Invalid);
            }
        };

        let existing_session = self.find_existing_session_with_conn(&invitation.inviter, conn)?;
        let plaintext: Vec<u8>;

        match existing_session {
            Some(mut session_manager) => {
                let olm_message: OlmMessage =
                    match serde_json::from_slice(&invitation.ciphertext.as_slice()) {
                        Ok(olm_message) => olm_message,
                        Err(_) => {
                            return Ok(InboundInviteStatus::DecryptionFailure);
                        }
                    };

                plaintext = match session_manager.decrypt(olm_message, conn) {
                    Ok(plaintext) => plaintext,
                    Err(_) => {
                        return Ok(InboundInviteStatus::DecryptionFailure);
                    }
                };
            }
            None => {
                (_, plaintext) = match self
                    .client
                    .create_inbound_session(&invitation.inviter, &invitation.ciphertext)
                {
                    Ok((session, plaintext)) => (session, plaintext),
                    Err(_) => {
                        return Ok(InboundInviteStatus::DecryptionFailure);
                    }
                };
            }
        };

        let inner_invite: ProtoWrapper<InvitationV1> = plaintext.try_into()?;
        if !self.validate_invite(&invitation, &inner_invite.proto) {
            return Ok(InboundInviteStatus::Invalid);
        }

        Ok(InboundInviteStatus::Processed)
    }

    fn validate_invite(&self, invitation: &Invitation, inner_invite: &InvitationV1) -> bool {
        let my_wallet_address = self.client.account.contact().wallet_address;
        let inviter_is_my_other_device = my_wallet_address == invitation.inviter.wallet_address;

        if inviter_is_my_other_device {
            return true;
        } else {
            return inner_invite.invitee_wallet_address != my_wallet_address;
        }
    }

    fn find_existing_session_with_conn(
        &self,
        contact: &Contact,
        conn: &mut DbConnection,
    ) -> Result<Option<SessionManager>, ConversationError> {
        let stored_session = self
            .client
            .store
            .get_session_with_conn(contact.installation_id().as_str(), conn)?;
        match stored_session {
            Some(i) => Ok(Some(SessionManager::try_from(&i)?)),
            None => Ok(None),
        }
    }

    fn get_start_time(&self, job: RefreshJob) -> i64 {
        // Adjust for padding and ensure start_time > 0
        std::cmp::max(job.last_run - PADDING_TIME_NS, 0)
    }
}

#[cfg(test)]
mod tests {
    use crate::{conversations::Conversations, ClientBuilder};

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
    async fn save_invites() {
        let mut alice_client = ClientBuilder::new_test().build().unwrap();
        alice_client.init().await.unwrap();

        let invites = Conversations::new(&alice_client).save_invites();
        assert!(invites.is_ok());
    }
}
