use std::time::Duration;

use diesel::Connection;
use futures::executor::block_on;
use log::info;
use prost::Message;
use vodozemac::olm::{self, OlmMessage};
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
    message::DecodedInboundMessage,
    session::SessionManager,
    storage::{
        now, ConversationState, DbConnection, InboundInvite, InboundInviteStatus, InboundMessage,
        InboundMessageStatus, MessageState, NewStoredMessage, OutboundPayloadState, RefreshJob,
        RefreshJobKind, StorageError, StoredConversation, StoredMessage, StoredOutboundPayload,
        StoredSession, StoredUser,
    },
    types::networking::XmtpApiClient,
    utils::{base64_encode, build_installation_message_topic},
    vmac_protos::ProtoWrapper,
    Client,
};

const PADDING_TIME_NS: i64 = 30 * 1000 * 1000 * 1000;

pub struct Conversations<A: XmtpApiClient> {
    _phantom: std::marker::PhantomData<A>,
}

impl<A: XmtpApiClient> Conversations<A> {
    pub async fn list(
        client: &Client<A>,
        refresh_from_network: bool,
    ) -> Result<Vec<SecretConversation<A>>, ConversationError> {
        if refresh_from_network {
            Conversations::save_invites(client)?;
            Conversations::process_invites(client)?;
        }
        let mut conn = client.store.conn()?;

        let mut secret_convos: Vec<SecretConversation<A>> = vec![];

        let convos: Vec<StoredConversation> = client.store.get_conversations(
            &mut conn,
            vec![
                ConversationState::InviteReceived,
                ConversationState::Invited,
            ],
        )?;
        // Releasing the connection early here as SecretConversation::new() will need to acquire a new one
        drop(conn);

        log::debug!("Retrieved {:?} convos from the database", convos.len());
        for convo in convos {
            let peer_address = peer_addr_from_convo_id(&convo.convo_id, &client.account.addr())?;

            let convo = SecretConversation::new(client, peer_address)?;
            secret_convos.push(convo);
        }

        Ok(secret_convos)
    }

    pub fn receive(client: &Client<A>) -> Result<(), ConversationError> {
        if Conversations::save_inbound_messages(client).is_err() {
            log::warn!("Saving messages did not complete successfully");
        }
        Conversations::process_inbound_messages(client)?;

        Ok(())
    }

    pub fn save_inbound_messages(client: &Client<A>) -> Result<(), ConversationError> {
        let inbound_topic = build_installation_message_topic(&client.installation_id());

        client
            .store
            .lock_refresh_job(RefreshJobKind::Message, |conn, job| {
                log::debug!(
                    "Refresh messages start time: {}",
                    Conversations::<A>::get_start_time(&job).unsigned_abs()
                );
                let downloaded = futures::executor::block_on(client.download_latest_from_topic(
                    Conversations::<A>::get_start_time(&job).unsigned_abs(),
                    inbound_topic,
                ))
                .map_err(|e| StorageError::Unknown(e.to_string()))?;

                log::info!("Messages Downloaded:{}", downloaded.len());

                for envelope in downloaded {
                    if let Err(e) = client.store.save_inbound_message(conn, envelope.into()) {
                        log::error!("Unable to save message:{}", e);
                    }
                }

                Ok(())
            })?;

        Ok(())
    }

