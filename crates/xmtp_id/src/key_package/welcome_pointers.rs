use openmls::prelude::UnknownExtension;
use openmls::prelude::{AeadType, Extension};
use prost::{EncodeError, Message};
use xmtp_configuration::WELCOME_POINTEE_ENCRYPTION_AEAD_TYPES_EXTENSION_ID;
use xmtp_proto::ConversionError;
use xmtp_proto::xmtp::mls::message_contents::{
    WelcomePointeeEncryptionAeadType as WelcomePointeeEncryptionAeadTypeProto,
    WelcomePointeeEncryptionAeadTypesExtension as WelcomePointeeEncryptionAeadTypesExtensionProto,
};

#[derive(Debug, Clone)]
pub struct WelcomePointersExtension {
    pub supported_aead_types: Vec<AeadType>,
}

impl WelcomePointersExtension {
    pub fn new(supported_aead_types: Vec<AeadType>) -> Self {
        Self {
            supported_aead_types,
        }
    }
    pub fn available_types() -> Self {
        Self::new(vec![Self::preferred_type()])
    }
    pub fn empty() -> Self {
        Self::new(vec![])
    }
    pub const fn preferred_type() -> AeadType {
        AeadType::ChaCha20Poly1305
    }
    pub fn compatible(&self) -> bool {
        self.supported_aead_types.contains(&Self::preferred_type())
    }
}

impl TryFrom<WelcomePointersExtension> for Extension {
    type Error = EncodeError;

    fn try_from(value: WelcomePointersExtension) -> Result<Self, Self::Error> {
        let proto_val: WelcomePointeeEncryptionAeadTypesExtensionProto = value.into();
        let mut buf = Vec::with_capacity(proto_val.encoded_len());
        proto_val.encode(&mut buf)?;

        Ok(Extension::Unknown(
            WELCOME_POINTEE_ENCRYPTION_AEAD_TYPES_EXTENSION_ID,
            UnknownExtension(buf),
        ))
    }
}

impl TryFrom<&UnknownExtension> for WelcomePointersExtension {
    type Error = ConversionError;

    fn try_from(value: &UnknownExtension) -> Result<Self, Self::Error> {
        value.0.as_slice().try_into()
    }
}

impl TryFrom<&[u8]> for WelcomePointersExtension {
    type Error = ConversionError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let proto = WelcomePointeeEncryptionAeadTypesExtensionProto::decode(value)?;
        let supported_aead_types: Vec<AeadType> = proto
            .supported_aead_types
            .iter()
            .copied()
            .map(|aead_type| {
                WelcomePointeeEncryptionAeadTypeProto::try_from(aead_type)
                    .map_err(ConversionError::UnknownEnumValue)
                    .and_then(TryInto::try_into)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(WelcomePointersExtension {
            supported_aead_types,
        })
    }
}

impl From<WelcomePointeeEncryptionAeadTypesExtensionProto> for WelcomePointersExtension {
    fn from(value: WelcomePointeeEncryptionAeadTypesExtensionProto) -> Self {
        Self {
            supported_aead_types: value
                .supported_aead_types
                .into_iter()
                // Ignore any values that are not valid because they cannot be used
                .filter_map(|aead_type| {
                    WelcomePointeeEncryptionAeadTypeProto::try_from(aead_type).ok()
                })
                .filter_map(|aead_type| aead_type.try_into().ok())
                .collect(),
        }
    }
}

impl From<WelcomePointersExtension> for WelcomePointeeEncryptionAeadTypesExtensionProto {
    fn from(value: WelcomePointersExtension) -> Self {
        Self {
            supported_aead_types: value
                .supported_aead_types
                .into_iter()
                // Ignore any values that are not valid because they cannot be used
                .filter_map(|aead_type| {
                    WelcomePointeeEncryptionAeadTypeProto::try_from(aead_type)
                        .map(Into::into)
                        .ok()
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[xmtp_common::test]
    fn test_serialization() {
        let aead_type = AeadType::ChaCha20Poly1305;

        let extension = WelcomePointersExtension::available_types();

        let mls_extension: Extension = extension.try_into().unwrap();

        let Extension::Unknown(id, unknown_extension) = mls_extension else {
            panic!("Expected unknown extension");
        };

        assert_eq!(id, WELCOME_POINTEE_ENCRYPTION_AEAD_TYPES_EXTENSION_ID);

        let deserialized: WelcomePointersExtension = (&unknown_extension).try_into().unwrap();

        assert_eq!(deserialized.supported_aead_types, vec![aead_type]);
    }
}
