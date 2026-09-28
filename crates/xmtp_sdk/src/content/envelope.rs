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

impl From<ProtoEncodedContent> for EncodedContent {
    fn from(value: ProtoEncodedContent) -> Self {
        let value = if value.compression.is_some() {
            xmtp_content_types::compression::decompress(value.clone()).unwrap_or_else(|_| {
                ProtoEncodedContent {
                    content: Vec::new(),
                    compression: None,
                    ..value
                }
            })
        } else {
            value
        };
        let kind = value.r#type.unwrap_or_default();
        Self {
            r#type: ContentTypeId {
                authority_id: kind.authority_id,
                type_id: kind.type_id,
                version_major: kind.version_major,
                version_minor: kind.version_minor,
            },
            parameters: value.parameters,
            fallback: value.fallback,
            content: value.content,
        }
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
        reference: crate::MessageId,
        reference_inbox_id: crate::InboxId,
    ) -> xmtp_proto::xmtp::mls::message_contents::content_types::ReactionV2 {
        use xmtp_proto::xmtp::mls::message_contents::content_types::{
            ReactionAction as ProtoAction, ReactionSchema as ProtoSchema, ReactionV2,
        };
        ReactionV2 {
            reference: reference.0,
            reference_inbox_id: reference_inbox_id.0,
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

