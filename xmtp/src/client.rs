use xmtp_proto::xmtp::{
    message_api::v1::Envelope,
    v3::message_contents::{VmacAccountLinkedKey, VmacContactBundle, VmacDeviceLinkedKey},
};

use crate::{
    account::Account,
    persistence::{NamespacedPersistence, Persistence},
    utils::{build_envelope, build_user_contact_topic},
};

use prost::Message;

#[derive(Clone, Copy, Default)]
pub enum Network {
    Local(&'static str),
    #[default]
    Dev,
    Prod,
}

pub struct Client<P>
where
    P: Persistence,
{
    pub network: Network,
    pub persistence: NamespacedPersistence<P>,
    pub account: Account,
    // TODO: Replace this with wallet address derived from account
    pub wallet_address: String,
}

impl<P: Persistence> Client<P> {
    pub fn write_to_persistence(&mut self, s: &str, b: &[u8]) -> Result<(), P::Error> {
        self.persistence.write(s, b)
    }

    pub fn read_from_persistence(&self, s: &str) -> Result<Option<Vec<u8>>, P::Error> {
        self.persistence.read(s)
    }

    pub fn get_contacts_from_network(
        &self,
        wallet_address: &str,
    ) -> Result<Vec<VmacContactBundle, Error>> {
        let topic = build_user_contact_topic(wallet_address.to_string());
        let envelopes = self.network.get_messages(topic)?;

        let mut contacts = vec![];
        for envelope in envelopes {
            let contact_bundle = VmacContactBundle::decode(envelope.message.as_slice())?;
            contacts.push(contact_bundle);
        }

        Ok(contacts)
    }

    fn build_contact_envelope(&self) -> Result<Envelope, String> {
        let contact_bundle = self.account.proto_contact_bundle();
        let mut bytes = vec![];
        contact_bundle
            .encode(&mut bytes)
            .map_err(|e| format!("{}", e))?;

        let wallet_address = self.wallet_address.clone();

        let envelope = build_envelope(build_user_contact_topic(wallet_address), bytes);

        Ok(envelope)
    }
}
