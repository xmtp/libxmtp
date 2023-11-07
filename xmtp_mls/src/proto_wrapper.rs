use xmtp_proto::xmtp::v3::message_contents::{
    vmac_unsigned_public_key::{Union, VodozemacCurve25519},
    VmacAccountLinkedKey, VmacInstallationLinkedKey, VmacUnsignedPublicKey,
};

// Generic wrapper for proto classes, so we can implement From trait without violating orphan rules
pub struct ProtoWrapper<T> {
    pub proto: T,
}

pub type InstallationPublicKey = Vec<u8>;

impl From<InstallationPublicKey> for ProtoWrapper<VmacUnsignedPublicKey> {
    fn from(public_key_bytes: InstallationPublicKey) -> Self {
        ProtoWrapper {
            proto: VmacUnsignedPublicKey {
                // TODO: this timestamp is hardcoded to 0 for now, this conversion is lossy
                created_ns: 0,
                union: Some(Union::Curve25519(VodozemacCurve25519 {
                    bytes: public_key_bytes,
                })),
            },
        }
    }
}

impl From<ProtoWrapper<VmacUnsignedPublicKey>> for InstallationPublicKey {
    fn from(key: ProtoWrapper<VmacUnsignedPublicKey>) -> Self {
        match key.proto.union.unwrap() {
            Union::Curve25519(curve25519) => curve25519.bytes,
        }
    }
}

impl From<ProtoWrapper<VmacAccountLinkedKey>> for InstallationPublicKey {
    fn from(key: ProtoWrapper<VmacAccountLinkedKey>) -> Self {
        match key.proto.key.unwrap().union.unwrap() {
            Union::Curve25519(curve25519) => curve25519.bytes,
        }
    }
}

impl From<ProtoWrapper<VmacInstallationLinkedKey>> for InstallationPublicKey {
    fn from(key: ProtoWrapper<VmacInstallationLinkedKey>) -> Self {
        match key.proto.key.unwrap().union.unwrap() {
            Union::Curve25519(curve25519) => curve25519.bytes,
        }
    }
}

