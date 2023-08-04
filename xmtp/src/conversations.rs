use std::time::Duration;

use diesel::Connection;
use prost::Message;
use vodozemac::olm::OlmMessage;
use xmtp_proto::xmtp::{
    message_api::v1::{Envelope, PublishRequest},
    v3::message_contents::{
        EdDsaSignature, PadlockMessageEnvelope, PadlockMessageHeader, PadlockMessagePayload,
        PadlockMessagePayloadVersion, PadlockMessageSealedMetadata,
    },
};

use crate::{
    conversation::{peer_addr_from_convo_id, ConversationError, SecretConversation},
    session::SessionManager,
    storage::{
        MessageState, OutboundPayloadState, RefreshJob, RefreshJobKind, StorageError,
        StoredMessage, StoredOutboundPayload, StoredSession,
    },
    types::networking::XmtpApiClient,
    utils::{base64_encode, build_installation_message_topic},
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
        Ok(StoredOutboundPayload {
            created_at_ns: message.created_at,
            content_topic: build_installation_message_topic(&session.installation_id()),
            payload: envelope.encode_to_vec(),
            outbound_payload_state: OutboundPayloadState::Pending as i32,
            locked_until_ns: 0,
        })
    }

    pub fn process_outbound_message(
        &self,
        message: &StoredMessage,
    ) -> Result<(), ConversationError> {
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
        let mut messages = self.client.store.get_unprocessed_messages()?;
        messages.sort_by(|a, b| a.created_at.cmp(&b.created_at));

        for message in messages {
            if let Err(e) = self.process_outbound_message(&message) {
                log::error!(
                    "Couldn't process message with ID {} because of error: {}",
                    message.id,
                    e.to_string()
                );
                // TODO update message status to failed on non-retryable errors so that we don't retry it next time
            }
        }

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

    use crate::{
        codecs::{text::TextCodec, ContentCodec},
        conversation::convo_id,
        conversations::Conversations,
        storage::{MessageState, OutboundPayloadState, StoredMessage},
        test_utils::test_utils::{gen_test_client, gen_test_conversation},
        ClientBuilder,
    };

    #[tokio::test]
    async fn create_secret_conversation() {
        let alice_client = gen_test_client().await;
        let bob_client = gen_test_client().await;

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

    #[tokio::test]
    async fn create_outbound_payload() {
        let alice_client = gen_test_client().await;
        let bob_client = gen_test_client().await;

        let conversations = Conversations::new(&alice_client);
        let mut session = alice_client
            .create_outbound_session(&bob_client.account.contact())
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
        let alice_client = gen_test_client().await;
        let bob_client = gen_test_client().await;

        let conversations = Conversations::new(&alice_client);
        let conversation =
            gen_test_conversation(&conversations, &bob_client.wallet_address()).await;

        conversation.send_message("Hello world").unwrap();
        let unprocessed_messages = alice_client.store.get_unprocessed_messages().unwrap();
        assert_eq!(unprocessed_messages.len(), 1);

        // TODO replace with Client.refresh_user_installations. Requires us to refactor the SDK so that
        // two XMTP clients can share the same API client
        alice_client
            .create_outbound_session(&bob_client.account.contact())
            .unwrap();

        conversations.process_outbound_messages().await.unwrap();
        let unprocessed_messages = alice_client.store.get_unprocessed_messages().unwrap();
        assert_eq!(unprocessed_messages.len(), 0);
        let unsent_payloads = alice_client
            .store
            .fetch_and_lock_outbound_payloads(OutboundPayloadState::Pending, 0)
            .unwrap();
        assert_eq!(unsent_payloads.len(), 1);

        conversations.publish_outbound_payloads().await.unwrap();
        let unsent_payloads = alice_client
            .store
            .fetch_and_lock_outbound_payloads(OutboundPayloadState::Pending, 0)
            .unwrap();
        assert_eq!(unsent_payloads.len(), 0);
    }
}
