use openmls::prelude::UnknownExtension;
use openmls::prelude::{Ciphersuite, Extension};
use prost::{EncodeError, Message};
use xmtp_configuration::WELCOME_WRAPPER_ENCRYPTION_EXTENSION_ID;
use xmtp_cryptography::configuration::{CIPHERSUITE, POST_QUANTUM_CIPHERSUITE};
use xmtp_proto::ConversionError;
use xmtp_proto::xmtp::mls::message_contents::{
    WelcomePointerWrapperAlgorithm as WelcomePointerWrapperAlgorithmProto,
    WelcomeWrapperAlgorithm as WrapperAlgorithmProto,
    WelcomeWrapperEncryption as WelcomeWrapperEncryptionProto,
};

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum WrapperAlgorithm {
    Curve25519,
    XWingMLKEM768Draft6,
}

impl WrapperAlgorithm {
    pub fn to_mls_ciphersuite(self) -> Ciphersuite {
        match self {
            WrapperAlgorithm::Curve25519 => CIPHERSUITE,
            WrapperAlgorithm::XWingMLKEM768Draft6 => POST_QUANTUM_CIPHERSUITE,
        }
    }
    // hardcoded because the functions to do the translations are private
    // and placed here so that any changes to the this algorithm will have to be handled
    pub fn to_hpke_config(self) -> hpke_rs::Hpke<hpke_rs::libcrux::HpkeLibcrux> {
        let kem = match self {
            Self::Curve25519 => hpke_rs::hpke_types::KemAlgorithm::DhKem25519,
            Self::XWingMLKEM768Draft6 => hpke_rs::hpke_types::KemAlgorithm::XWingDraft06,
        };
        hpke_rs::Hpke::<hpke_rs::libcrux::HpkeLibcrux>::new(
            hpke_rs::Mode::Base,
            kem,
            hpke_rs::hpke_types::KdfAlgorithm::HkdfSha256,
            hpke_rs::hpke_types::AeadAlgorithm::ChaCha20Poly1305,
        )
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

impl TryFrom<WrapperAlgorithmProto> for WrapperAlgorithm {
    type Error = xmtp_proto::ConversionError;
    fn try_from(value: WrapperAlgorithmProto) -> Result<Self, Self::Error> {
        match value {
            WrapperAlgorithmProto::Curve25519 | WrapperAlgorithmProto::Unspecified => {
                Ok(WrapperAlgorithm::Curve25519)
            }
            WrapperAlgorithmProto::XwingMlkem768Draft6 => Ok(WrapperAlgorithm::XWingMLKEM768Draft6),
            WrapperAlgorithmProto::SymmetricKey => Err(xmtp_proto::ConversionError::InvalidValue {
                item: "WrapperAlgorithm",
                expected: "Curve25519 or XwingMlkem768Draft6",
                got: format!("{value:?}"),
            }),
        }
    }
}

impl From<WrapperAlgorithm> for i32 {
    fn from(value: WrapperAlgorithm) -> Self {
        let proto_val: WrapperAlgorithmProto = value.into();
        proto_val as i32
    }
}

impl TryFrom<i32> for WrapperAlgorithm {
    type Error = xmtp_proto::ConversionError;
    fn try_from(value: i32) -> Result<Self, Self::Error> {
        let algorithm = match value {
            1 => WrapperAlgorithm::Curve25519, // WrapperAlgorithmProto::Curve25519
            2 => WrapperAlgorithm::XWingMLKEM768Draft6, // WrapperAlgorithmProto::XwingMlkem512
            3 => {
                return Err(xmtp_proto::ConversionError::InvalidValue {
                    item: "WrapperAlgorithm",
                    expected: "1 or 2",
                    got: value.to_string(),
                });
            }
            _ => WrapperAlgorithm::Curve25519, // Everything else including unknown
        };
        Ok(algorithm)
    }
}

impl TryFrom<WelcomePointerWrapperAlgorithmProto> for WrapperAlgorithm {
    type Error = xmtp_proto::ConversionError;
    fn try_from(value: WelcomePointerWrapperAlgorithmProto) -> Result<Self, Self::Error> {
        match value {
            WelcomePointerWrapperAlgorithmProto::XwingMlkem768Draft6 => {
                Ok(WrapperAlgorithm::XWingMLKEM768Draft6)
            }
            _ => Err(xmtp_proto::ConversionError::InvalidValue {
                item: "WrapperAlgorithm",
                expected: "XwingMlkem768Draft6",
                got: format!("{value:?}"),
            }),
        }
    }
}

impl TryFrom<WrapperAlgorithm> for WelcomePointerWrapperAlgorithmProto {
    type Error = xmtp_proto::ConversionError;
    fn try_from(value: WrapperAlgorithm) -> Result<Self, Self::Error> {
        match value {
            WrapperAlgorithm::XWingMLKEM768Draft6 => {
                Ok(WelcomePointerWrapperAlgorithmProto::XwingMlkem768Draft6)
            }
            _ => Err(xmtp_proto::ConversionError::InvalidValue {
                item: "WrapperAlgorithm",
                expected: "XwingMlkem768Draft6",
                got: format!("{value:?}"),
            }),
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

impl TryFrom<&UnknownExtension> for WrapperEncryptionExtension {
    type Error = ConversionError;

    fn try_from(value: &UnknownExtension) -> Result<Self, Self::Error> {
        value.0.as_slice().try_into()
    }
}

impl TryFrom<&[u8]> for WrapperEncryptionExtension {
    type Error = ConversionError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let proto = WelcomeWrapperEncryptionProto::decode(value)?;
        let algorithm: WrapperAlgorithm = proto.algorithm.try_into()?;
        Ok(WrapperEncryptionExtension {
            algorithm,
            pub_key_bytes: proto.pub_key,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[xmtp_common::test]
    fn test_serialization() {
        let algorithm = WrapperAlgorithm::XWingMLKEM768Draft6;
        let pub_key_bytes = xmtp_common::rand_vec::<32>();

        let extension = WrapperEncryptionExtension::new(algorithm, pub_key_bytes.clone());

        let mls_extension: Extension = extension.try_into().unwrap();

        let Extension::Unknown(id, unknown_extension) = mls_extension else {
            panic!("Expected unknown extension");
        };

        assert_eq!(id, WELCOME_WRAPPER_ENCRYPTION_EXTENSION_ID);

        let deserialized: WrapperEncryptionExtension = (&unknown_extension).try_into().unwrap();

        assert_eq!(deserialized.algorithm, algorithm);
        assert_eq!(deserialized.pub_key_bytes, pub_key_bytes);
    }
}
