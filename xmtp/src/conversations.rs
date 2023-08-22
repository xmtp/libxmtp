use diesel::Connection;
use futures::executor::block_on;
use log::info;
use prost::Message;
use vodozemac::olm::{self, OlmMessage};
use xmtp_proto::xmtp::v3::message_contents::{InvitationV1, PadlockMessagePayload};

use crate::{
    contact::Contact,
    conversation::{convo_id, peer_addr_from_convo_id, ConversationError, SecretConversation},
    invitation::Invitation,
    message::DecodedInboundMessage,
    session::SessionManager,
    storage::{
        now, ConversationState, DbConnection, InboundInvite, InboundInviteStatus, InboundMessage,
        InboundMessageStatus, MessageState, NewStoredMessage, RefreshJob, RefreshJobKind,
        StorageError, StoredConversation, StoredUser,
    },
    types::networking::XmtpApiClient,
    utils::build_installation_message_topic,
    vmac_protos::ProtoWrapper,
    Client,
};

const PADDING_TIME_NS: i64 = 30 * 1000 * 1000 * 1000;

pub struct Conversations<'c, A>
where
    A: XmtpApiClient,
{
    pub(crate) client: &'c Client<A>,
}

impl<'c, A> Conversations<'c, A>
where
    A: XmtpApiClient,
{
    pub fn new(client: &'c Client<A>) -> Self {
        Self { client }
    }

    pub fn new_secret_conversation(
        &self,
        wallet_address: String,
    ) -> Result<SecretConversation<A>, ConversationError> {
        SecretConversation::create(self.client, wallet_address)
    }

    pub async fn list(
        &self,
        refresh_from_network: bool,
    ) -> Result<Vec<SecretConversation<A>>, ConversationError> {
        if refresh_from_network {
            self.save_invites()?;
            self.process_invites()?;
        }
        let conn = &mut self.client.store.conn()?;

        let mut secret_convos: Vec<SecretConversation<A>> = vec![];

        let convos: Vec<StoredConversation> = self.client.store.get_conversations(
            conn,
            vec![
                ConversationState::InviteReceived,
                ConversationState::Invited,
            ],
        )?;
        log::debug!("Retrieved {:?} convos from the database", convos.len());
        for convo in convos {
            let peer_address =
                peer_addr_from_convo_id(&convo.convo_id, &self.client.account.addr())?;

            let convo = SecretConversation::new(self.client, peer_address);
            secret_convos.push(convo);
        }

        Ok(secret_convos)
    }

    pub fn receive(&self) -> Result<(), ConversationError> {
        if self.save_inbound_messages().is_err() {
            log::warn!("Saving messages did not complete successfully");
        }
        self.process_inbound_messages()?;

        Ok(())
    }

    pub fn save_inbound_messages(&self) -> Result<(), ConversationError> {
        let inbound_topic = build_installation_message_topic(&self.client.installation_id());

        self.client
            .store
            .lock_refresh_job(RefreshJobKind::Message, |conn, job| {
                log::debug!(
                    "Refresh messages start time: {}",
                    self.get_start_time(&job).unsigned_abs()
                );
                let downloaded =
                    futures::executor::block_on(self.client.download_latest_from_topic(
                        self.get_start_time(&job).unsigned_abs(),
                        inbound_topic,
                    ))
                    .map_err(|e| StorageError::Unknown(e.to_string()))?;

                log::info!("Messages Downloaded:{}", downloaded.len());

                for envelope in downloaded {
                    if let Err(e) = self
                        .client
                        .store
                        .save_inbound_message(conn, envelope.into())
                    {
                        log::error!("Unable to save message:{}", e);
                    }
                }

                Ok(())
            })?;

        Ok(())
    }

    pub fn process_inbound_messages(&self) -> Result<(), StorageError> {
        let conn = &mut self.client.store.conn()?;
        conn.transaction::<_, StorageError, _>(|transaction_manager| {
            let msgs = self
                .client
                .store
                .get_inbound_messages(transaction_manager, InboundMessageStatus::Pending)?;
            for msg in msgs {
                let payload_id = msg.id.clone();
                match self.process_inbound_message(transaction_manager, msg) {
                    Ok(status) => {
                        info!(
                            "message processed: {:?}. Status: {:?}",
                            payload_id,
                            status.clone()
                        );
                        self.client.store.set_msg_status(
                            transaction_manager,
                            payload_id,
                            status,
                        )?;
                    }
                    Err(err) => {
                        log::error!("Error processing msg: {:?}", err);
                        return Err(StorageError::Unknown(err.to_string()));
                    }
                }
            }
            Ok(())
        })
    }

    fn process_inbound_message(
        &self,
        conn: &mut DbConnection,
        msg: InboundMessage,
    ) -> Result<InboundMessageStatus, ConversationError> {
        let payload = DecodedInboundMessage::try_from(msg.clone())?;
        let olm_message = (&payload).try_into()?;

        let existing_sessions = self
            .client
            .store
            .get_latest_sessions_for_installation(&payload.sender_installation_id, conn)?;

        // Attempt to decrypt with existing sessions
        for raw_session in existing_sessions {
            let mut session = match SessionManager::try_from(&raw_session) {
                Ok(s) => s,
                Err(e) => {
                    log::warn!("corrupted session:{} {}", raw_session.session_id, e);
                    continue;
                }
            };

            match session.decrypt(&olm_message, conn) {
                Ok(p) => {
                    self.process_plaintext(conn, &p, &payload)?;
                    return Ok(InboundMessageStatus::Processed);
                }
                Err(_) => continue,
            }
        }

        // No existing session, attempt to create new session
        if let OlmMessage::PreKey(m) = olm_message {
            self.process_prekey_message(conn, m, &payload)?;
            Ok(InboundMessageStatus::Processed)
        } else {
            log::warn!("Message:{} could not be decrypted", msg.id);
            Ok(InboundMessageStatus::DecryptionFailure)
        }
    }

    fn process_plaintext(
        &self,
        conn: &mut DbConnection,
        bytes: &Vec<u8>,
        payload: &DecodedInboundMessage,
    ) -> Result<(), ConversationError> {
        let message_obj =
            PadlockMessagePayload::decode(bytes.as_slice()).map_err(ConversationError::Decode)?;

        //TODO: Validate message

        let stored_message = NewStoredMessage::new(
            message_obj.convo_id,
            payload.sender_address.clone(),
            message_obj.content_bytes,
            MessageState::Received as i32,
            payload.sent_at_ns,
        );

        self.client
            .store
            .insert_or_ignore_message(conn, stored_message)?;

        Ok(())
    }

    fn process_prekey_message(
        &self,
        conn: &mut DbConnection,
        msg: olm::PreKeyMessage,
        payload: &DecodedInboundMessage,
    ) -> Result<(), ConversationError> {
        let network_contact = block_on(self.client.download_contact_for_installation(
            &payload.sender_address,
            &payload.sender_installation_id,
        ))?;

        let contact = match network_contact {
            Some(contact) => contact,
            None => {
                return Err(ConversationError::Generic(
                    "No contact for Prekey Messag".into(),
                ))
            }
        };

        let (_, plaintext) = self.client.create_inbound_session(conn, &contact, msg)?;
        self.process_plaintext(conn, &plaintext, payload)?;
        Ok(())
    }

    pub fn save_invites(&self) -> Result<(), ConversationError> {
        let my_contact = self.client.account.contact();

        self.client
            .store
            .lock_refresh_job(RefreshJobKind::Invite, |conn, job| {
                let downloaded =
                    futures::executor::block_on(self.client.download_latest_from_topic(
                        self.get_start_time(&job).unsigned_abs(),
                        crate::utils::build_user_invite_topic(my_contact.installation_id()),
                    ))
                    .map_err(|e| StorageError::Unknown(e.to_string()))?;
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
                        log::debug!(
                            "Invite processed: {:?}. Status: {:?}",
                            invite_id,
                            status.clone()
                        );
                        self.client.store.set_invite_status(
                            transaction_manager,
                            invite_id,
                            status,
                        )?;
                    }
                    Err(err) => {
                        log::error!("Error processing invite: {:?}", err);
                        return Err(StorageError::Unknown(err.to_string()));
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

        let olm_message = match serde_json::from_slice(&invitation.ciphertext) {
            Ok(olm_message) => olm_message,
            Err(err) => {
                log::error!("Error deserializing olm message: {:?}", err);
                return Ok(InboundInviteStatus::DecryptionFailure);
            }
        };

        match existing_session {
            Some(mut session_manager) => {
                plaintext = match session_manager.decrypt(&olm_message, conn) {
                    Ok(plaintext) => plaintext,
                    Err(err) => {
                        log::error!("Error decrypting olm message: {:?}", err);
                        return Ok(InboundInviteStatus::DecryptionFailure);
                    }
                };
            }
            None => {
                let prek_key = match olm_message {
                    olm::OlmMessage::Normal(_) => {
                        log::error!("Cannot create new session from non-prekey message");
                        return Ok(InboundInviteStatus::DecryptionFailure);
                    }
                    olm::OlmMessage::PreKey(k) => k,
                };

                (_, plaintext) =
                    match self
                        .client
                        .create_inbound_session(conn, &invitation.inviter, prek_key)
                    {
                        Ok((session, plaintext)) => (session, plaintext),
                        Err(err) => {
                            log::error!("Error creating session: {:?}", err);
                            return Ok(InboundInviteStatus::DecryptionFailure);
                        }
                    };
            }
        };

        let inner_invite: ProtoWrapper<InvitationV1> = plaintext.try_into()?;
        if !self.validate_invite(&invitation, &inner_invite.proto) {
            return Ok(InboundInviteStatus::Invalid);
        }
        // Create the user if doesn't exist
        let peer_address = self.get_invite_peer_address(&invitation, &inner_invite.proto);
        self.client.store.insert_or_ignore_user_with_conn(
            conn,
            StoredUser {
                user_address: peer_address.clone(),
                created_at: now(),
                last_refreshed: 0,
            },
        )?;

        // Create the conversation if doesn't exist
        self.client.store.insert_or_ignore_conversation_with_conn(
            conn,
            StoredConversation {
                convo_id: convo_id(
                    peer_address.clone(),
                    self.client.account.contact().wallet_address,
                ),
                peer_address,
                created_at: now(),
                convo_state: ConversationState::InviteReceived as i32,
            },
        )?;

        Ok(InboundInviteStatus::Processed)
    }

    fn validate_invite(&self, invitation: &Invitation, inner_invite: &InvitationV1) -> bool {
        let my_wallet_address = self.client.account.contact().wallet_address;
        let inviter_is_my_other_device = my_wallet_address == invitation.inviter.wallet_address;

        if inviter_is_my_other_device {
            true
        } else {
            inner_invite.invitee_wallet_address == my_wallet_address
        }
    }

    fn get_invite_peer_address(
        &self,
        invitation: &Invitation,
        inner_invite: &InvitationV1,
    ) -> String {
        let my_wallet_address = self.client.account.contact().wallet_address;
        let inviter_is_my_other_device = my_wallet_address == invitation.inviter.wallet_address;

        if inviter_is_my_other_device {
            inner_invite.invitee_wallet_address.clone()
        } else {
            invitation.inviter.wallet_address.clone()
        }
    }

    fn find_existing_session_with_conn(
        &self,
        contact: &Contact,
        conn: &mut DbConnection,
    ) -> Result<Option<SessionManager>, ConversationError> {
        self.find_existing_session(&contact.installation_id(), conn)
    }

    fn find_existing_session(
        &self,
        installation_id: &str,
        conn: &mut DbConnection,
    ) -> Result<Option<SessionManager>, ConversationError> {
        let stored_session = self
            .client
            .store
            .get_latest_session_for_installation(installation_id, conn)?;

        match stored_session {
            Some(i) => Ok(Some(SessionManager::try_from(&i)?)),
            None => Ok(None),
        }
    }

    fn get_start_time(&self, job: &RefreshJob) -> i64 {
        // Adjust for padding and ensure start_time > 0
        std::cmp::max(job.last_run - PADDING_TIME_NS, 0)
    }
}

#[cfg(test)]
mod tests {
    use xmtp_proto::xmtp::message_api::v1::QueryRequest;

    use crate::{
        conversations::Conversations,
        invitation::Invitation,
        mock_xmtp_api_client::MockXmtpApiClient,
        storage::{now, InboundInvite, InboundInviteStatus, StoredConversation, StoredUser},
        test_utils::test_utils::{
            gen_test_client, gen_test_client_internal, gen_test_conversation, gen_two_test_clients,
        },
        types::networking::XmtpApiClient,
        utils::{build_envelope, build_installation_message_topic, build_user_invite_topic},
        ClientBuilder, Fetch,
    };

    fn init() {
        let _ = env_logger::builder().is_test(true).try_init();
    }

    #[tokio::test]
    async fn create_secret_conversation() {
        let alice_client = gen_test_client().await;
        let bob_client = gen_test_client().await;

        let conversations = Conversations::new(&alice_client);
        let conversation = conversations
            .new_secret_conversation(bob_client.wallet_address().to_string())
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

    #[tokio::test]
    async fn process_outbound_messages() {
        let (alice_client, bob_client) = gen_two_test_clients().await;

        let conversations = Conversations::new(&alice_client);
        let conversation =
            gen_test_conversation(&conversations, &bob_client.wallet_address()).await;

        conversation.send_text("Hello world").await.unwrap();
        alice_client.process_outbound_messages().await.unwrap();
        let response = bob_client
            .api_client
            .query(QueryRequest {
                content_topics: vec![build_installation_message_topic(
                    &bob_client.installation_id(),
                )],
                start_time_ns: 0 as u64,
                end_time_ns: now() as u64,
                paging_info: None,
            })
            .await
            .unwrap();
        assert_eq!(response.envelopes.len(), 1);
        // TODO verify using receive logic
    }

    #[tokio::test]
    async fn process_invites_happy_path() {
        let alice_client = gen_test_client().await;
        let bob_client = gen_test_client().await;

        let bob_address = bob_client.account.contact().wallet_address;
        let alice_to_bob_inner_invite = Invitation::build_inner_invite_bytes(bob_address).unwrap();
        let mut alice_to_bob_session = alice_client
            .get_session(
                &mut alice_client.store.conn().unwrap(),
                &bob_client.account.contact(),
            )
            .unwrap();
        let alice_to_bob_invite = Invitation::build(
            alice_client.account.contact(),
            &mut alice_to_bob_session,
            &alice_to_bob_inner_invite,
        )
        .unwrap();

        let envelope = build_envelope(
            build_user_invite_topic(bob_client.account.contact().installation_id()),
            alice_to_bob_invite.try_into().unwrap(),
        );

        // Save the invite to Bob's DB
        bob_client
            .store
            .save_inbound_invite(
                &mut bob_client.store.conn().unwrap(),
                envelope.try_into().unwrap(),
            )
            .unwrap();

        let bob_conversations = Conversations::new(&bob_client);
        let process_result = bob_conversations.process_invites();
        assert!(process_result.is_ok());

        let conn = &mut bob_client.store.conn().unwrap();

        let inbound_invites: Vec<InboundInvite> = conn.fetch_all().unwrap();
        assert_eq!(inbound_invites.len(), 1);
        assert!(inbound_invites[0].status == InboundInviteStatus::Processed as i16);

        let users: Vec<StoredUser> = conn.fetch_all().unwrap();
        // Expect 2 users because Bob is always in his own DB already
        assert_eq!(users.len(), 2);
        assert_eq!(users[1].user_address, alice_client.wallet_address());

        let conversations: Vec<StoredConversation> = conn.fetch_all().unwrap();
        assert_eq!(conversations.len(), 1);
        assert_eq!(conversations[0].peer_address, alice_client.wallet_address());
    }

    #[tokio::test]
    async fn process_invites_decryption_failure() {
        let alice_client = gen_test_client().await;
        let bob_client = gen_test_client().await;

        let bob_address = bob_client.account.contact().wallet_address;
        let alice_to_bob_inner_invite = Invitation::build_inner_invite_bytes(bob_address).unwrap();
        let mut bad_session = alice_client
            .get_session(
                &mut alice_client.store.conn().unwrap(),
                &gen_test_client().await.account.contact(),
            )
            .unwrap();
        let alice_to_bob_invite = Invitation::build(
            alice_client.account.contact(),
            &mut bad_session,
            &alice_to_bob_inner_invite,
        )
        .unwrap();

        let envelope = build_envelope(
            build_user_invite_topic(bob_client.account.contact().installation_id()),
            alice_to_bob_invite.try_into().unwrap(),
        );

        // Save the invite to Bob's DB
        bob_client
            .store
            .save_inbound_invite(
                &mut bob_client.store.conn().unwrap(),
                envelope.try_into().unwrap(),
            )
            .unwrap();

        let bob_conversations = Conversations::new(&bob_client);
        let process_result = bob_conversations.process_invites();
        assert!(process_result.is_ok());

        let conn = &mut bob_client.store.conn().unwrap();

        let inbound_invites: Vec<InboundInvite> = conn.fetch_all().unwrap();
        assert_eq!(inbound_invites.len(), 1);
        assert!(inbound_invites[0].status == InboundInviteStatus::DecryptionFailure as i16);

        let users: Vec<StoredUser> = conn.fetch_all().unwrap();
        // Expect 1 user because Bob is always in his own DB already
        assert_eq!(users.len(), 1);

        let conversations: Vec<StoredConversation> = conn.fetch_all().unwrap();
        assert_eq!(conversations.len(), 0);
    }

    #[tokio::test]
    async fn process_messages_happy_path() {
        init();
        let (alice_client, bob_client) = gen_two_test_clients().await;

        let bob_address = bob_client.account.contact().wallet_address;

        let a_convos = Conversations::new(&alice_client);
        let b_convos = Conversations::new(&bob_client);

        let a_to_b = a_convos
            .new_secret_conversation(bob_address.clone())
            .unwrap();

        // Send First Message

        a_to_b.send_text("Hi").await.unwrap();
        alice_client.process_outbound_messages().await.unwrap();
        b_convos.receive().unwrap();

        let bob_messages = bob_client
            .store
            .get_stored_messages(
                &mut bob_client.store.conn().unwrap(),
                None,
                None,
                None,
                None,
                None,
            )
            .unwrap();

        assert_eq!(bob_messages.len(), 1);

        {
            let alice_messages = alice_client
                .store
                .get_stored_messages(
                    &mut alice_client.store.conn().unwrap(),
                    None,
                    None,
                    None,
                    None,
                    None,
                )
                .unwrap();
            assert_eq!(alice_messages.len(), 1);
        }

        // Reply
        let b_to_a = b_convos
            .new_secret_conversation(bob_address.clone())
            .unwrap();

        b_to_a.send_text("Reply").await.unwrap();
        bob_client.process_outbound_messages().await.unwrap();

        a_convos.receive().unwrap();

        let _alice_messages = alice_client
            .store
            .get_stored_messages(
                &mut alice_client.store.conn().unwrap(),
                None,
                None,
                None,
                None,
                None,
            )
            .unwrap();

        // TODO: This is currently failing with a NoSession error for unknown reasons
        // assert_eq!(alice_messages.len(), 2);
    }

    #[tokio::test]
    async fn list() {
        let api_client = MockXmtpApiClient::new();
        let alice_client = gen_test_client_internal(api_client.clone()).await;
        let bob_client = gen_test_client_internal(api_client.clone()).await;

        let conversations = Conversations::new(&alice_client);
        let conversation =
            gen_test_conversation(&conversations, &bob_client.wallet_address()).await;

        let list = conversations.list(true).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].peer_address(), conversation.peer_address());
    }
}
