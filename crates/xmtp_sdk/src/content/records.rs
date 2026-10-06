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

#[xmtp_macro::sdk_export]
#[derive(Clone, uniffi::Record)]
pub struct RemoteAttachment {
    #[sdk(redact)]
    pub url: String,
    #[sdk(shown)]
    pub content_digest: String,
    #[sdk(redact)]
    pub secret: Vec<u8>,
    #[sdk(shown)]
    pub salt: Vec<u8>,
    #[sdk(shown)]
    pub nonce: Vec<u8>,
    #[sdk(shown)]
    pub scheme: String,
    #[sdk(shown)]
    pub content_length: Option<u32>,
    #[sdk(shown)]
    pub filename: Option<String>,
}

impl std::fmt::Debug for RemoteAttachment {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RemoteAttachment")
            .field("url", &"<redacted>")
            .field("content_digest", &self.content_digest)
            .field("secret", &"<redacted>")
            .field("salt", &self.salt)
            .field("nonce", &self.nonce)
            .field("scheme", &self.scheme)
            .field("content_length", &self.content_length)
            .field("filename", &self.filename)
            .finish()
    }
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

impl From<RemoteAttachment>
    for xmtp_proto::xmtp::mls::message_contents::content_types::RemoteAttachmentInfo
{
    fn from(value: RemoteAttachment) -> Self {
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

impl From<WalletSendCalls> for xmtp_content_types::wallet_send_calls::WalletSendCalls {
    fn from(value: WalletSendCalls) -> Self {
        use xmtp_content_types::wallet_send_calls::{
            WalletCall as CoreCall, WalletCallMetadata as CoreMetadata,
        };
        Self {
            version: value.version,
            chain_id: value.chain_id,
            from: value.from,
            calls: value
                .calls
                .into_iter()
                .map(|call| CoreCall {
                    to: call.to,
                    data: call.data,
                    value: call.value,
                    gas: call.gas,
                    metadata: call.metadata.map(|metadata| CoreMetadata {
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

impl TryFrom<Intent> for xmtp_content_types::intent::Intent {
    type Error = crate::XmtpError;

    fn try_from(value: Intent) -> Result<Self, Self::Error> {
        let metadata = value
            .metadata_json
            .map(|json| {
                serde_json::from_str(&json)
                    .map_err(|error| crate::XmtpError::invalid(error.to_string()))
            })
            .transpose()?;
        Ok(Self {
            id: value.id,
            action_id: value.action_id,
            metadata,
        })
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

impl TryFrom<xmtp_content_types::actions::Actions> for Actions {
    type Error = crate::XmtpError;

    fn try_from(value: xmtp_content_types::actions::Actions) -> Result<Self, Self::Error> {
        use xmtp_content_types::actions::ActionStyle as CoreStyle;
        Ok(Self {
            id: value.id,
            description: value.description,
            expires_at: value
                .expires_at
                .map(|time| {
                    time.timestamp_nanos_opt()
                        .map(crate::Timestamp)
                        .ok_or_else(|| crate::XmtpError::invalid("Actions expiry is out of range"))
                })
                .transpose()?,
            actions: value
                .actions
                .into_iter()
                .map(|action| {
                    Ok(Action {
                        id: action.id,
                        label: action.label,
                        image_url: action.image_url,
                        style: action.style.map(|style| match style {
                            CoreStyle::Primary => ActionStyle::Primary,
                            CoreStyle::Secondary => ActionStyle::Secondary,
                            CoreStyle::Danger => ActionStyle::Danger,
                        }),
                        expires_at: action
                            .expires_at
                            .map(|time| {
                                time.timestamp_nanos_opt()
                                    .map(crate::Timestamp)
                                    .ok_or_else(|| {
                                        crate::XmtpError::invalid("Action expiry is out of range")
                                    })
                            })
                            .transpose()?,
                    })
                })
                .collect::<Result<Vec<_>, crate::XmtpError>>()?,
        })
    }
}

impl From<Actions> for xmtp_content_types::actions::Actions {
    fn from(value: Actions) -> Self {
        use xmtp_content_types::actions::{Action as CoreAction, ActionStyle as CoreStyle};
        Self {
            id: value.id,
            description: value.description,
            expires_at: value
                .expires_at
                .map(|time| chrono::DateTime::from_timestamp_nanos(time.0)),
            actions: value
                .actions
                .into_iter()
                .map(|action| CoreAction {
                    id: action.id,
                    label: action.label,
                    image_url: action.image_url,
                    style: action.style.map(|style| match style {
                        ActionStyle::Primary => CoreStyle::Primary,
                        ActionStyle::Secondary => CoreStyle::Secondary,
                        ActionStyle::Danger => CoreStyle::Danger,
                    }),
                    expires_at: action
                        .expires_at
                        .map(|time| chrono::DateTime::from_timestamp_nanos(time.0)),
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
    Admin { inbox_id: crate::InboxId },
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
    pub initiated_by_inbox_id: crate::InboxId,
    pub added_inboxes: Vec<crate::InboxId>,
    pub removed_inboxes: Vec<crate::InboxId>,
    pub left_inboxes: Vec<crate::InboxId>,
    pub metadata_field_changes: Vec<MetadataFieldChange>,
    pub added_admin_inboxes: Vec<crate::InboxId>,
    pub removed_admin_inboxes: Vec<crate::InboxId>,
    pub added_super_admin_inboxes: Vec<crate::InboxId>,
    pub removed_super_admin_inboxes: Vec<crate::InboxId>,
}

impl TryFrom<xmtp_proto::xmtp::mls::message_contents::GroupUpdated> for GroupUpdated {
    type Error = crate::XmtpError;
    fn try_from(
        value: xmtp_proto::xmtp::mls::message_contents::GroupUpdated,
    ) -> Result<Self, Self::Error> {
        fn ids(
            values: Vec<xmtp_proto::xmtp::mls::message_contents::group_updated::Inbox>,
        ) -> Result<Vec<crate::InboxId>, crate::XmtpError> {
            values
                .into_iter()
                .map(|value| crate::InboxId::try_from(value.inbox_id))
                .collect()
        }
        Ok(Self {
            initiated_by_inbox_id: crate::InboxId::try_from(value.initiated_by_inbox_id)?,
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

impl TryFrom<GroupUpdated> for xmtp_proto::xmtp::mls::message_contents::GroupUpdated {
    type Error = crate::XmtpError;

    fn try_from(value: GroupUpdated) -> Result<Self, Self::Error> {
        use xmtp_proto::xmtp::mls::message_contents::group_updated::{
            Inbox, MetadataFieldChange as ProtoChange,
        };
        fn inboxes(values: Vec<crate::InboxId>) -> Result<Vec<Inbox>, crate::XmtpError> {
            values
                .into_iter()
                .map(|value| {
                    Ok(Inbox {
                        inbox_id: value.into_checked()?,
                    })
                })
                .collect()
        }
        Ok(Self {
            initiated_by_inbox_id: value.initiated_by_inbox_id.into_checked()?,
            added_inboxes: inboxes(value.added_inboxes)?,
            removed_inboxes: inboxes(value.removed_inboxes)?,
            left_inboxes: inboxes(value.left_inboxes)?,
            added_admin_inboxes: inboxes(value.added_admin_inboxes)?,
            removed_admin_inboxes: inboxes(value.removed_admin_inboxes)?,
            added_super_admin_inboxes: inboxes(value.added_super_admin_inboxes)?,
            removed_super_admin_inboxes: inboxes(value.removed_super_admin_inboxes)?,
            metadata_field_changes: value
                .metadata_field_changes
                .into_iter()
                .map(|field| ProtoChange {
                    field_name: field.field_name,
                    old_value: field.old_value,
                    new_value: field.new_value,
                })
                .collect(),
        })
    }
}

#[derive(Clone, uniffi::Record)]
pub struct EncryptionKeys {
    pub secret: Vec<u8>,
    pub salt: Vec<u8>,
    pub nonce: Vec<u8>,
    pub digest: String,
    pub length: u64,
}

impl std::fmt::Debug for EncryptionKeys {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EncryptionKeys")
            .field("secret", &"<redacted>")
            .field("salt", &self.salt)
            .field("nonce", &self.nonce)
            .field("digest", &self.digest)
            .field("length", &self.length)
            .finish()
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct EncryptedEncodedContent {
    pub ciphertext: Vec<u8>,
    pub keys: EncryptionKeys,
}
