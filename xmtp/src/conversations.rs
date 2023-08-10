use std::time::Duration;

use diesel::Connection;
use prost::Message;
use vodozemac::olm::OlmMessage;
use xmtp_proto::xmtp::{
    message_api::v1::{Envelope, PublishRequest},
    v3::message_contents::{
        EdDsaSignature, InvitationV1, PadlockMessageEnvelope, PadlockMessageHeader,
        PadlockMessagePayload, PadlockMessagePayloadVersion, PadlockMessageSealedMetadata,
    },
};

use crate::{
    contact::Contact,
    conversation::{convo_id, peer_addr_from_convo_id, ConversationError, SecretConversation},
    invitation::Invitation,
    session::SessionManager,
    storage::{
        now, ConversationState, DbConnection, InboundInvite, InboundInviteStatus, MessageState,
        OutboundPayloadState, RefreshJob, RefreshJobKind, StorageError, StoredConversation,
        StoredMessage, StoredOutboundPayload, StoredSession, StoredUser,
    },
    types::networking::XmtpApiClient,
    utils::{base64_encode, build_installation_message_topic},
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

        match existing_session {
            Some(mut session_manager) => {
                let olm_message: OlmMessage = match serde_json::from_slice(&invitation.ciphertext) {
                    Ok(olm_message) => olm_message,
                    Err(err) => {
                        log::error!("Error deserializing olm message: {:?}", err);
                        return Ok(InboundInviteStatus::DecryptionFailure);
                    }
                };

                plaintext = match session_manager.decrypt(olm_message, conn) {
                    Ok(plaintext) => plaintext,
                    Err(err) => {
                        log::error!("Error decrypting olm message: {:?}", err);
                        return Ok(InboundInviteStatus::DecryptionFailure);
                    }
                };
            }
            None => {
                (_, plaintext) = match self.client.create_inbound_session(
                    conn,
                    &invitation.inviter,
                    &invitation.ciphertext,
                ) {
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

    fn create_outbound_payload(
        &self,
        session: &mut SessionManager,
        message: &StoredMessage,
    ) -> Result<StoredOutboundPayload, ConversationError> {
        let is_prekey_message = !session.has_received_message();

        let metadata = PadlockMessageSealedMetadata {
            sender_user_address: self.client.wallet_address(),
            sender_installation_id: self.client.account.contact().installation_id(),
            recipient_user_address: session.user_address(),
            recipient_installation_id: session.installation_id(),
            is_prekey_message,
        };
        // TODO encrypted sealed metadata using sealed sender
        let sealed_metadata = metadata.encode_to_vec();
        let message_header = PadlockMessageHeader {
            sent_ns: message.created_at as u64,
            sealed_metadata,
        };
        let header_bytes = message_header.encode_to_vec();
        // TODO expose a vmac method to sign bytes rather than string
        // https://matrix-org.github.io/vodozemac/vodozemac/olm/struct.Account.html#method.sign
        let header_signature = self.client.account.sign(&base64_encode(&header_bytes));
        let header_signature = EdDsaSignature {
            bytes: header_signature.to_bytes().to_vec(),
        };

        let payload = PadlockMessagePayload {
            message_version: PadlockMessagePayloadVersion::One as i32,
            header_signature: Some(header_signature),
            convo_id: message.convo_id.clone(),
            content_bytes: message.content.clone(),
        };
        let olm_message = session.encrypt(&payload.encode_to_vec());
        let ciphertext = match olm_message {
            OlmMessage::Normal(message) => message.to_bytes(),
            OlmMessage::PreKey(prekey_message) => prekey_message.to_bytes(),
        };

        let envelope = PadlockMessageEnvelope {
            header_bytes,
            ciphertext,
        };
        Ok(StoredOutboundPayload::new(
            message.created_at,
            build_installation_message_topic(&session.installation_id()),
            envelope.encode_to_vec(),
            OutboundPayloadState::Pending as i32,
            0,
        ))
    }

    pub async fn process_outbound_message(
        &self,
        message: &StoredMessage,
    ) -> Result<(), ConversationError> {
        let peer_address =
            peer_addr_from_convo_id(&message.convo_id, &self.client.wallet_address())?;
        self.client
            .refresh_user_installations_if_stale(&peer_address)
            .await?;
        self.client.store.conn().unwrap().transaction(
            |transaction| -> Result<(), ConversationError> {
                let my_sessions = self
                    .client
                    .store
                    .get_sessions(&self.client.wallet_address(), transaction)?;
                let their_user_addr =
                    peer_addr_from_convo_id(&message.convo_id, &self.client.wallet_address())?;
                let their_sessions = self
                    .client
                    .store
                    .get_sessions(&their_user_addr, transaction)?;
                if their_sessions.is_empty() {
                    return Err(ConversationError::NoSessions(their_user_addr));
                }

                let mut outbound_payloads = Vec::new();
                let mut updated_sessions = Vec::new();
                for stored_session in my_sessions.iter().chain(&their_sessions) {
                    if stored_session.peer_installation_id
                        == self.client.account.contact().installation_id()
                    {
                        continue;
                    }
                    let mut session = SessionManager::try_from(stored_session)?;
                    let outbound_payload = self.create_outbound_payload(&mut session, message)?;
                    let updated_session = StoredSession::try_from(&session)?;
                    outbound_payloads.push(outbound_payload);
                    updated_sessions.push(updated_session);
                }

                self.client.store.commit_outbound_payloads_for_message(
                    message.id,
                    MessageState::LocallyCommitted,
                    outbound_payloads,
                    updated_sessions,
                    transaction,
                )?;
                Ok(())
            },
        )?;

        Ok(())
    }

    pub async fn process_outbound_messages(&self) -> Result<(), ConversationError> {
        self.client
            .refresh_user_installations_if_stale(&self.client.wallet_address())
            .await?;
        let mut messages = self.client.store.get_unprocessed_messages()?;
        messages.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        for message in messages {
            if let Err(e) = self.process_outbound_message(&message).await {
                log::error!(
                    "Couldn't process message with ID {} because of error: {:?}",
                    message.id,
                    e
                );
                // TODO update message status to failed on non-retryable errors so that we don't retry it next time
            }
        }

        self.publish_outbound_payloads().await?;
        Ok(())
    }

    pub async fn publish_outbound_payloads(&self) -> Result<(), ConversationError> {
        let unsent_payloads = self.client.store.fetch_and_lock_outbound_payloads(
            OutboundPayloadState::Pending,
            Duration::from_secs(60).as_nanos() as i64,
        )?;

        if unsent_payloads.is_empty() {
            return Ok(());
        }

        let envelopes = unsent_payloads
            .iter()
            .map(|payload| Envelope {
                content_topic: payload.content_topic.clone(),
                timestamp_ns: payload.created_at_ns as u64,
                message: payload.payload.clone(),
            })
            .collect();

        // TODO: API tokens
        self.client
            .api_client
            .publish("".to_string(), PublishRequest { envelopes })
            .await?;

        let payload_ids = unsent_payloads
            .iter()
            .map(|payload| payload.created_at_ns)
            .collect();
        self.client.store.update_and_unlock_outbound_payloads(
            payload_ids,
            OutboundPayloadState::ServerAcknowledged,
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use prost::Message;
    use xmtp_proto::xmtp::message_api::v1::QueryRequest;

    use crate::{
        codecs::{text::TextCodec, ContentCodec},
        conversation::convo_id,
        conversations::Conversations,
        invitation::Invitation,
        mock_xmtp_api_client::MockXmtpApiClient,
        storage::{
            now, InboundInvite, InboundInviteStatus, MessageState, StoredConversation,
            StoredMessage, StoredUser,
        },
        test_utils::test_utils::{
            gen_test_client, gen_test_client_internal, gen_test_conversation, gen_two_test_clients,
        },
        types::networking::XmtpApiClient,
        utils::{build_envelope, build_installation_message_topic, build_user_invite_topic},
        ClientBuilder, Fetch,
    };

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
    async fn create_outbound_payload() {
        let alice_client = gen_test_client().await;
        let bob_client = gen_test_client().await;

        let conversations = Conversations::new(&alice_client);
        let mut session = alice_client
            .get_session(
                &mut alice_client.store.conn().unwrap(),
                &bob_client.account.contact(),
            )
            .unwrap();

        let _payload = conversations
            .create_outbound_payload(
                &mut session,
                &StoredMessage {
                    id: 0,
                    created_at: 0,
                    convo_id: convo_id(alice_client.wallet_address(), bob_client.wallet_address()),
                    addr_from: alice_client.wallet_address(),
                    content: TextCodec::encode("Hello world".to_string())
                        .unwrap()
                        .encode_to_vec(),
                    state: MessageState::Unprocessed as i32,
                },
            )
            .unwrap();

        // TODO validate the payload when implementing the receiver side
    }

    #[tokio::test]
    async fn process_outbound_messages() {
        let (alice_client, bob_client) = gen_two_test_clients().await;

        let conversations = Conversations::new(&alice_client);
        let conversation =
            gen_test_conversation(&conversations, &bob_client.wallet_address()).await;

        conversation.send_message("Hello world").unwrap();
        let unprocessed_messages = alice_client.store.get_unprocessed_messages().unwrap();
        assert_eq!(unprocessed_messages.len(), 1);

        conversations.process_outbound_messages().await.unwrap();
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

        let inbound_invites: Vec<InboundInvite> = conn.fetch().unwrap();
        assert_eq!(inbound_invites.len(), 1);
        assert!(inbound_invites[0].status == InboundInviteStatus::Processed as i16);

        let users: Vec<StoredUser> = conn.fetch().unwrap();
        // Expect 2 users because Bob is always in his own DB already
        assert_eq!(users.len(), 2);
        assert_eq!(users[1].user_address, alice_client.wallet_address());

        let conversations: Vec<StoredConversation> = conn.fetch().unwrap();
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

        let inbound_invites: Vec<InboundInvite> = conn.fetch().unwrap();
        assert_eq!(inbound_invites.len(), 1);
        assert!(inbound_invites[0].status == InboundInviteStatus::DecryptionFailure as i16);

        let users: Vec<StoredUser> = conn.fetch().unwrap();
        // Expect 1 user because Bob is always in his own DB already
        assert_eq!(users.len(), 1);

        let conversations: Vec<StoredConversation> = conn.fetch().unwrap();
        assert_eq!(conversations.len(), 0);
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
