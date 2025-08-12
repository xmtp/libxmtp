use std::collections::HashMap;
use std::sync::Arc;

use crate::mls::FfiReaction;

use xmtp_content_types::{
    attachment::Attachment,
    read_receipt::ReadReceipt,
    remote_attachment::RemoteAttachment,
    transaction_reference::{TransactionMetadata, TransactionReference},
};
use xmtp_mls::groups::message_list_item::{
    MessageListItem, MessageListItemContent, Reaction, Reply, Text,
};
use xmtp_proto::xmtp::mls::message_contents::content_types::{
    MultiRemoteAttachment, RemoteAttachmentInfo,
};
use xmtp_proto::xmtp::mls::message_contents::{
    ContentTypeId, EncodedContent, GroupMembershipChanges, GroupUpdated, MembershipChange,
};

#[derive(uniffi::Record, Clone)]
pub struct FfiReplyContent {
    // The original message that this reply is in reply to.
    // This goes at most one level deep from the original message, and won't happen recursively if there are replies to replies to replies
    pub in_reply_to: Option<Arc<FfiProcessedMessage>>,
    pub content: Option<FfiProcessedMessageBody>,
}

// Create a separate enum for the body of the message, which excludes replies and reactions
// This prevents circular references
#[derive(uniffi::Enum, Clone)]
pub enum FfiProcessedMessageBody {
    Text(FfiTextContent),
    Attachment(FfiAttachment),
    RemoteAttachment(FfiRemoteAttachment),
    MultiRemoteAttachment(FfiMultiRemoteAttachment),
    TransactionReference(FfiTransactionReference),
    GroupUpdated(FfiGroupUpdated),
    GroupMembershipChanges(FfiGroupMembershipChanges),
    ReadReceipt(FfiReadReceipt),
    Custom(FfiEncodedContent),
}

// Wrap text content in a struct to be consident with other content types
#[derive(uniffi::Record, Clone)]
pub struct FfiTextContent {
    pub content: String,
}

// FfiReaction is defined in mls.rs with proper enum types for action and schema

#[derive(uniffi::Record, Clone)]
pub struct FfiAttachment {
    pub filename: Option<String>,
    pub mime_type: String,
    pub size: u64,
    pub content: String,
}

#[derive(uniffi::Record, Clone)]
pub struct FfiRemoteAttachment {
    pub url: String,
    pub content_digest: String,
    pub secret: Vec<u8>,
    pub salt: Vec<u8>,
    pub nonce: Vec<u8>,
    pub size: u64,
    pub mime_type: String,
    pub filename: Option<String>,
}

#[derive(uniffi::Record, Clone)]
pub struct FfiMultiRemoteAttachment {
    pub attachments: Vec<FfiRemoteAttachmentInfo>,
}

#[derive(uniffi::Record, Clone, Default)]
pub struct FfiRemoteAttachmentInfo {
    pub url: String,
    pub content_digest: String,
    pub secret: Vec<u8>,
    pub salt: Vec<u8>,
    pub nonce: Vec<u8>,
    pub scheme: String,
    pub content_length: Option<u32>,
    pub filename: Option<String>,
}

#[derive(uniffi::Record, Clone, Default)]
pub struct FfiTransactionMetadata {
    pub transaction_type: String,
    pub currency: String,
    pub amount: f64,
    pub decimals: u32,
    pub from_address: String,
    pub to_address: String,
}

#[derive(uniffi::Record, Clone)]
pub struct FfiTransactionReference {
    pub namespace: Option<String>,
    pub network_id: String,
    pub reference: String,
    pub metadata: Option<FfiTransactionMetadata>,
}

#[derive(uniffi::Record, Clone)]
pub struct FfiGroupUpdated {
    pub initiated_by_inbox_id: String,
    pub added_inboxes: Vec<FfiInbox>,
    pub removed_inboxes: Vec<FfiInbox>,
    pub metadata_field_changes: Vec<FfiMetadataFieldChange>,
}

#[derive(uniffi::Record, Clone)]
pub struct FfiInbox {
    pub inbox_id: String,
}

#[derive(uniffi::Record, Clone)]
pub struct FfiMetadataFieldChange {
    pub field_name: String,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
}

#[derive(uniffi::Record, Clone)]
pub struct FfiGroupMembershipChanges {
    pub members_added: Vec<FfiMembershipChange>,
    pub members_removed: Vec<FfiMembershipChange>,
    pub installations_added: Vec<FfiMembershipChange>,
    pub installations_removed: Vec<FfiMembershipChange>,
}

#[derive(uniffi::Record, Clone)]
pub struct FfiMembershipChange {
    pub installation_ids: Vec<Vec<u8>>,
    pub account_address: String,
    pub initiated_by_account_address: String,
}

