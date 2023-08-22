use core::fmt;
use std::{fmt::Formatter, time::Duration};

use diesel::Connection;
use log::{debug, info};
use prost::Message;
use thiserror::Error;
use vodozemac::olm::{self, PreKeyMessage};

use crate::{
    account::Account,
    contact::{Contact, ContactError},
    conversation::peer_addr_from_convo_id,
    session::SessionManager,
    storage::{
        now, DbConnection, EncryptedMessageStore, MessageState, OutboundPayloadState, StorageError,
        StoredInstallation, StoredMessage, StoredOutboundPayload, StoredSession, StoredUser,
    },
    types::networking::{PublishRequest, QueryRequest, XmtpApiClient},
    types::Address,
    utils::{
        base64_encode, build_envelope, build_installation_message_topic, build_user_contact_topic,
        key_fingerprint,
    },
    Store,
};
use std::collections::HashMap;
use xmtp_proto::xmtp::{
    message_api::v1::Envelope,
    v3::message_contents::{
        EdDsaSignature, PadlockMessageEnvelope, PadlockMessageHeader, PadlockMessagePayload,
        PadlockMessagePayloadVersion, PadlockMessageSealedMetadata,
    },
};

const INSTALLATION_REFRESH_INTERVAL_NS: i64 = 0;

#[derive(Clone, Copy, Default, Debug)]
pub enum Network {
    Local(&'static str),
    #[default]
    Dev,
    Prod,
}

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("contact error {0}")]
    Contact(#[from] ContactError),
    #[error("could not publish: {0}")]
    PublishError(String),
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("dieselError: {0}")]
    Ddd(#[from] diesel::result::Error),
    #[error("Query failed: {0}")]
    QueryError(#[from] crate::types::networking::Error),
    #[error("generic:{0}")]
    Generic(String),
    #[error("No sessions for user: {0}")]
    NoSessions(String),
}

impl From<String> for ClientError {
    fn from(value: String) -> Self {
        Self::Generic(value)
    }
}

impl From<&str> for ClientError {
    fn from(value: &str) -> Self {
        Self::Generic(value.to_string())
    }
}

pub struct Client<A>
where
    A: XmtpApiClient,
{
    pub api_client: A,
    pub(crate) network: Network,
    pub(crate) account: Account,
    pub store: EncryptedMessageStore, // Temporarily exposed outside crate for CLI client
    is_initialized: bool,
}

impl<A> core::fmt::Debug for Client<A>
where
    A: XmtpApiClient,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Client({:?})::{}", self.network, self.account.addr())
    }
}

