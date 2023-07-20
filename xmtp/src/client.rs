use core::fmt;
use std::fmt::Formatter;

use thiserror::Error;
use vodozemac::olm::OlmMessage;

use crate::{
    account::Account,
    contact::{Contact, ContactError},
    session::SessionManager,
    storage::{EncryptedMessageStore, StorageError, StoredInstallation},
    types::networking::{PublishRequest, QueryRequest, XmtpApiClient},
    types::Address,
    utils::{build_envelope, build_user_contact_topic},
    Store,
};
use xmtp_proto::xmtp::message_api::v1::Envelope;

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
    #[error("Query failed: {0}")]
    QueryError(String),
    #[error("unknown client error")]
    Unknown,
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
            .await
            .map_err(|e| ClientError::QueryError(format!("Could not query for contacts: {}", e)))?;

        let mut contacts = vec![];
        for envelope in response.envelopes {
            let contact_bundle = Contact::from_bytes(envelope.message, wallet_address.to_string());
            match contact_bundle {
                Ok(bundle) => {
                    contacts.push(bundle);
                }
                Err(err) => {
                    println!("bad contact bundle: {:?}", err);
                }
            }
        }

        Ok(contacts)
    }

    // async fn update_installations()

    pub fn get_session(&self, contact: &Contact) -> Result<SessionManager, ClientError> {
        let existing_session = self.store.get_session(&contact.installation_id())?;
        match existing_session {
            Some(i) => Ok(SessionManager::try_from(&i)?),
            None => self.create_outbound_session(contact),
        }
    }

    pub async fn my_other_devices(&self) -> Result<Vec<Contact>, ClientError> {
        let contacts = self.get_contacts(self.account.addr().as_str()).await?;
        let my_contact_id = self.account.contact().installation_id();
        Ok(contacts
            .into_iter()
            .filter(|c| c.installation_id() != my_contact_id)
            .collect())
    }

    pub async fn refresh_user_installations(&self, user_address: &str) -> Result<(), ClientError> {
        let contacts = self.get_contacts(user_address).await?;

        let stored_contacts: Vec<StoredInstallation> =
            self.store.get_contacts(user_address)?.into();
        println!("{:?}", contacts);
        for contact in contacts {
            println!(" {:?} ", contact)
        }

        Ok(())
    }

    pub fn create_outbound_session(
        &self,
        contact: &Contact,
    ) -> Result<SessionManager, ClientError> {
        let olm_session = self.account.create_outbound_session(contact);
        let session = SessionManager::from_olm_session(olm_session, contact)
            .map_err(|_| ClientError::Unknown)?;

        session.store(&self.store)?;

        Ok(session)
    }

    pub fn create_inbound_session(
        &self,
        contact: Contact,
        // Message MUST be a pre-key message
        message: Vec<u8>,
    ) -> Result<(SessionManager, Vec<u8>), ClientError> {
        let olm_message: OlmMessage =
            serde_json::from_slice(message.as_slice()).map_err(|_| ClientError::Unknown)?;
        let msg = match olm_message {
            OlmMessage::PreKey(msg) => msg,
            _ => return Err(ClientError::Unknown),
        };

        let create_result = self
            .account
            .create_inbound_session(&contact, msg)
            .map_err(|_| ClientError::Unknown)?;

        let session = SessionManager::from_olm_session(create_result.session, &contact)
            .map_err(|_| ClientError::Unknown)?;

        session.store(&self.store)?;

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
            .await
            .map_err(|e| ClientError::PublishError(format!("Could not publish contact: {}", e)))?;

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
}

#[cfg(test)]
mod tests {
    use xmtp_proto::xmtp::v3::message_contents::installation_contact_bundle::Version;
    use xmtp_proto::xmtp::v3::message_contents::vmac_unsigned_public_key::Union::Curve25519;
    use xmtp_proto::xmtp::v3::message_contents::vmac_unsigned_public_key::VodozemacCurve25519;

    use crate::conversations::Conversations;
    use crate::ClientBuilder;

    #[tokio::test]
    async fn registration() {
        let mut client = ClientBuilder::new_test().build().unwrap();
        client.init().await.expect("BadReg");
    }

    #[tokio::test]
    async fn test_local_conversation_creation() {
        let mut client = ClientBuilder::new_test().build().unwrap();
        client.init().await.expect("BadReg");
        let peer_address = "0x000";
        let convo_id = format!(":{}:{}", peer_address, client.wallet_address());
        assert!(client.store.get_conversation(&convo_id).unwrap().is_none());
        let conversations = Conversations::new(&client);
        let conversation = conversations
            .new_secret_conversation(peer_address.to_string())
            .await
            .unwrap();
        assert!(conversation.peer_address() == peer_address);
        assert!(client.store.get_conversation(&convo_id).unwrap().is_some());
    }

    #[tokio::test]
    async fn refresh() {
        let mut client = ClientBuilder::new_test().build().unwrap();
        client
            .refresh_user_installations(&client.wallet_address())
            .await;
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
}