#[derive(uniffi::Record, Clone)]
pub struct FfiReadReceipt {
    pub reference: String,
    pub reference_inbox_id: Option<String>,
    pub read_at_ns: i64,
}

#[derive(uniffi::Record, Clone, Default, Debug, PartialEq)]
pub struct FfiEncodedContent {
    pub type_id: Option<FfiContentTypeId>,
    pub parameters: HashMap<String, String>,
    pub fallback: Option<String>,
    pub compression: Option<i32>,
    pub content: Vec<u8>,
}

#[derive(uniffi::Record, Clone, Default, Debug, PartialEq)]
pub struct FfiContentTypeId {
    pub authority_id: String,
    pub type_id: String,
    pub version_major: u32,
    pub version_minor: u32,
}

#[derive(uniffi::Enum, Clone)]
pub enum FfiProcessedMessageContent {
    Text(FfiTextContent),
    Reply(FfiReplyContent),
    Reaction(FfiReaction),
    Attachment(FfiAttachment),
    RemoteAttachment(FfiRemoteAttachment),
    MultiRemoteAttachment(FfiMultiRemoteAttachment),
    TransactionReference(FfiTransactionReference),
    GroupUpdated(FfiGroupUpdated),
    GroupMembershipChanges(FfiGroupMembershipChanges),
    ReadReceipt(FfiReadReceipt),
    Custom(FfiEncodedContent),
}

// Individual From implementations for each content type

impl From<Text> for FfiTextContent {
    fn from(text: Text) -> Self {
        FfiTextContent {
            content: text.content,
        }
    }
}

impl From<Reaction> for FfiReaction {
    fn from(reaction: Reaction) -> Self {
        // Convert via ReactionV2 which has the From implementation in mls.rs
        reaction.reaction.into()
    }
}

impl From<Attachment> for FfiAttachment {
    fn from(attachment: Attachment) -> Self {
        FfiAttachment {
            filename: attachment.filename,
            mime_type: attachment.mime_type,
            size: attachment.size,
            content: attachment.content,
        }
    }
}

impl From<FfiAttachment> for Attachment {
    fn from(ffi: FfiAttachment) -> Self {
        Attachment {
            filename: ffi.filename,
            mime_type: ffi.mime_type,
            size: ffi.size,
            content: ffi.content,
        }
    }
}

impl From<RemoteAttachment> for FfiRemoteAttachment {
    fn from(remote: RemoteAttachment) -> Self {
        FfiRemoteAttachment {
            url: remote.url,
            content_digest: remote.content_digest,
            secret: remote.secret,
            salt: remote.salt,
            nonce: remote.nonce,
            size: remote.size,
            mime_type: remote.mime_type,
            filename: remote.filename,
        }
    }
}

impl From<FfiRemoteAttachment> for RemoteAttachment {
    fn from(ffi: FfiRemoteAttachment) -> Self {
        RemoteAttachment {
            url: ffi.url,
            content_digest: ffi.content_digest,
            secret: ffi.secret,
            salt: ffi.salt,
            nonce: ffi.nonce,
            size: ffi.size,
            mime_type: ffi.mime_type,
            filename: ffi.filename,
        }
    }
}

impl From<RemoteAttachmentInfo> for FfiRemoteAttachmentInfo {
    fn from(info: RemoteAttachmentInfo) -> Self {
        FfiRemoteAttachmentInfo {
            url: info.url,
            content_digest: info.content_digest,
            secret: info.secret,
            salt: info.salt,
            nonce: info.nonce,
            scheme: info.scheme,
            content_length: info.content_length,
            filename: info.filename,
        }
    }
}

impl From<FfiRemoteAttachmentInfo> for RemoteAttachmentInfo {
    fn from(ffi: FfiRemoteAttachmentInfo) -> Self {
        RemoteAttachmentInfo {
            url: ffi.url,
            content_digest: ffi.content_digest,
            secret: ffi.secret,
            salt: ffi.salt,
            nonce: ffi.nonce,
            scheme: ffi.scheme,
            content_length: ffi.content_length,
            filename: ffi.filename,
        }
    }
}

impl From<MultiRemoteAttachment> for FfiMultiRemoteAttachment {
    fn from(multi: MultiRemoteAttachment) -> Self {
        FfiMultiRemoteAttachment {
            attachments: multi.attachments.into_iter().map(|a| a.into()).collect(),
        }
    }
}

impl From<FfiMultiRemoteAttachment> for MultiRemoteAttachment {
    fn from(ffi: FfiMultiRemoteAttachment) -> Self {
        MultiRemoteAttachment {
            attachments: ffi.attachments.into_iter().map(|a| a.into()).collect(),
        }
    }
}

