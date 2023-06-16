use core::fmt;
use std::fmt::Formatter;

use thiserror::Error;

use crate::{
    account::Account,
    contact::{Contact, ContactError},
    networking::XmtpApiClient,
    session::SessionManager,
    storage::{EncryptedMessageStore, StorageError},
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
    pub network: Network,
    pub(crate) account: Account,
    pub(super) _store: EncryptedMessageStore,
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
            _store: store,
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
            .query(topic, None, None, None)
            .await
            .map_err(|e| ClientError::QueryError(format!("Could not query for contacts: {}", e)))?;

        let mut contacts = vec![];
        for envelope in response.envelopes {
            let contact_bundle = Contact::from_bytes(envelope.message, wallet_address.to_string())?;
            contacts.push(contact_bundle);
        }

        Ok(contacts)
    }

    pub fn create_outbound_session(
        &mut self,
        contact: Contact,
    ) -> Result<SessionManager, ClientError> {
        let olm_session = self.account.create_outbound_session(contact.clone());
        let session = SessionManager::from_olm_session(olm_session, contact)
            .map_err(|_| ClientError::Unknown)?;

        session
            .store(&self._store)
            .map_err(|_| ClientError::Unknown)?;

        Ok(session)
    }

    async fn publish_user_contact(&mut self) -> Result<(), ClientError> {
        let envelope = self.build_contact_envelope()?;
        self.api_client
            .publish("".to_string(), vec![envelope])
            .await
            .map_err(|e| ClientError::PublishError(format!("Could not publish contact: {}", e)))?;

        Ok(())
    }

    fn build_contact_envelope(&self) -> Result<Envelope, ClientError> {
        let contact = self.account.contact();
        let contact_bytes = contact.to_bytes()?;

        let envelope = build_envelope(
            build_user_contact_topic(self.wallet_address()),
            contact_bytes,
        );

        Ok(envelope)
    }
}

#[cfg(test)]
mod tests {
    use xmtp_proto::xmtp::v3::message_contents::installation_contact_bundle::Version;
    use xmtp_proto::xmtp::v3::message_contents::vmac_unsigned_public_key::Union::Curve25519;
    use xmtp_proto::xmtp::v3::message_contents::vmac_unsigned_public_key::VodozemacCurve25519;

    use crate::ClientBuilder;

    #[tokio::test]
    async fn registration() {
        let mut client = ClientBuilder::new_test().build().unwrap();
        client.init().await.expect("BadReg");
    }

    #[tokio::test]
    async fn test_publish_user_contact() {
        let mut client = ClientBuilder::new_test().build().unwrap();
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
                        .keys
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
