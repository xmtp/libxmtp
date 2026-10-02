/// Serialize an uncompressed content envelope with the shared wire format.
#[xmtp_macro::sdk_export(pure)]
pub fn encode_encoded_content(content: EncodedContent) -> Vec<u8> {
    use prost::Message;
    ProtoEncodedContent::from(content).encode_to_vec()
}

/// Read a content envelope and apply the shared decompression limits.
#[xmtp_macro::sdk_export(pure)]
pub fn decode_encoded_content(bytes: Vec<u8>) -> Result<EncodedContent, crate::XmtpError> {
    use prost::Message;
    ProtoEncodedContent::decode(bytes.as_slice())
        .map_err(|error| crate::XmtpError::malformed_envelope(error.to_string()))?
        .try_into()
}

impl From<EncodedContent> for ProtoEncodedContent {
    fn from(value: EncodedContent) -> Self {
        Self {
            r#type: Some(ProtoContentTypeId {
                authority_id: value.r#type.authority_id,
                type_id: value.r#type.type_id,
                version_major: value.r#type.version_major,
                version_minor: value.r#type.version_minor,
            }),
            parameters: value.parameters,
            fallback: value.fallback,
            compression: None,
            content: value.content,
        }
    }
}

impl TryFrom<ProtoEncodedContent> for EncodedContent {
    type Error = crate::XmtpError;

    fn try_from(value: ProtoEncodedContent) -> Result<Self, Self::Error> {
        let kind = value
            .r#type
            .as_ref()
            .filter(|kind| !kind.authority_id.is_empty() && !kind.type_id.is_empty())
            .ok_or_else(|| {
                crate::XmtpError::malformed_envelope(
                    "content type identifier is absent or incomplete",
                )
            })?;
        let kind = ContentTypeId {
            authority_id: kind.authority_id.clone(),
            type_id: kind.type_id.clone(),
            version_major: kind.version_major,
            version_minor: kind.version_minor,
        };
        // Codec input must be uncompressed. Never replace failed decompression with empty content.
        // implements: CTYPE-024, CTYPE-025
        let value = xmtp_content_types::compression::decompress(value)
            .map_err(|error| crate::XmtpError::codec_decode_failed(error.to_string()))?;
        Ok(Self {
            r#type: kind,
            parameters: value.parameters,
            fallback: value.fallback,
            content: value.content,
        })
    }
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum Compression {
    Deflate,
    Gzip,
}

impl From<Compression> for xmtp_proto::xmtp::mls::message_contents::Compression {
    fn from(value: Compression) -> Self {
        match value {
            Compression::Deflate => Self::Deflate,
            Compression::Gzip => Self::Gzip,
        }
    }
}

#[derive(Clone, Debug, Default, uniffi::Record)]
pub struct SendOptions {
    #[uniffi(default = None)]
    pub should_push: Option<bool>,
    #[uniffi(default = false)]
    pub optimistic: bool,
    #[uniffi(default = None)]
    pub idempotency_key: Option<String>,
    #[uniffi(default = None)]
    pub compression: Option<Compression>,
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum ReactionAction {
    Unknown,
    Added,
    Removed,
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum ReactionSchema {
    Unknown,
    Unicode,
    Shortcode,
    Custom,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct Reaction {
    pub content: String,
    pub action: ReactionAction,
    pub schema: ReactionSchema,
}

impl Reaction {
    pub(crate) fn from_proto(
        value: xmtp_proto::xmtp::mls::message_contents::content_types::ReactionV2,
    ) -> Self {
        use xmtp_proto::xmtp::mls::message_contents::content_types::{
            ReactionAction as ProtoAction, ReactionSchema as ProtoSchema,
        };
        Self {
            content: value.content,
            action: match ProtoAction::try_from(value.action) {
                Ok(ProtoAction::Added) => ReactionAction::Added,
                Ok(ProtoAction::Removed) => ReactionAction::Removed,
                _ => ReactionAction::Unknown,
            },
            schema: match ProtoSchema::try_from(value.schema) {
                Ok(ProtoSchema::Unicode) => ReactionSchema::Unicode,
                Ok(ProtoSchema::Shortcode) => ReactionSchema::Shortcode,
                Ok(ProtoSchema::Custom) => ReactionSchema::Custom,
                _ => ReactionSchema::Unknown,
            },
        }
    }

    pub(crate) fn into_proto(
        self,
        reference: String,
        reference_inbox_id: String,
    ) -> xmtp_proto::xmtp::mls::message_contents::content_types::ReactionV2 {
        use xmtp_proto::xmtp::mls::message_contents::content_types::{
            ReactionAction as ProtoAction, ReactionSchema as ProtoSchema, ReactionV2,
        };
        ReactionV2 {
            reference,
            reference_inbox_id,
            action: match self.action {
                ReactionAction::Unknown => 0,
                ReactionAction::Added => ProtoAction::Added as i32,
                ReactionAction::Removed => ProtoAction::Removed as i32,
            },
            content: self.content,
            schema: match self.schema {
                ReactionSchema::Unknown => 0,
                ReactionSchema::Unicode => ProtoSchema::Unicode as i32,
                ReactionSchema::Shortcode => ProtoSchema::Shortcode as i32,
                ReactionSchema::Custom => ProtoSchema::Custom as i32,
            },
        }
    }
}
