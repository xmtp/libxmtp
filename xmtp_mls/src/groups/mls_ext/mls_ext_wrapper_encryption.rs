use openmls::prelude::UnknownExtension;
use openmls::prelude::{Ciphersuite, Extension};
use prost::Message;
use prost::{DecodeError, EncodeError};
use xmtp_cryptography::configuration::{CIPHERSUITE, POST_QUANTUM_CIPHERSUITE};
use xmtp_mls_common::config::WELCOME_WRAPPER_ENCRYPTION_EXTENSION_ID;
use xmtp_proto::xmtp::mls::message_contents::{
    WelcomeWrapperAlgorithm as WrapperAlgorithmProto,
    WelcomeWrapperEncryption as WelcomeWrapperEncryptionProto,
};

#[derive(Debug)]
pub enum WrapperAlgorithm {
    Curve25519,
    XWingMLKEM768Draft6,
}

impl WrapperAlgorithm {
    pub fn to_mls_ciphersuite(&self) -> Ciphersuite {
        match self {
            WrapperAlgorithm::Curve25519 => CIPHERSUITE,
            WrapperAlgorithm::XWingMLKEM768Draft6 => POST_QUANTUM_CIPHERSUITE,
        }
    }
}

impl From<WrapperAlgorithm> for WrapperAlgorithmProto {
    fn from(value: WrapperAlgorithm) -> Self {
        match value {
            WrapperAlgorithm::Curve25519 => WrapperAlgorithmProto::Curve25519,
            WrapperAlgorithm::XWingMLKEM768Draft6 => WrapperAlgorithmProto::XwingMlkem768Draft6,
        }
    }
}

impl From<WrapperAlgorithm> for i32 {
    fn from(value: WrapperAlgorithm) -> Self {
        let proto_val: WrapperAlgorithmProto = value.into();
        proto_val as i32
    }
}

impl From<i32> for WrapperAlgorithm {
    fn from(value: i32) -> Self {
        match value {
            1 => WrapperAlgorithm::Curve25519, // WrapperAlgorithmProto::Curve25519
            2 => WrapperAlgorithm::XWingMLKEM768Draft6, // WrapperAlgorithmProto::XwingMlkem512
            _ => WrapperAlgorithm::Curve25519, // Everything else including unknown
        }
    }
}
#[derive(Debug)]
pub struct WrapperEncryptionExtension {
    pub algorithm: WrapperAlgorithm,
    pub pub_key_bytes: Vec<u8>,
}

impl WrapperEncryptionExtension {
    pub fn new(algorithm: WrapperAlgorithm, pub_key_bytes: Vec<u8>) -> Self {
        Self {
            algorithm,
            pub_key_bytes,
        }
    }
}

impl TryFrom<WrapperEncryptionExtension> for Extension {
    type Error = EncodeError;

    fn try_from(value: WrapperEncryptionExtension) -> Result<Self, Self::Error> {
        let proto_val = WelcomeWrapperEncryptionProto {
            pub_key: value.pub_key_bytes,
            algorithm: value.algorithm.into(),
        };
        let mut buf = Vec::new();
        proto_val.encode(&mut buf)?;

        Ok(Extension::Unknown(
            WELCOME_WRAPPER_ENCRYPTION_EXTENSION_ID,
            UnknownExtension(buf),
        ))
    }
}

impl TryFrom<UnknownExtension> for WrapperEncryptionExtension {
    type Error = DecodeError;

    fn try_from(value: UnknownExtension) -> Result<Self, Self::Error> {
        value.0.try_into()
    }
}

impl TryFrom<Vec<u8>> for WrapperEncryptionExtension {
    type Error = DecodeError;

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        let proto = WelcomeWrapperEncryptionProto::decode(&mut value.as_slice())?;
        let algorithm: WrapperAlgorithm = proto.algorithm.into();
        Ok(WrapperEncryptionExtension {
            algorithm,
            pub_key_bytes: proto.pub_key,
        })
    }
}
