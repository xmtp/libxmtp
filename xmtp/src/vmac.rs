use xmtp_proto::xmtp::v3::message_contents::{
    VmacAccountLinkedKey, VmacContactBundle, VmacDeviceLinkedKey, VmacUnsignedPublicKey,
};

use serde_json::json;
use vodozemac::{
    olm::{Account, SessionConfig},
};

use crate::vmac_traits::ProtoWrapper;

// Generate a VmacContactBundle
pub fn generate_test_contact_bundle() -> VmacContactBundle {
    let mut account = Account::new();
    // Get account identity key
    let identity_key = account.curve25519_key();
    account.generate_fallback_key();
    let fallback_key = account.fallback_key().values().next().unwrap().to_owned();

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

// Generate an outbound session (Olm Prekey Message) given a VmacContactBundle
pub fn generate_outbound_session(bundle: VmacContactBundle) -> Vec<u8> {
    let account = Account::new();

    let identity_key = bundle.identity_key.unwrap();
    let fallback_key = bundle.prekey.unwrap().key.unwrap();

    let identity_key = ProtoWrapper {
        proto: identity_key,
    }
    .into();
    let fallback_key = ProtoWrapper {
        proto: fallback_key,
    }
    .into();

    let mut session =
        account.create_outbound_session(SessionConfig::version_2(), identity_key, fallback_key);
    let message = session.encrypt("Hello, world!");
    json!(message).to_string().as_bytes().to_vec()
}