impl<A> Client<A>
where
    A: XmtpApiClient,
{
    pub fn new(
        api_client: A,
        network: Network,
        account: Account,
        store: EncryptedMessageStore,
    ) -> Self {
        Self {
            api_client,
            network,
            account,
            store,
            is_initialized: false,
        }
    }

    pub fn wallet_address(&self) -> Address {
        self.account.addr()
    }

    pub fn installation_id(&self) -> String {
        self.account.contact().installation_id()
    }

    pub async fn init(&mut self) -> Result<(), ClientError> {
        let app_contact_bundle = self.account.contact();
        let registered_bundles = self.get_contacts(&self.wallet_address()).await?;

        if !registered_bundles
            .iter()
            .any(|contact| contact.installation_id() == app_contact_bundle.installation_id())
        {
            self.publish_user_contact().await?;
        }

        self.is_initialized = true;

        // Send any unsent messages
        if let Err(err) = self.process_outbound_messages().await {
            log::error!("Could not process outbound messages on init: {:?}", err)
        }

        Ok(())
    }

    pub async fn get_contacts(&self, wallet_address: &str) -> Result<Vec<Contact>, ClientError> {
        let topic = build_user_contact_topic(wallet_address.to_string());
        let response = self
            .api_client
            .query(QueryRequest {
                content_topics: vec![topic],
                start_time_ns: 0,
                end_time_ns: 0,
                paging_info: None,
            })
            .await?;

        let mut contacts = vec![];
        for envelope in response.envelopes {
            let contact_bundle = Contact::from_bytes(envelope.message, wallet_address.to_string());
            match contact_bundle {
                Ok(bundle) => {
                    contacts.push(bundle);
                }
                Err(err) => {
                    log::error!("bad contact bundle: {:?}", err);
                }
            }
        }

        Ok(contacts)
    }

    pub fn get_session(
        &self,
        conn: &mut DbConnection,
        contact: &Contact,
    ) -> Result<SessionManager, ClientError> {
        let existing_session = self
            .store
            .get_latest_session_for_installation(&contact.installation_id(), conn)?;
        match existing_session {
            Some(i) => Ok(SessionManager::try_from(&i)?),
            None => self.create_outbound_session(conn, contact),
        }
    }

    pub fn my_other_devices(&self, conn: &mut DbConnection) -> Result<Vec<Contact>, ClientError> {
        let contacts = self.get_contacts_from_db(conn, self.account.addr().as_str())?;
        Ok(contacts
            .into_iter()
            .filter(|c| c.installation_id() != self.installation_id())
            .collect())
    }

    pub async fn refresh_user_installations_if_stale(
        &self,
        user_address: &str,
    ) -> Result<(), ClientError> {
        let user = self.store.get_user(user_address)?;
        if user.is_none() || user.unwrap().last_refreshed < now() - INSTALLATION_REFRESH_INTERVAL_NS
        {
            self.refresh_user_installations(user_address).await?;
        }

        Ok(())
    }

    /// Fetch Installations from the Network and create unintialized sessions for newly discovered contacts
    // TODO: Reduce Visibility
    pub async fn refresh_user_installations(&self, user_address: &str) -> Result<(), ClientError> {
        // Store the timestamp of when the refresh process begins
        let refresh_timestamp = now();

        let self_install_id = key_fingerprint(&self.account.identity_keys().curve25519);
        let contacts = self.get_contacts(user_address).await?;
        debug!(
            "Fetched contacts for address {}: {:?}",
            user_address, contacts
        );

        let installation_map = self
            .store
            .get_installations(&mut self.store.conn()?, user_address)?
            .into_iter()
            .map(|v| (v.installation_id.clone(), v))
            .collect::<HashMap<_, _>>();

        let new_installs: Vec<StoredInstallation> = contacts
            .iter()
            .filter(|contact| self_install_id != contact.installation_id())
            .filter(|contact| !installation_map.contains_key(&contact.installation_id()))
            .filter_map(|contact| StoredInstallation::new(contact).ok())
            .collect();
        debug!(
            "New installs for address {}: {:?}",
            user_address, new_installs
        );

        self.store
            .conn()?
            .transaction(|transaction_manager| -> Result<(), ClientError> {
                self.store.insert_or_ignore_user_with_conn(
                    transaction_manager,
                    StoredUser {
                        user_address: user_address.to_string(),
                        created_at: now(),
                        last_refreshed: refresh_timestamp,
                    },
                )?;
                for install in new_installs {
                    info!("Saving Install {}", install.installation_id);
                    let session = self.create_uninitialized_session(&install.get_contact()?)?;

                    self.store
                        .insert_or_ignore_install(install, transaction_manager)?;
                    self.store.insert_or_ignore_session(
                        StoredSession::try_from(&session)?,
                        transaction_manager,
                    )?;
                }

                self.store.update_user_refresh_timestamp(
                    transaction_manager,
                    user_address,
                    refresh_timestamp,
                )?;

                Ok(())
            })?;

        Ok(())
    }

    pub fn get_contacts_from_db(
        &self,
        conn: &mut DbConnection,
        wallet_address: &str,
    ) -> Result<Vec<Contact>, ClientError> {
        let installations = self.store.get_installations(conn, wallet_address)?;

        Ok(installations
            .into_iter()
            .filter_map(|i| i.get_contact().ok())
            .collect())
    }

    pub fn create_uninitialized_session(
        &self,
        contact: &Contact,
    ) -> Result<SessionManager, ClientError> {
        let olm_session = self.account.create_outbound_session(contact);
        Ok(SessionManager::from_olm_session(olm_session, contact)?)
    }

    fn create_outbound_session(
        &self,
        conn: &mut DbConnection,
        contact: &Contact,
    ) -> Result<SessionManager, ClientError> {
        let olm_session = self.account.create_outbound_session(contact);
        let session = SessionManager::from_olm_session(olm_session, contact)?;

        session.store(conn)?;

        Ok(session)
    }

    pub fn create_inbound_session(
        &self,
        conn: &mut DbConnection,
        contact: &Contact,
        prekey_message: PreKeyMessage,
    ) -> Result<(SessionManager, Vec<u8>), ClientError> {
        let create_result = self
            .account
            .create_inbound_session(contact, prekey_message)
            .map_err(|e| e.to_string())?;

        let session = SessionManager::from_olm_session(create_result.session, contact)?;

        if let Err(e) = session.store(conn) {
            match e {
                StorageError::DieselResultError(_) => log::warn!("Session Already exists"), // TODO: Some thought is needed here, is this a critical error which should unroll?
                other_error => return Err(other_error.into()),
            }
        }

        Ok((session, create_result.plaintext))
    }

    async fn publish_user_contact(&self) -> Result<(), ClientError> {
        let envelope = self.build_contact_envelope()?;
        self.api_client
            .publish(
                "".to_string(),
                PublishRequest {
                    envelopes: vec![envelope],
                },
            )
            .await?;

        Ok(())
    }

    fn build_contact_envelope(&self) -> Result<Envelope, ClientError> {
        let contact = self.account.contact();

        let envelope = build_envelope(
            build_user_contact_topic(self.wallet_address()),
            contact.try_into()?,
        );

        Ok(envelope)
    }

    pub async fn download_latest_from_topic(
        &self,
        start_time: u64,
        topic: String,
    ) -> Result<Vec<Envelope>, ClientError> {
        let response = self
            .api_client
            .query(QueryRequest {
                content_topics: vec![topic],
                start_time_ns: start_time,
                end_time_ns: 0,
                // TODO: Pagination
                paging_info: None,
            })
            .await?;

        Ok(response.envelopes)
    }

    /// Search network for a specific InstallationContact
    /// This function should be removed as soon as possible given it is a potential DOS vector.
    /// Contacts for a message should always be known to the client
    pub async fn download_contact_for_installation(
        &self,
        wallet_address: &str,
        installation_id: &str,
    ) -> Result<Option<Contact>, ClientError> {
        let contacts = self.get_contacts(wallet_address).await?; // TODO: Ensure invalid contacts cannot be initialized

        for contact in contacts {
            if contact.installation_id() == installation_id {
                return Ok(Some(contact));
            }
        }
        Ok(None)
    }

    fn create_outbound_payload(
        &self,
        session: &mut SessionManager,
        message: &StoredMessage,
    ) -> Result<StoredOutboundPayload, ClientError> {
        let is_prekey_message = !session.has_received_message();

        let metadata = PadlockMessageSealedMetadata {
            sender_user_address: self.wallet_address(),
            sender_installation_id: self.account.contact().installation_id(),
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
        let header_signature = self.account.sign(&base64_encode(&header_bytes));
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
        &self,
        message: &StoredMessage,
    ) -> Result<(), ClientError> {
        let peer_address = peer_addr_from_convo_id(&message.convo_id, &self.wallet_address())
            .map_err(|e| e.to_string())?;

        // Refresh remote installations
        self.refresh_user_installations_if_stale(&peer_address)
            .await?;
        self.store
            .conn()
            .unwrap()
            .transaction(|transaction| -> Result<(), ClientError> {
                let my_sessions = self
                    .store
                    .get_latest_sessions(&self.wallet_address(), transaction)?;
                let their_user_addr =
                    peer_addr_from_convo_id(&message.convo_id, &self.wallet_address())
                        .map_err(|e| e.to_string())?;
                let their_sessions = self
                    .store
                    .get_latest_sessions(&their_user_addr, transaction)?;
                if their_sessions.is_empty() {
                    return Err(ClientError::NoSessions(their_user_addr));
                }

                let mut outbound_payloads = Vec::new();
                let mut updated_sessions = Vec::new();
                for stored_session in my_sessions.iter().chain(&their_sessions) {
                    if stored_session.peer_installation_id
                        == self.account.contact().installation_id()
                    {
                        continue;
                    }
                    let mut session = SessionManager::try_from(stored_session)?;
                    let outbound_payload = self.create_outbound_payload(&mut session, message)?;
                    let updated_session = StoredSession::try_from(&session)?;
                    outbound_payloads.push(outbound_payload);
                    updated_sessions.push(updated_session);
                }

                self.store.commit_outbound_payloads_for_message(
                    message.id,
                    MessageState::LocallyCommitted,
                    outbound_payloads,
                    updated_sessions,
                    transaction,
                )?;
                Ok(())
            })?;

        Ok(())
    }

    pub async fn process_outbound_messages(&self) -> Result<(), ClientError> {
        //Refresh self installations
        self.refresh_user_installations_if_stale(&self.wallet_address())
            .await?;
        let mut messages = self.store.get_unprocessed_messages()?;
        log::debug!("Processing {} messages", messages.len());
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

    pub async fn publish_outbound_payloads(&self) -> Result<(), ClientError> {
        let unsent_payloads = self.store.fetch_and_lock_outbound_payloads(
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
        self.api_client
            .publish("".to_string(), PublishRequest { envelopes })
            .await?;

        let payload_ids = unsent_payloads
            .iter()
            .map(|payload| payload.created_at_ns)
            .collect();
        self.store.update_and_unlock_outbound_payloads(
            payload_ids,
            OutboundPayloadState::ServerAcknowledged,
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use prost::Message;
    use xmtp_proto::xmtp::v3::message_contents::installation_contact_bundle::Version;
    use xmtp_proto::xmtp::v3::message_contents::vmac_unsigned_public_key::Union::Curve25519;
    use xmtp_proto::xmtp::v3::message_contents::vmac_unsigned_public_key::VodozemacCurve25519;

    use crate::conversation::convo_id;
    use crate::storage::{MessageState, StoredMessage};
    use crate::test_utils::test_utils::gen_test_client;
    use crate::{ClientBuilder, ContentCodec, TextCodec};

    #[tokio::test]
    async fn registration() {
        gen_test_client().await;
    }

    #[tokio::test]
    async fn refresh() {
        let client = ClientBuilder::new_test().build().unwrap();
        client
            .refresh_user_installations(&client.wallet_address())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_publish_user_contact() {
        let client = ClientBuilder::new_test().build().unwrap();
        client
            .publish_user_contact()
            .await
            .expect("Failed to publish user contact");

        let contacts = client
            .get_contacts(client.wallet_address().as_str())
            .await
            .unwrap();

        assert_eq!(contacts.len(), 1);
        let installation_bundle = match contacts[0].clone().bundle.version.unwrap() {
            Version::V1(bundle) => bundle,
        };
        assert!(installation_bundle.fallback_key.is_some());
        assert!(installation_bundle.identity_key.is_some());
        contacts[0].vmac_identity_key();
        contacts[0].vmac_fallback_key();

        let key_bytes = installation_bundle
            .clone()
            .identity_key
            .unwrap()
            .key
            .unwrap()
            .union
            .unwrap();

        match key_bytes {
            Curve25519(VodozemacCurve25519 { bytes }) => {
                assert_eq!(bytes.len(), 32);
                assert_eq!(
                    client
                        .account
                        .olm_account()
                        .unwrap()
                        .get()
                        .curve25519_key()
                        .to_bytes()
                        .to_vec(),
                    bytes
                )
            }
        }
    }

    #[tokio::test]
    async fn test_roundtrip_encrypt() {}

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

        let _payload = alice_client
            .create_outbound_payload(
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
}
