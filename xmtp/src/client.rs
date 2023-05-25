use thiserror::Error;

use crate::{
    account::Account,
    contact::{Contact, ContactError},
    networking::XmtpApiClient,
    persistence::{NamespacedPersistence, Persistence},
    types::Address,
    utils::{build_envelope, build_user_contact_topic},
};
use xmtp_proto::xmtp::message_api::v1::Envelope;

#[derive(Clone, Copy, Default)]
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
    #[error("Query failed: {0}")]
    QueryError(String),
    #[error("unknown client error")]
    Unknown,
}

pub struct Client<A, P, S>
where
    A: XmtpApiClient,
    P: Persistence,
{
    pub api_client: A,
    pub network: Network,
    pub persistence: NamespacedPersistence<P>,
    pub(crate) account: Account,
    pub(super) _store: S,
}

impl<A, P, S> Client<A, P, S>
where
    A: XmtpApiClient,
    P: Persistence,
{
}

impl<A, P, S> Client<A, P, S>
where
    A: XmtpApiClient,
    P: Persistence,
{
    pub fn write_to_persistence(&mut self, s: &str, b: &[u8]) -> Result<(), P::Error> {
        self.persistence.write(s, b)
    }

    pub fn read_from_persistence(&self, s: &str) -> Result<Option<Vec<u8>>, P::Error> {
        self.persistence.read(s)
    }

    pub fn wallet_address(&self) -> Address {
        self.account.addr()
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
            let contact_bundle = Contact::from_bytes(envelope.message)?;
            contacts.push(contact_bundle);
        }

        Ok(contacts)
    }

    pub async fn publish_user_contact(&mut self) -> Result<(), ClientError> {
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
    use xmtp_proto::xmtp::v3::message_contents::vmac_unsigned_public_key::Union::Curve25519;
    use xmtp_proto::xmtp::v3::message_contents::vmac_unsigned_public_key::VodozemacCurve25519;

    use crate::ClientBuilder;

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
        contacts[0].vmac_identity_key();
        contacts[0].vmac_fallback_key();

        let key_bytes = contacts[0]
            .bundle
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
