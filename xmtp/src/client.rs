use xmtp_proto::xmtp::v3::message_contents::{
    VmacAccountLinkedKey, VmacContactBundle, VmacDeviceLinkedKey, VmacUnsignedPublicKey,
};

use prost::Message;
use xmtp_proto::xmtp::message_api::v1::Envelope;

use crate::{
    account::VmacAccount,
    networking::XmtpApiClient,
    persistence::{NamespacedPersistence, Persistence},
    utils,
    vmac_protos::ProtoWrapper,
};

#[derive(Clone, Copy, Default)]
pub enum Network {
    Local(&'static str),
    #[default]
    Dev,
    Prod,
}

impl Network {
    pub fn url(&self) -> &'static str {
        match self {
            Network::Local(url) => url,
            Network::Dev => "https://dev.xmtp.network",
            Network::Prod => "https://production.xmtp.network",
        }
    }
}

pub struct Client<P, A>
where
    P: Persistence,
    A: XmtpApiClient + Sized,
{
    pub network: Network,
    pub persistence: NamespacedPersistence<P>,
    pub account: VmacAccount,
    pub api_client: Box<A>,
    pub wallet_address: String,
}

impl<P: Persistence, A: XmtpApiClient + Sized> Client<P, A> {
    pub fn publish_contact_bundle(&mut self) -> Result<(), String> {
        let envelope = self.build_contact_envelope()?;
        let api_client = self.api_client.as_mut();
        api_client.publish("/contact".to_string(), vec![envelope])?;

        Ok(())
    }

    fn build_contact_envelope(&self) -> Result<Envelope, String> {
        let contact_bundle = self.build_proto_contact_bundle();
        let mut bytes = vec![];
        contact_bundle
            .encode(&mut bytes)
            .map_err(|e| format!("{}", e))?;

        let wallet_address = self.wallet_address.clone();

        let envelope = self.build_envelope(utils::build_user_contact_topic(wallet_address), bytes);

        Ok(envelope)
    }

    fn build_envelope(&self, content_topic: String, message: Vec<u8>) -> Envelope {
        Envelope {
            content_topic,
            message,
            timestamp_ns: utils::get_current_time_ns(),
        }
    }

    fn build_proto_contact_bundle(&self) -> VmacContactBundle {
        let identity_key = self.account.account.curve25519_key();
        let fallback_key = self
            .account
            .account
            .fallback_key()
            .values()
            .next()
            .unwrap()
            .to_owned();

        let identity_key_proto: ProtoWrapper<VmacUnsignedPublicKey> = identity_key.into();
        let fallback_key_proto: ProtoWrapper<VmacUnsignedPublicKey> = fallback_key.into();
        let identity_key = VmacAccountLinkedKey {
            key: Some(identity_key_proto.proto),
        };
        let fallback_key = VmacDeviceLinkedKey {
            key: Some(fallback_key_proto.proto),
        };
        VmacContactBundle {
            identity_key: Some(identity_key),
            prekey: Some(fallback_key),
        }
    }

    pub fn write_to_persistence(&mut self, s: &str, b: &[u8]) -> Result<(), String> {
        self.persistence.write(s, b)
    }

    pub fn read_from_persistence(&self, s: &str) -> Result<Option<Vec<u8>>, String> {
        self.persistence.read(s)
    }
}

#[cfg(test)]
mod tests {
    use xmtp_proto::xmtp::v3::message_contents::VmacContactBundle;

    use crate::{builder::ClientBuilder, networking::XmtpApiClient};

    use prost::Message;

    #[test]
    fn can_pass_persistence_methods() {
        let mut client = ClientBuilder::new_test().build().unwrap();
        assert_eq!(client.read_from_persistence("foo").unwrap(), None);
        client.write_to_persistence("foo", b"bar").unwrap();
        assert_eq!(
            client.read_from_persistence("foo").unwrap(),
            Some(b"bar".to_vec())
        );
    }

    #[test]
    fn can_publish_contact_bundle() {
        let mut client = ClientBuilder::new_test().build().unwrap();
        client.publish_contact_bundle().unwrap();

        let results = client
            .api_client
            .query("/contact".to_string(), None, None, None)
            .unwrap();
        assert_eq!(results.envelopes.len(), 1);

        let bundle_bytes: &Vec<u8> = results.envelopes[0].message.as_ref();
        let bundle = VmacContactBundle::decode(&bundle_bytes[..]).unwrap();
        assert!(bundle.identity_key.is_some());
        assert!(bundle.prekey.is_some());
    }
}
