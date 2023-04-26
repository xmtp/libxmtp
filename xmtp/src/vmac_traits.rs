use xmtp_proto::xmtp::v3::message_contents::vmac_unsigned_public_key::{
    Union, VodozemacCurve25519,
};
use xmtp_proto::xmtp::v3::message_contents::{
    VmacAccountLinkedKey, VmacDeviceLinkedKey, VmacUnsignedPublicKey,
};


use vodozemac::{
    Curve25519PublicKey,
};

// Generic wrapper for proto classes, so we can implement From trait without violating orphan rules
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

impl From<ProtoWrapper<VmacDeviceLinkedKey>> for Curve25519PublicKey {
    fn from(key: ProtoWrapper<VmacDeviceLinkedKey>) -> Self {
        match key.proto.key.unwrap().union.unwrap() {
            Union::Curve25519(curve25519) => {
                Curve25519PublicKey::from_bytes(curve25519.bytes.as_slice().try_into().unwrap())
            }
        }
    }
}
