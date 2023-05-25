use base64::{engine::general_purpose, Engine as _};
use prost::{DecodeError, Message};
use thiserror::Error;

use vodozemac::Curve25519PublicKey;
use xmtp_cryptography::hash::keccak256;
use xmtp_proto::xmtp::v3::message_contents::VmacContactBundle;

use crate::{utils::base64_encode, vmac_protos::ProtoWrapper};

#[derive(Debug, Error)]
pub enum ContactError {
    #[error("decode error")]
    Decode(#[from] DecodeError),
    #[error("unknown error")]
    Unknown,
}

pub struct Contact {
    pub(crate) bundle: VmacContactBundle,
}

impl Contact {
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, ContactError> {
        let bundle = VmacContactBundle::decode(bytes.as_slice())?;

        Ok(Self { bundle })
    }

    // The id of a contact is the base64 encoding of the keccak256 hash of the identity key
    pub fn id(&self) -> String {
        base64_encode(keccak256(self.identity_key().to_string().as_str()).as_slice())
    }

    pub fn identity_key(&self) -> Curve25519PublicKey {
        // TODO: Replace unwrap with proper error handling
        let proto_key = ProtoWrapper {
            proto: self.bundle.clone().identity_key.unwrap(),
        };

        proto_key.into()
    }

    pub fn fallback_key(&self) -> Curve25519PublicKey {
        let proto_key = ProtoWrapper {
            proto: self.bundle.clone().prekey.unwrap(),
        };

        proto_key.into()
    }
}
