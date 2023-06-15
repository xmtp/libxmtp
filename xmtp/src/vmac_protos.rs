use prost::EncodeError;
// Generic wrapper for proto classes, so we can implement From trait without violating orphan rules
use prost::Message;
use vodozemac::Curve25519PublicKey;
use xmtp_proto::xmtp::v3::message_contents::vmac_unsigned_public_key::{
    Union, VodozemacCurve25519,
};
use xmtp_proto::xmtp::v3::message_contents::{
    InvitationV1, VmacAccountLinkedKey, VmacInstallationLinkedKey, VmacUnsignedPublicKey,
};

pub struct ProtoWrapper<T> {
    pub proto: T,
}

impl From<Curve25519PublicKey> for ProtoWrapper<VmacUnsignedPublicKey> {
    fn from(key: Curve25519PublicKey) -> Self {
        ProtoWrapper {
            proto: VmacUnsignedPublicKey {
                // TODO: this timestamp is hardcoded to 0 for now, this conversion is lossy
                created_ns: 0,
                union: Some(Union::Curve25519(VodozemacCurve25519 {
                    bytes: key.to_bytes().to_vec(),
                })),
            },
        }
    }
}

impl From<ProtoWrapper<VmacUnsignedPublicKey>> for Curve25519PublicKey {
    fn from(key: ProtoWrapper<VmacUnsignedPublicKey>) -> Self {
        match key.proto.union.unwrap() {
            Union::Curve25519(curve25519) => {
                Curve25519PublicKey::from_bytes(curve25519.bytes.as_slice().try_into().unwrap())
            }
        }
    }
}

impl From<ProtoWrapper<VmacAccountLinkedKey>> for Curve25519PublicKey {
    fn from(key: ProtoWrapper<VmacAccountLinkedKey>) -> Self {
        match key.proto.key.unwrap().union.unwrap() {
            Union::Curve25519(curve25519) => {
                Curve25519PublicKey::from_bytes(curve25519.bytes.as_slice().try_into().unwrap())
            }
        }
    }
}

impl From<ProtoWrapper<VmacInstallationLinkedKey>> for Curve25519PublicKey {
    fn from(key: ProtoWrapper<VmacInstallationLinkedKey>) -> Self {
        match key.proto.key.unwrap().union.unwrap() {
            Union::Curve25519(curve25519) => {
                Curve25519PublicKey::from_bytes(curve25519.bytes.as_slice().try_into().unwrap())
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use vodozemac::olm::{Account, SessionConfig};
    use xmtp_proto::xmtp::v3::message_contents::{
        VmacAccountLinkedKey, VmacInstallationLinkedKey, VmacInstallationPublicKeyBundleV1,
        VmacUnsignedPublicKey,
    };

    // Generate a VmacContactBundle
    fn generate_test_contact_bundle() -> VmacInstallationPublicKeyBundleV1 {
        let mut account = Account::new();
        // Get account identity key
        let identity_key = account.curve25519_key();
        account.generate_fallback_key();
        let fallback_key = account.fallback_key().values().next().unwrap().to_owned();

        let identity_key_proto: ProtoWrapper<VmacUnsignedPublicKey> = identity_key.into();
        let fallback_key_proto: ProtoWrapper<VmacUnsignedPublicKey> = fallback_key.into();
        let identity_key = VmacAccountLinkedKey {
            key: Some(identity_key_proto.proto),
            association: None,
        };
        let fallback_key = VmacInstallationLinkedKey {
            key: Some(fallback_key_proto.proto),
        };
        VmacInstallationPublicKeyBundleV1 {
            identity_key: Some(identity_key),
            fallback_key: Some(fallback_key),
        }
    }

    #[test]
    fn test_can_generate_test_contact_bundle_and_session() {
        let bundle = generate_test_contact_bundle();
        assert!(bundle.identity_key.is_some());
        assert!(bundle.fallback_key.is_some());

        let account = Account::new();

        let identity_key = bundle.identity_key.unwrap();
        let fallback_key = bundle.fallback_key.unwrap().key.unwrap();

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
        // Assert message is not empty
        assert!(!message.message().is_empty());
    }
}
