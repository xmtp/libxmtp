use crate::{
    account::Account,
    networking::XmtpApiClient,
    persistence::{NamespacedPersistence, Persistence},
    types::Address,
    utils::{build_envelope, build_user_contact_topic},
};
use prost::Message;
use xmtp_proto::xmtp::{message_api::v1::Envelope, v3::message_contents::VmacContactBundle};

#[derive(Clone, Copy, Default)]
pub enum Network {
    Local(&'static str),
    #[default]
    Dev,
    Prod,
}

pub struct Client<A, P>
where
    A: XmtpApiClient,
    P: Persistence,
{
    pub api_client: A,
    pub network: Network,
    pub persistence: NamespacedPersistence<P>,
    // TODO: Make account private. Just need to figure out how to access for tests
    pub account: Account,
}

impl<A: XmtpApiClient, P: Persistence> Client<A, P> {
    pub fn write_to_persistence(&mut self, s: &str, b: &[u8]) -> Result<(), P::Error> {
        self.persistence.write(s, b)
    }

    pub fn read_from_persistence(&self, s: &str) -> Result<Option<Vec<u8>>, P::Error> {
        self.persistence.read(s)
    }

    pub fn wallet_address(&self) -> Address {
        self.account.addr()
    }

    pub async fn get_contacts(
        &self,
        wallet_address: &str,
    ) -> Result<Vec<VmacContactBundle>, String> {
        let topic = build_user_contact_topic(wallet_address.to_string());
        let response = self.api_client.query(topic, None, None, None).await?;

        let mut contacts = vec![];
        for envelope in response.envelopes {
            // TODO: Wrap the proto in some special struct
            // TODO: Handle errors better
            let contact_bundle = VmacContactBundle::decode(envelope.message.as_slice())
                .map_err(|e| format!("{}", e))?;
            contacts.push(contact_bundle);
        }

        Ok(contacts)
    }

    pub async fn publish_user_contact(&mut self) -> Result<(), String> {
        let envelope = self.build_contact_envelope()?;
        self.api_client
            .publish("".to_string(), vec![envelope])
            .await?;

        Ok(())
    }

    fn build_contact_envelope(&self) -> Result<Envelope, String> {
        let contact_bundle = self.account.proto_contact_bundle();
        let mut bytes = vec![];
        contact_bundle
            .encode(&mut bytes)
            .map_err(|e| format!("{}", e))?;

        let envelope = build_envelope(build_user_contact_topic(self.wallet_address()), bytes);

        Ok(envelope)
    }
}

#[cfg(test)]
mod tests {
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
        assert!(contacts[0].prekey.is_some());
        assert!(contacts[0].identity_key.is_some());
    }
}