impl From<TransactionMetadata> for FfiTransactionMetadata {
    fn from(metadata: TransactionMetadata) -> Self {
        FfiTransactionMetadata {
            transaction_type: metadata.transaction_type,
            currency: metadata.currency,
            amount: metadata.amount,
            decimals: metadata.decimals,
            from_address: metadata.from_address,
            to_address: metadata.to_address,
        }
    }
}

impl From<FfiTransactionMetadata> for TransactionMetadata {
    fn from(ffi: FfiTransactionMetadata) -> Self {
        TransactionMetadata {
            transaction_type: ffi.transaction_type,
            currency: ffi.currency,
            amount: ffi.amount,
            decimals: ffi.decimals,
            from_address: ffi.from_address,
            to_address: ffi.to_address,
        }
    }
}

impl From<TransactionReference> for FfiTransactionReference {
    fn from(tx_ref: TransactionReference) -> Self {
        FfiTransactionReference {
            namespace: tx_ref.namespace,
            network_id: tx_ref.network_id,
            reference: tx_ref.reference,
            metadata: tx_ref.metadata.map(|m| m.into()),
        }
    }
}

impl From<FfiTransactionReference> for TransactionReference {
    fn from(ffi: FfiTransactionReference) -> Self {
        TransactionReference {
            namespace: ffi.namespace,
            network_id: ffi.network_id,
            reference: ffi.reference,
            metadata: ffi.metadata.map(|m| m.into()),
        }
    }
}

impl From<GroupUpdated> for FfiGroupUpdated {
    fn from(updated: GroupUpdated) -> Self {
        FfiGroupUpdated {
            initiated_by_inbox_id: updated.initiated_by_inbox_id,
            added_inboxes: updated
                .added_inboxes
                .into_iter()
                .map(|inbox| FfiInbox {
                    inbox_id: inbox.inbox_id,
                })
                .collect(),
            removed_inboxes: updated
                .removed_inboxes
                .into_iter()
                .map(|inbox| FfiInbox {
                    inbox_id: inbox.inbox_id,
                })
                .collect(),
            metadata_field_changes: updated
                .metadata_field_changes
                .into_iter()
                .map(|change| FfiMetadataFieldChange {
                    field_name: change.field_name,
                    old_value: change.old_value,
                    new_value: change.new_value,
                })
                .collect(),
        }
    }
}

impl From<GroupMembershipChanges> for FfiGroupMembershipChanges {
    fn from(changes: GroupMembershipChanges) -> Self {
        FfiGroupMembershipChanges {
            members_added: changes
                .members_added
                .into_iter()
                .map(|m| m.into())
                .collect(),
            members_removed: changes
                .members_removed
                .into_iter()
                .map(|m| m.into())
                .collect(),
            installations_added: changes
                .installations_added
                .into_iter()
                .map(|m| m.into())
                .collect(),
            installations_removed: changes
                .installations_removed
                .into_iter()
                .map(|m| m.into())
                .collect(),
        }
    }
}

impl From<MembershipChange> for FfiMembershipChange {
    fn from(change: MembershipChange) -> Self {
        FfiMembershipChange {
            installation_ids: change.installation_ids,
            account_address: change.account_address,
            initiated_by_account_address: change.initiated_by_account_address,
        }
    }
}

impl From<ReadReceipt> for FfiReadReceipt {
    fn from(receipt: ReadReceipt) -> Self {
        FfiReadReceipt {
            reference: receipt.reference,
            reference_inbox_id: receipt.reference_inbox_id,
            read_at_ns: receipt.read_at_ns,
        }
    }
}

impl From<FfiReadReceipt> for ReadReceipt {
    fn from(ffi: FfiReadReceipt) -> Self {
        ReadReceipt {
            reference: ffi.reference,
            reference_inbox_id: ffi.reference_inbox_id,
            read_at_ns: ffi.read_at_ns,
        }
    }
}

impl From<EncodedContent> for FfiEncodedContent {
    fn from(encoded: EncodedContent) -> Self {
        FfiEncodedContent {
            type_id: encoded.r#type.map(|t| t.into()),
            parameters: encoded.parameters,
            fallback: encoded.fallback,
            compression: encoded.compression,
            content: encoded.content,
        }
    }
}

impl From<FfiEncodedContent> for EncodedContent {
    fn from(ffi: FfiEncodedContent) -> Self {
        EncodedContent {
            r#type: ffi.type_id.map(|t| t.into()),
            parameters: ffi.parameters,
            fallback: ffi.fallback,
            compression: ffi.compression,
            content: ffi.content,
        }
    }
}

