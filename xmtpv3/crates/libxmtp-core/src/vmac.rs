use xmtp_proto::xmtp::v3::message_contents::vmac_unsigned_public_key::{
    Union, VodozemacCurve25519,
};
use xmtp_proto::xmtp::v3::message_contents::{
    VmacAccountLinkedKey, VmacContactBundle, VmacDeviceLinkedKey, VmacUnsignedPublicKey,
};

use serde_json::json;
use vodozemac::{
    olm::{Account, SessionConfig},
    Curve25519PublicKey,
};

// Can't impl From<Curve25519PublicKey> for VmacUnsignedPublicKey because of orphan rules
// - Need to come back and write a better translation trait, we can discuss
fn vmac_unsigned_public_key_from_curve25519_public_key(
    key: Curve25519PublicKey,
) -> VmacUnsignedPublicKey {
    VmacUnsignedPublicKey {
        created_ns: 0,
        union: Some(Union::Curve25519(VodozemacCurve25519 {
            bytes: key.to_bytes().to_vec(),
        })),
    }
}

fn account_linked_key_from_curve25519_public_key(key: Curve25519PublicKey) -> VmacAccountLinkedKey {
    VmacAccountLinkedKey {
        key: Some(vmac_unsigned_public_key_from_curve25519_public_key(key)),
    }
}

fn device_linked_key_from_curve25519_public_key(key: Curve25519PublicKey) -> VmacDeviceLinkedKey {
    VmacDeviceLinkedKey {
        key: Some(vmac_unsigned_public_key_from_curve25519_public_key(key)),
    }
}

fn curve25519_public_key_from_vmac_unsigned_public_key(
    key: VmacUnsignedPublicKey,
) -> Curve25519PublicKey {
    match key.union.unwrap() {
        Union::Curve25519(curve25519) => {
            Curve25519PublicKey::from_bytes(curve25519.bytes.as_slice().try_into().unwrap())
        }
    }
}

fn curve25519_public_key_from_vmac_account_linked_key(
    key: VmacAccountLinkedKey,
) -> Curve25519PublicKey {
    // Match on key.union
    match key.key.unwrap().union.unwrap() {
        Union::Curve25519(curve25519) => {
            Curve25519PublicKey::from_bytes(curve25519.bytes.as_slice().try_into().unwrap())
        }
    }
}

// Generate a VmacContactBundle
pub fn generate_test_contact_bundle() -> VmacContactBundle {
    let mut account = Account::new();
    // Get account identity key
    let identity_key = account.curve25519_key();
    account.generate_fallback_key();
    let fallback_key = account.fallback_key().values().next().unwrap().to_owned();

    let identity_key = account_linked_key_from_curve25519_public_key(identity_key);
    let fallback_key = device_linked_key_from_curve25519_public_key(fallback_key);
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

    let identity_key = curve25519_public_key_from_vmac_account_linked_key(identity_key);
    let fallback_key = curve25519_public_key_from_vmac_unsigned_public_key(fallback_key);

    let mut session =
        account.create_outbound_session(SessionConfig::version_2(), identity_key, fallback_key);
    let message = session.encrypt("Hello, world!");
    json!(message).to_string().as_bytes().to_vec()
}
