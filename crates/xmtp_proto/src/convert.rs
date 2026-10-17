use crate::ConversionError;
use crate::xmtp::identity::MlsCredential;
use openmls::{
    credentials::{BasicCredential, errors::BasicCredentialError},
    prelude::Credential as OpenMlsCredential,
};
use prost::Message;

impl From<crate::xmtp::mls::message_contents::ContentTypeId> for xmtp_common::types::ContentTypeId {
    fn from(value: crate::xmtp::mls::message_contents::ContentTypeId) -> Self {
        Self {
            authority_id: value.authority_id,
            type_id: value.type_id,
            version_major: value.version_major,
        }
    }
}

impl TryFrom<MlsCredential> for OpenMlsCredential {
    type Error = BasicCredentialError;

    fn try_from(proto: MlsCredential) -> Result<OpenMlsCredential, Self::Error> {
        let bytes = proto.encode_to_vec();
        Ok(BasicCredential::new(bytes).into())
    }
}

impl TryFrom<openmls::prelude::AeadType>
    for crate::xmtp::mls::message_contents::WelcomePointeeEncryptionAeadType
{
    type Error = ConversionError;

    fn try_from(aead_type: openmls::prelude::AeadType) -> Result<Self, Self::Error> {
        match aead_type {
            openmls::prelude::AeadType::ChaCha20Poly1305 => {
                Ok(crate::xmtp::mls::message_contents::WelcomePointeeEncryptionAeadType::Chacha20Poly1305)
            }
            openmls::prelude::AeadType::Aes128Gcm | openmls::prelude::AeadType::Aes256Gcm => Err(ConversionError::InvalidValue {
                item: "AeadType",
                expected: "ChaCha20Poly1305",
                got: format!("{:?}", aead_type),
            }),
        }
    }
}

impl TryFrom<crate::xmtp::mls::message_contents::WelcomePointeeEncryptionAeadType>
    for openmls::prelude::AeadType
{
    type Error = ConversionError;

    fn try_from(
        aead_type: crate::xmtp::mls::message_contents::WelcomePointeeEncryptionAeadType,
    ) -> Result<Self, Self::Error> {
        match aead_type {
            crate::xmtp::mls::message_contents::WelcomePointeeEncryptionAeadType::Chacha20Poly1305 => {
                Ok(openmls::prelude::AeadType::ChaCha20Poly1305)
            }
            crate::xmtp::mls::message_contents::WelcomePointeeEncryptionAeadType::Unspecified => {
                Err(ConversionError::InvalidValue {
                    item: "AeadType",
                    expected: "ChaCha20Poly1305",
                    got: format!("{:?}", aead_type),
                })
            }
        }
    }
}

#[cfg(test)]
mod content_type_tests {
    #[xmtp_common::test]
    fn proto_minor_version_does_not_change_codec_id() {
        let proto = crate::xmtp::mls::message_contents::ContentTypeId {
            authority_id: "xmtp.org".into(),
            type_id: "text".into(),
            version_major: 1,
            version_minor: 4,
        };
        let id = xmtp_common::types::ContentTypeId::from(proto.clone());
        assert_eq!(
            id,
            xmtp_common::types::ContentTypeId::from(
                crate::xmtp::mls::message_contents::ContentTypeId {
                    version_minor: 9,
                    ..proto
                }
            )
        );
    }
}