impl From<ContentTypeId> for FfiContentTypeId {
    fn from(type_id: ContentTypeId) -> Self {
        FfiContentTypeId {
            authority_id: type_id.authority_id,
            type_id: type_id.type_id,
            version_major: type_id.version_major as u32,
            version_minor: type_id.version_minor as u32,
        }
    }
}

impl From<FfiContentTypeId> for ContentTypeId {
    fn from(ffi: FfiContentTypeId) -> Self {
        ContentTypeId {
            authority_id: ffi.authority_id,
            type_id: ffi.type_id,
            version_major: ffi.version_major,
            version_minor: ffi.version_minor,
        }
    }
}

impl From<Reply> for FfiReplyContent {
    fn from(reply: Reply) -> Self {
        FfiReplyContent {
            in_reply_to: reply.in_reply_to.map(|m| Arc::new((*m).into())),
            content: content_to_optional_body(*reply.content),
        }
    }
}

// Main From implementation for MessageListItemContent using the individual implementations

impl From<MessageListItemContent> for FfiProcessedMessageContent {
    fn from(content: MessageListItemContent) -> Self {
        match content {
            MessageListItemContent::Text(text) => FfiProcessedMessageContent::Text(text.into()),
            MessageListItemContent::Reply(reply) => FfiProcessedMessageContent::Reply(reply.into()),
            MessageListItemContent::Reaction(reaction) => {
                FfiProcessedMessageContent::Reaction(reaction.into())
            }
            MessageListItemContent::Attachment(attachment) => {
                FfiProcessedMessageContent::Attachment(attachment.into())
            }
            MessageListItemContent::RemoteAttachment(remote) => {
                FfiProcessedMessageContent::RemoteAttachment(remote.into())
            }
            MessageListItemContent::MultiRemoteAttachment(multi) => {
                FfiProcessedMessageContent::MultiRemoteAttachment(multi.into())
            }
            MessageListItemContent::TransactionReference(tx_ref) => {
                FfiProcessedMessageContent::TransactionReference(tx_ref.into())
            }
            MessageListItemContent::GroupUpdated(updated) => {
                FfiProcessedMessageContent::GroupUpdated(updated.into())
            }
            MessageListItemContent::GroupMembershipChanges(changes) => {
                FfiProcessedMessageContent::GroupMembershipChanges(changes.into())
            }
            MessageListItemContent::ReadReceipt(receipt) => {
                FfiProcessedMessageContent::ReadReceipt(receipt.into())
            }
            MessageListItemContent::Custom(encoded) => {
                FfiProcessedMessageContent::Custom(encoded.into())
            }
        }
    }
}

// Helper function to convert MessageListItemContent to Option<FfiProcessedMessageBody>
pub fn content_to_optional_body(
    content: MessageListItemContent,
) -> Option<FfiProcessedMessageBody> {
    match content {
        MessageListItemContent::Text(text) => Some(FfiProcessedMessageBody::Text(text.into())),
        MessageListItemContent::Reply(_) => None,
        MessageListItemContent::Reaction(_) => None,
        MessageListItemContent::Attachment(attachment) => {
            Some(FfiProcessedMessageBody::Attachment(attachment.into()))
        }
        MessageListItemContent::RemoteAttachment(remote) => {
            Some(FfiProcessedMessageBody::RemoteAttachment(remote.into()))
        }
        MessageListItemContent::MultiRemoteAttachment(multi) => {
            Some(FfiProcessedMessageBody::MultiRemoteAttachment(multi.into()))
        }
        MessageListItemContent::TransactionReference(tx_ref) => {
            Some(FfiProcessedMessageBody::TransactionReference(tx_ref.into()))
        }
        MessageListItemContent::GroupUpdated(updated) => {
            Some(FfiProcessedMessageBody::GroupUpdated(updated.into()))
        }
        MessageListItemContent::GroupMembershipChanges(changes) => Some(
            FfiProcessedMessageBody::GroupMembershipChanges(changes.into()),
        ),
        MessageListItemContent::ReadReceipt(receipt) => {
            Some(FfiProcessedMessageBody::ReadReceipt(receipt.into()))
        }
        MessageListItemContent::Custom(encoded) => {
            Some(FfiProcessedMessageBody::Custom(encoded.into()))
        }
    }
}

#[derive(uniffi::Object, Clone)]
pub struct FfiProcessedMessage {
    record: MessageListItem,
}

#[uniffi::export]
impl FfiProcessedMessage {
    pub fn content(&self) -> FfiProcessedMessageContent {
        self.record.content.clone().into()
    }
}

impl From<MessageListItem> for FfiProcessedMessage {
    fn from(item: MessageListItem) -> Self {
        FfiProcessedMessage { record: item }
    }
}
