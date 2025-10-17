use crate::types::{Cursor, InstallationId};
use crate::{
    ConversionError,
    xmtp::mls::message_contents::{
        WelcomePointeeEncryptionAeadType, WelcomePointerWrapperAlgorithm, WelcomeWrapperAlgorithm,
    },
};
use chrono::Utc;
use derive_builder::Builder;
use prost::Message;

/// Welcome Message from the network
#[derive(Clone, Builder, Debug)]
#[builder(setter(into), build_fn(error = "ConversionError"))]
pub struct WelcomeMessage {
    /// cursor of this message
    pub cursor: Cursor,
    /// server timestamp indicating when this message was created
    pub created_ns: chrono::DateTime<Utc>,
    /// Variant of the welcome message
    pub variant: WelcomeMessageType,
}

impl WelcomeMessage {
    pub fn builder() -> WelcomeMessageBuilder {
        WelcomeMessageBuilder::default()
    }
    pub fn as_v1(&self) -> Option<&WelcomeMessageV1> {
        match &self.variant {
            WelcomeMessageType::V1(v1) => Some(v1),
            _ => None,
        }
    }
}

impl WelcomeMessage {
    pub fn sequence_id(&self) -> u64 {
        self.cursor.sequence_id
    }

    pub fn originator_id(&self) -> u32 {
        self.cursor.originator_id
    }

    pub fn timestamp(&self) -> i64 {
        self.created_ns
            .timestamp_nanos_opt()
            .expect("timestamp out of range for i64, are we in 2262 A.D?")
    }

    pub fn resuming(&self) -> bool {
        matches!(
            &self.variant,
            WelcomeMessageType::DecryptedWelcomePointer(_)
        )
    }
}

#[derive(Clone, Debug)]
pub enum WelcomeMessageType {
    V1(WelcomeMessageV1),
    WelcomePointer(WelcomePointer),
    DecryptedWelcomePointer(DecryptedWelcomePointer),
}

impl From<WelcomeMessageV1> for WelcomeMessageType {
    fn from(v1: WelcomeMessageV1) -> Self {
        WelcomeMessageType::V1(v1)
    }
}

impl From<WelcomePointer> for WelcomeMessageType {
    fn from(pointer: WelcomePointer) -> Self {
        WelcomeMessageType::WelcomePointer(pointer)
    }
}

impl From<DecryptedWelcomePointer> for WelcomeMessageType {
    fn from(pointer: DecryptedWelcomePointer) -> Self {
        WelcomeMessageType::DecryptedWelcomePointer(pointer)
    }
}

#[derive(Clone, Builder, Debug)]
#[builder(build_fn(error = "ConversionError"))]
pub struct WelcomeMessageV1 {
    // Installation key the welcome was sent to
    pub installation_key: InstallationId,
    // HPKE public key used to encrypt the welcome
    pub hpke_public_key: Vec<u8>,
    // Wrapper algorithm used to encrypt the welcome
    pub wrapper_algorithm: WelcomeWrapperAlgorithm,
    // Encrypted welcome message payload
    pub data: Vec<u8>,
    // Encrypted welcome metadata
    pub welcome_metadata: Vec<u8>,
}

impl WelcomeMessageV1 {
    pub fn builder() -> WelcomeMessageV1Builder {
        WelcomeMessageV1Builder::default()
    }
}

#[derive(Clone, Builder, Debug)]
#[builder(build_fn(error = "ConversionError"))]
pub struct WelcomePointer {
    // Installation key the welcome pointer was sent to
    pub installation_key: InstallationId,
    // HPKE public key used to encrypt the welcome pointer
    pub hpke_public_key: Vec<u8>,
    // Wrapper algorithm used to encrypt the welcome pointer (Only post quantum compatible algorithms are allowed)
    pub wrapper_algorithm: WelcomePointerWrapperAlgorithm,
    // Encrypted welcome pointer data
    pub welcome_pointer: Vec<u8>,
}

impl WelcomePointer {
    pub fn builder() -> WelcomePointerBuilder {
        WelcomePointerBuilder::default()
    }
}

#[derive(Clone, Builder, Debug)]
#[builder(build_fn(error = "ConversionError"))]
pub struct DecryptedWelcomePointer {
    // Topic the welcome pointee was sent to
    pub destination: InstallationId,
    // AEAD type used to encrypt the welcome pointee
    pub aead_type: WelcomePointeeEncryptionAeadType,
    // Encryption key used to encrypt the welcome pointee. Length MUST match the aead_type.
    pub encryption_key: Vec<u8>,
    // Nonce used to encrypt the welcome pointee data. Length MUST match the aead_type.
    pub data_nonce: Vec<u8>,
    // Nonce used to encrypt the welcome pointee metadata. Length MUST match the aead_type.
    pub welcome_metadata_nonce: Vec<u8>,
}

