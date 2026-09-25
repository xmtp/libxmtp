use std::collections::HashMap;

use xmtp_proto::xmtp::mls::message_contents::{
    ContentTypeId as ProtoContentTypeId, EncodedContent as ProtoEncodedContent,
};

use crate::ContentTypeId;

#[derive(Clone, Debug, uniffi::Record)]
pub struct Attachment {
    pub filename: Option<String>,
    pub mime_type: String,
    pub content: Vec<u8>,
}

impl From<xmtp_content_types::attachment::Attachment> for Attachment {
    fn from(value: xmtp_content_types::attachment::Attachment) -> Self {
        Self {
            filename: value.filename,
            mime_type: value.mime_type,
            content: value.content,
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct RemoteAttachment {
    pub url: String,
    pub content_digest: String,
    pub secret: Vec<u8>,
    pub salt: Vec<u8>,
    pub nonce: Vec<u8>,
    pub scheme: String,
    pub content_length: Option<u32>,
    pub filename: Option<String>,
}

impl From<xmtp_content_types::remote_attachment::RemoteAttachment> for RemoteAttachment {
    fn from(value: xmtp_content_types::remote_attachment::RemoteAttachment) -> Self {
        Self {
            url: value.url,
            content_digest: value.content_digest,
            secret: value.secret,
            salt: value.salt,
            nonce: value.nonce,
            scheme: value.scheme,
            content_length: value.content_length,
            filename: value.filename,
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct MultiRemoteAttachment {
    pub attachments: Vec<RemoteAttachment>,
}

impl From<xmtp_proto::xmtp::mls::message_contents::content_types::MultiRemoteAttachment>
    for MultiRemoteAttachment
{
    fn from(
        value: xmtp_proto::xmtp::mls::message_contents::content_types::MultiRemoteAttachment,
    ) -> Self {
        Self {
            attachments: value.attachments.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct TransactionMetadata {
    pub transaction_type: String,
    pub currency: String,
    pub amount: f64,
    pub decimals: u32,
    pub from_address: String,
    pub to_address: String,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct TransactionReference {
    pub namespace: Option<String>,
    pub network_id: String,
    pub reference: String,
    pub metadata: Option<TransactionMetadata>,
}

impl From<xmtp_content_types::transaction_reference::TransactionReference>
    for TransactionReference
{
    fn from(value: xmtp_content_types::transaction_reference::TransactionReference) -> Self {
        Self {
            namespace: value.namespace,
            network_id: value.network_id,
            reference: value.reference,
            metadata: value.metadata.map(|metadata| TransactionMetadata {
                transaction_type: metadata.transaction_type,
                currency: metadata.currency,
                amount: metadata.amount,
                decimals: metadata.decimals,
                from_address: metadata.from_address,
                to_address: metadata.to_address,
            }),
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct WalletCallMetadata {
    pub description: String,
    pub transaction_type: String,
    pub extra: HashMap<String, String>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct WalletCall {
    pub to: Option<String>,
    pub data: Option<String>,
    pub value: Option<String>,
    pub gas: Option<String>,
    pub metadata: Option<WalletCallMetadata>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct WalletSendCalls {
    pub version: String,
    pub chain_id: String,
    pub from: String,
    pub calls: Vec<WalletCall>,
    pub capabilities: Option<HashMap<String, String>>,
}

impl From<xmtp_content_types::wallet_send_calls::WalletSendCalls> for WalletSendCalls {
    fn from(value: xmtp_content_types::wallet_send_calls::WalletSendCalls) -> Self {
        Self {
            version: value.version,
            chain_id: value.chain_id,
            from: value.from,
            calls: value
                .calls
                .into_iter()
                .map(|call| WalletCall {
                    to: call.to,
                    data: call.data,
                    value: call.value,
                    gas: call.gas,
                    metadata: call.metadata.map(|metadata| WalletCallMetadata {
                        description: metadata.description,
                        transaction_type: metadata.transaction_type,
                        extra: metadata.extra,
                    }),
                })
                .collect(),
            capabilities: value.capabilities,
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct Intent {
    pub id: String,
    pub action_id: String,
    pub metadata_json: Option<String>,
}

impl From<xmtp_content_types::intent::Intent> for Intent {
    fn from(value: xmtp_content_types::intent::Intent) -> Self {
        Self {
            id: value.id,
            action_id: value.action_id,
            metadata_json: value
                .metadata
                .and_then(|value| serde_json::to_string(&value).ok()),
        }
    }
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum ActionStyle {
    Primary,
    Secondary,
    Danger,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct Action {
    pub id: String,
    pub label: String,
    pub image_url: Option<String>,
    pub style: Option<ActionStyle>,
    pub expires_at: Option<crate::Timestamp>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct Actions {
    pub id: String,
    pub description: String,
    pub actions: Vec<Action>,
    pub expires_at: Option<crate::Timestamp>,
}

impl From<xmtp_content_types::actions::Actions> for Actions {
    fn from(value: xmtp_content_types::actions::Actions) -> Self {
        use xmtp_content_types::actions::ActionStyle as CoreStyle;
        Self {
            id: value.id,
            description: value.description,
            expires_at: value
                .expires_at
                .map(|time| crate::Timestamp(time.timestamp_nanos_opt().unwrap_or(i64::MAX))),
            actions: value
                .actions
                .into_iter()
                .map(|action| Action {
                    id: action.id,
                    label: action.label,
                    image_url: action.image_url,
                    style: action.style.map(|style| match style {
                        CoreStyle::Primary => ActionStyle::Primary,
                        CoreStyle::Secondary => ActionStyle::Secondary,
                        CoreStyle::Danger => ActionStyle::Danger,
                    }),
                    expires_at: action.expires_at.map(|time| {
                        crate::Timestamp(time.timestamp_nanos_opt().unwrap_or(i64::MAX))
                    }),
                })
                .collect(),
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct LeaveRequest {
    pub authenticated_note: Option<Vec<u8>>,
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum DeletedBy {
    Sender,
    Admin { inbox_id: crate::InboxID },
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct DeletedMessage {
    pub deleted_by: DeletedBy,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct MetadataFieldChange {
    pub field_name: String,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct GroupUpdated {
    pub initiated_by_inbox_id: crate::InboxID,
    pub added_inboxes: Vec<crate::InboxID>,
    pub removed_inboxes: Vec<crate::InboxID>,
    pub left_inboxes: Vec<crate::InboxID>,
    pub metadata_field_changes: Vec<MetadataFieldChange>,
    pub added_admin_inboxes: Vec<crate::InboxID>,
    pub removed_admin_inboxes: Vec<crate::InboxID>,
    pub added_super_admin_inboxes: Vec<crate::InboxID>,
    pub removed_super_admin_inboxes: Vec<crate::InboxID>,
}

impl TryFrom<xmtp_proto::xmtp::mls::message_contents::GroupUpdated> for GroupUpdated {
    type Error = crate::XmtpError;
    fn try_from(
        value: xmtp_proto::xmtp::mls::message_contents::GroupUpdated,
    ) -> Result<Self, Self::Error> {
        fn ids(
            values: Vec<xmtp_proto::xmtp::mls::message_contents::group_updated::Inbox>,
        ) -> Result<Vec<crate::InboxID>, crate::XmtpError> {
            values
                .into_iter()
                .map(|value| crate::InboxID::try_from(value.inbox_id))
                .collect()
        }
        Ok(Self {
            initiated_by_inbox_id: crate::InboxID::try_from(value.initiated_by_inbox_id)?,
            added_inboxes: ids(value.added_inboxes)?,
            removed_inboxes: ids(value.removed_inboxes)?,
            left_inboxes: ids(value.left_inboxes)?,
            metadata_field_changes: value
                .metadata_field_changes
                .into_iter()
                .map(|field| MetadataFieldChange {
                    field_name: field.field_name,
                    old_value: field.old_value,
                    new_value: field.new_value,
                })
                .collect(),
            added_admin_inboxes: ids(value.added_admin_inboxes)?,
            removed_admin_inboxes: ids(value.removed_admin_inboxes)?,
            added_super_admin_inboxes: ids(value.added_super_admin_inboxes)?,
            removed_super_admin_inboxes: ids(value.removed_super_admin_inboxes)?,
        })
    }
}

/// Content at this boundary is uncompressed. Send options control wire compression.
#[derive(Clone, Debug, uniffi::Record)]
pub struct EncodedContent {
    pub r#type: ContentTypeId,
    #[uniffi(default)]
    pub parameters: HashMap<String, String>,
    #[uniffi(default = None)]
    pub fallback: Option<String>,
    pub content: Vec<u8>,
}

#[xmtp_macro::sdk_export]
pub fn encode_text(text: String) -> Result<EncodedContent, crate::XmtpError> {
    use xmtp_content_types::{ContentCodec, text::TextCodec};
    TextCodec::encode(text)
        .map(Into::into)
        .map_err(crate::XmtpError::unknown)
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
    #[uniffi(default = true)]
    pub should_push: bool,
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
        reference: crate::MessageID,
        reference_inbox_id: crate::InboxID,
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