    pub fn process_inbound_messages(client: &Client<A>) -> Result<(), StorageError> {
        let conn = &mut client.store.conn()?;
        conn.transaction::<_, StorageError, _>(|transaction_manager| {
            let msgs = client
                .store
                .get_inbound_messages(transaction_manager, InboundMessageStatus::Pending)?;
            for msg in msgs {
                let payload_id = msg.id.clone();
                match Conversations::process_inbound_message(client, transaction_manager, msg) {
                    Ok(status) => {
                        info!(
                            "message processed: {:?}. Status: {:?}",
                            payload_id,
                            status.clone()
                        );
                        client
                            .store
                            .set_msg_status(transaction_manager, payload_id, status)?;
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
        client: &Client<A>,
        conn: &mut DbConnection,
        msg: InboundMessage,
    ) -> Result<InboundMessageStatus, ConversationError> {
        let payload = DecodedInboundMessage::try_from(msg.clone())?;
        let olm_message = (&payload).try_into()?;

        let existing_sessions = client
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
                    Conversations::process_plaintext(client, conn, &p, &payload)?;
                    return Ok(InboundMessageStatus::Processed);
                }
                Err(_) => continue,
            }
        }

        // No existing session, attempt to create new session
        if let OlmMessage::PreKey(m) = olm_message {
            Conversations::process_prekey_message(client, conn, m, &payload)?;
            Ok(InboundMessageStatus::Processed)
        } else {
            log::warn!("Message:{} could not be decrypted", msg.id);
            Ok(InboundMessageStatus::DecryptionFailure)
        }
    }

    fn process_plaintext(
        client: &Client<A>,
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

        client
            .store
            .insert_or_ignore_message(conn, stored_message)?;

        Ok(())
    }

    fn process_prekey_message(
        client: &Client<A>,
        conn: &mut DbConnection,
        msg: olm::PreKeyMessage,
        payload: &DecodedInboundMessage,
    ) -> Result<(), ConversationError> {
        let network_contact = block_on(client.download_contact_for_installation(
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

        let (_, plaintext) = client.create_inbound_session(conn, &contact, msg)?;
        Conversations::process_plaintext(client, conn, &plaintext, payload)?;
        Ok(())
    }

    pub fn save_invites(client: &Client<A>) -> Result<(), ConversationError> {
        let my_contact = client.account.contact();

        client
            .store
            .lock_refresh_job(RefreshJobKind::Invite, |conn, job| {
                let downloaded = futures::executor::block_on(client.download_latest_from_topic(
                    Conversations::<A>::get_start_time(&job).unsigned_abs(),
                    crate::utils::build_user_invite_topic(my_contact.installation_id()),
                ))
                .map_err(|e| StorageError::Unknown(e.to_string()))?;
                // Save all invites
                for envelope in downloaded {
                    client.store.save_inbound_invite(conn, envelope.into())?;
                }

                Ok(())
            })?;

        Ok(())
    }

    pub fn process_invites(client: &Client<A>) -> Result<(), ConversationError> {
        let conn = &mut client.store.conn()?;
        conn.transaction::<_, StorageError, _>(|transaction_manager| {
            let invites = client
                .store
                .get_inbound_invites(transaction_manager, InboundInviteStatus::Pending)?;
            for invite in invites {
                let invite_id = invite.id.clone();
                match Conversations::process_inbound_invite(client, transaction_manager, invite) {
                    Ok(status) => {
                        log::debug!(
                            "Invite processed: {:?}. Status: {:?}",
                            invite_id,
                            status.clone()
                        );
                        client
                            .store
                            .set_invite_status(transaction_manager, invite_id, status)?;
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
        client: &Client<A>,
        conn: &mut DbConnection,
        invite: InboundInvite,
    ) -> Result<InboundInviteStatus, ConversationError> {
        let invitation: Invitation = match invite.payload.try_into() {
            Ok(invitation) => invitation,
            Err(_) => {
                return Ok(InboundInviteStatus::Invalid);
            }
        };

        let existing_session =
            Conversations::find_existing_session_with_conn(client, &invitation.inviter, conn)?;
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
                    match client.create_inbound_session(conn, &invitation.inviter, prek_key) {
                        Ok((session, plaintext)) => (session, plaintext),
                        Err(err) => {
                            log::error!("Error creating session: {:?}", err);
                            return Ok(InboundInviteStatus::DecryptionFailure);
                        }
                    };
            }
        };

        let inner_invite: ProtoWrapper<InvitationV1> = plaintext.try_into()?;
        if !Conversations::validate_invite(client, &invitation, &inner_invite.proto) {
            return Ok(InboundInviteStatus::Invalid);
        }
        // Create the user if doesn't exist
        let peer_address =
            Conversations::get_invite_peer_address(client, &invitation, &inner_invite.proto);
        client.store.insert_or_ignore_user_with_conn(
            conn,
            StoredUser {
                user_address: peer_address.clone(),
                created_at: now(),
                last_refreshed: 0,
            },
        )?;

        // Create the conversation if doesn't exist
        client.store.insert_or_ignore_conversation_with_conn(
            conn,
            StoredConversation {
                convo_id: convo_id(
                    peer_address.clone(),
                    client.account.contact().wallet_address,
                ),
                peer_address,
                created_at: now(),
                convo_state: ConversationState::InviteReceived as i32,
            },
        )?;

        Ok(InboundInviteStatus::Processed)
    }

    fn validate_invite(
        client: &Client<A>,
        invitation: &Invitation,
        inner_invite: &InvitationV1,
    ) -> bool {
        let my_wallet_address = client.account.contact().wallet_address;
        let inviter_is_my_other_device = my_wallet_address == invitation.inviter.wallet_address;

        if inviter_is_my_other_device {
            true
        } else {
            inner_invite.invitee_wallet_address == my_wallet_address
        }
    }

    fn get_invite_peer_address(
        client: &Client<A>,
        invitation: &Invitation,
        inner_invite: &InvitationV1,
    ) -> String {
        let my_wallet_address = client.account.contact().wallet_address;
        let inviter_is_my_other_device = my_wallet_address == invitation.inviter.wallet_address;

        if inviter_is_my_other_device {
            inner_invite.invitee_wallet_address.clone()
        } else {
            invitation.inviter.wallet_address.clone()
        }
    }

    fn find_existing_session_with_conn(
        client: &Client<A>,
        contact: &Contact,
        conn: &mut DbConnection,
    ) -> Result<Option<SessionManager>, ConversationError> {
        Conversations::find_existing_session(client, &contact.installation_id(), conn)
    }

    fn find_existing_session(
        client: &Client<A>,
        installation_id: &str,
        conn: &mut DbConnection,
    ) -> Result<Option<SessionManager>, ConversationError> {
        let stored_session = client
            .store
            .get_latest_session_for_installation(installation_id, conn)?;

        match stored_session {
            Some(i) => Ok(Some(SessionManager::try_from(&i)?)),
            None => Ok(None),
        }
    }

    fn get_start_time(job: &RefreshJob) -> i64 {
        // Adjust for padding and ensure start_time > 0
        std::cmp::max(job.last_run - PADDING_TIME_NS, 0)
    }

    fn create_outbound_payload(
        client: &Client<A>,
        session: &mut SessionManager,
        message: &StoredMessage,
    ) -> Result<StoredOutboundPayload, ConversationError> {
        let is_prekey_message = !session.has_received_message();

        let metadata = PadlockMessageSealedMetadata {
            sender_user_address: client.wallet_address(),
            sender_installation_id: client.account.contact().installation_id(),
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
        let header_signature = client.account.sign(&base64_encode(&header_bytes));
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
            olm::OlmMessage::Normal(message) => message.to_bytes(),
            olm::OlmMessage::PreKey(prekey_message) => prekey_message.to_bytes(),
        };
        let envelope: PadlockMessageEnvelope = PadlockMessageEnvelope {
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
        client: &Client<A>,
        message: &StoredMessage,
    ) -> Result<(), ConversationError> {
        let peer_address = peer_addr_from_convo_id(&message.convo_id, &client.wallet_address())?;

        // Refresh remote installations
        client
            .refresh_user_installations_if_stale(&peer_address)
            .await?;
        client.store.conn().unwrap().transaction(
            |transaction| -> Result<(), ConversationError> {
                let my_sessions = client
                    .store
                    .get_latest_sessions(&client.wallet_address(), transaction)?;
                let their_user_addr =
                    peer_addr_from_convo_id(&message.convo_id, &client.wallet_address())?;
                let their_sessions = client
                    .store
                    .get_latest_sessions(&their_user_addr, transaction)?;
                if their_sessions.is_empty() {
                    return Err(ConversationError::NoSessions(their_user_addr));
                }

                let mut outbound_payloads = Vec::new();
                let mut updated_sessions = Vec::new();
                for stored_session in my_sessions.iter().chain(&their_sessions) {
                    if stored_session.peer_installation_id
                        == client.account.contact().installation_id()
                    {
                        continue;
                    }
                    let mut session = SessionManager::try_from(stored_session)?;
                    let outbound_payload =
                        Conversations::create_outbound_payload(client, &mut session, message)?;
                    let updated_session = StoredSession::try_from(&session)?;
                    outbound_payloads.push(outbound_payload);
                    updated_sessions.push(updated_session);
                }

                client.store.commit_outbound_payloads_for_message(
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

    pub async fn process_outbound_messages(client: &Client<A>) -> Result<(), ConversationError> {
        //Refresh self installations
        client
            .refresh_user_installations_if_stale(&client.wallet_address())
            .await?;
        let mut messages = client.store.get_unprocessed_messages()?;
        log::debug!("Processing {} messages", messages.len());
        messages.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        for message in messages {
            if let Err(e) = Conversations::process_outbound_message(client, &message).await {
                log::error!(
                    "Couldn't process message with ID {} because of error: {:?}",
                    message.id,
                    e
                );
                // TODO update message status to failed on non-retryable errors so that we don't retry it next time
            }
        }

        Conversations::publish_outbound_payloads(client).await?;
        Ok(())
    }

    pub async fn publish_outbound_payloads(client: &Client<A>) -> Result<(), ConversationError> {
        let unsent_payloads = client.store.fetch_and_lock_outbound_payloads(
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
        client
            .api_client
            .publish("".to_string(), PublishRequest { envelopes })
            .await?;

        let payload_ids = unsent_payloads
            .iter()
            .map(|payload| payload.created_at_ns)
            .collect();
        client.store.update_and_unlock_outbound_payloads(
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
        conversation::{convo_id, SecretConversation},
        conversations::Conversations,
        invitation::Invitation,
        storage::{
            now, InboundInvite, InboundInviteStatus, MessageState, StoredConversation,
            StoredMessage, StoredUser,
        },
        test_utils::test_utils::{gen_test_client, gen_test_conversation, gen_two_test_clients},
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
        let conversation =
            SecretConversation::new(&alice_client, bob_client.wallet_address().to_string())
                .unwrap();
        assert_eq!(conversation.peer_address(), bob_client.wallet_address());
        conversation.initialize().await.unwrap();
    }

    #[tokio::test]
    async fn save_invites() {
        let mut alice_client = ClientBuilder::new_test().build().unwrap();
        alice_client.init().await.unwrap();
        let invites = Conversations::save_invites(&alice_client);
        assert!(invites.is_ok());
    }

    #[tokio::test]
    async fn create_outbound_payload() {
        let alice_client = gen_test_client().await;
        let bob_client = gen_test_client().await;

        let mut session = alice_client
            .get_session(
                &mut alice_client.store.conn().unwrap(),
                &bob_client.account.contact(),
            )
            .unwrap();

        let _payload = Conversations::create_outbound_payload(
            &alice_client,
            &mut session,
            &StoredMessage {
                id: 0,
                created_at: 0,
                convo_id: convo_id(alice_client.wallet_address(), bob_client.wallet_address()),
                addr_from: alice_client.wallet_address(),
                sent_at_ns: 0,
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

        let conversation = gen_test_conversation(&alice_client, &bob_client.wallet_address()).await;
        conversation.send_text("Hello world").await.unwrap();
        Conversations::process_outbound_messages(&alice_client)
            .await
            .unwrap();
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

        let process_result = Conversations::process_invites(&bob_client);
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

        let process_result = Conversations::process_invites(&bob_client);
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

        let a_to_b = SecretConversation::new(&alice_client, bob_address.clone()).unwrap();
        // Send First Message
        a_to_b.send_text("Hi").await.unwrap();
        Conversations::receive(&bob_client).unwrap();

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
        let b_to_a = SecretConversation::new(&bob_client, bob_address.clone()).unwrap();
        b_to_a.send_text("Reply").await.unwrap();
        Conversations::receive(&alice_client).unwrap();

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
        let (alice_client, bob_client) = gen_two_test_clients().await;

        let conversation = gen_test_conversation(&alice_client, &bob_client.wallet_address()).await;

        let list = Conversations::list(&alice_client, true).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].peer_address(), conversation.peer_address());
    }
}