impl DecryptedWelcomePointer {
    pub fn builder() -> DecryptedWelcomePointerBuilder {
        DecryptedWelcomePointerBuilder::default()
    }
    pub fn decode(data: &[u8]) -> Result<Self, ConversionError> {
        let wp = crate::xmtp::mls::message_contents::WelcomePointer::decode(data)?;
        let wp = match wp.version {
            Some(
                crate::xmtp::mls::message_contents::welcome_pointer::Version::WelcomeV1Pointer(v1),
            ) => v1,
            None => {
                return Err(ConversionError::InvalidValue {
                    item: "WelcomePointer",
                    expected: "WelcomeV1Pointer",
                    got: "None".into(),
                });
            }
        };
        Ok(Self {
            destination: wp.destination.try_into()?,
            aead_type: wp.aead_type.try_into()?,
            encryption_key: wp.encryption_key,
            data_nonce: wp.data_nonce,
            welcome_metadata_nonce: wp.welcome_metadata_nonce,
        })
    }
    pub fn to_proto(self) -> crate::xmtp::mls::message_contents::WelcomePointer {
        crate::xmtp::mls::message_contents::WelcomePointer {
            version: Some(
                crate::xmtp::mls::message_contents::welcome_pointer::Version::WelcomeV1Pointer(
                    crate::xmtp::mls::message_contents::welcome_pointer::WelcomeV1Pointer {
                        destination: self.destination.to_vec(),
                        aead_type: self.aead_type.into(),
                        encryption_key: self.encryption_key,
                        data_nonce: self.data_nonce,
                        welcome_metadata_nonce: self.welcome_metadata_nonce,
                    },
                ),
            ),
        }
    }
}

impl TryFrom<crate::xmtp::mls::message_contents::WelcomePointer> for DecryptedWelcomePointer {
    type Error = ConversionError;
    fn try_from(
        value: crate::xmtp::mls::message_contents::WelcomePointer,
    ) -> Result<Self, Self::Error> {
        let wp = match value.version {
            Some(
                crate::xmtp::mls::message_contents::welcome_pointer::Version::WelcomeV1Pointer(v1),
            ) => v1,
            None => {
                return Err(ConversionError::InvalidValue {
                    item: "WelcomePointer",
                    expected: "WelcomeV1Pointer",
                    got: "None".into(),
                });
            }
        };
        Ok(Self {
            destination: wp.destination.try_into()?,
            aead_type: wp.aead_type.try_into()?,
            encryption_key: wp.encryption_key,
            data_nonce: wp.data_nonce,
            welcome_metadata_nonce: wp.welcome_metadata_nonce,
        })
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl xmtp_common::Generate for WelcomeMessage {
    fn generate() -> Self {
        Self {
            cursor: Cursor::generate(),
            created_ns: chrono::DateTime::from_timestamp_nanos(xmtp_common::rand_i64()),
            variant: WelcomeMessageV1 {
                installation_key: xmtp_common::rand_array::<32>().into(),
                data: xmtp_common::rand_vec::<16>(),
                hpke_public_key: xmtp_common::rand_vec::<16>(),
                wrapper_algorithm: WelcomeWrapperAlgorithm::Curve25519,
                welcome_metadata: xmtp_common::rand_vec::<16>(),
            }
            .into(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use rstest::rstest;
    use xmtp_common::Generate;

    #[rstest]
    #[case(Cursor::new(123, 456u32), 123, 456u32)]
    #[case(Cursor::new(0, 0u32), 0, 0u32)]
    #[case(Cursor::new(u64::MAX, u32::MAX), u64::MAX, u32::MAX)]
    #[xmtp_common::test]
    fn test_accessor_methods(
        #[case] cursor: Cursor,
        #[case] expected_seq: u64,
        #[case] expected_orig: u32,
    ) {
        use xmtp_common::Generate;

        let mut welcome_message = WelcomeMessage::generate();
        welcome_message.cursor = cursor;
        assert_eq!(welcome_message.sequence_id(), expected_seq);
        assert_eq!(welcome_message.originator_id(), expected_orig);
    }

    #[xmtp_common::test]
    fn test_timestamp() {
        let test_time = chrono::Utc::now();
        let mut welcome_message = WelcomeMessage::generate();
        welcome_message.created_ns = test_time;
        assert_eq!(
            welcome_message.timestamp(),
            test_time.timestamp_nanos_opt().unwrap()
        );
    }
}
