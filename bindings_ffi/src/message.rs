use std::collections::HashMap;
use std::sync::Arc;

use xmtp_content_types::{
    attachment::Attachment,
    read_receipt::ReadReceipt,
    remote_attachment::RemoteAttachment,
    reply::Reply,
    transaction_reference::{TransactionMetadata, TransactionReference},
};
use xmtp_db::group_message::{DeliveryStatus, GroupMessageKind};
use xmtp_mls::groups::decoded_message::{
    DecodedMessage, DecodedMessageMetadata, MessageBody, Reply as ProcessedReply, Text,
};
use xmtp_proto::xmtp::mls::message_contents::content_types::{
    MultiRemoteAttachment, ReactionAction, ReactionSchema, ReactionV2, RemoteAttachmentInfo,
};
use xmtp_proto::xmtp::mls::message_contents::{ContentTypeId, EncodedContent, GroupUpdated};

#[derive(uniffi::Record, Clone, Debug)]
pub struct FfiEnrichedReply {
    // The original message that this reply is in reply to.
    // This goes at most one level deep from the original message, and won't happen recursively if there are replies to replies to replies
    pub in_reply_to: Option<Arc<FfiDecodedMessage>>,
    pub content: Option<FfiDecodedMessageBody>,
    pub reference_id: String,
}

// Create a separate enum for the body of the message, which excludes replies and reactions
// This prevents circular references
#[derive(uniffi::Enum, Clone, Debug)]
pub enum FfiDecodedMessageBody {
    Text(FfiTextContent),
    Reaction(FfiReactionPayload),
    Attachment(FfiAttachment),
    RemoteAttachment(FfiRemoteAttachment),
    MultiRemoteAttachment(FfiMultiRemoteAttachment),
    TransactionReference(FfiTransactionReference),
    GroupUpdated(FfiGroupUpdated),
    ReadReceipt(FfiReadReceipt),
    Custom(FfiEncodedContent),
}

// Wrap text content in a struct to be consident with other content types
#[derive(uniffi::Record, Clone, Debug)]
pub struct FfiTextContent {
    pub content: String,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct FfiAttachment {
    pub filename: Option<String>,
    pub mime_type: String,
    pub content: Vec<u8>,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct FfiRemoteAttachment {
    pub url: String,
    pub content_digest: String,
    pub secret: Vec<u8>,
    pub salt: Vec<u8>,
    pub nonce: Vec<u8>,
    pub scheme: String,
    pub content_length: u64,
    pub filename: Option<String>,
}

#[derive(uniffi::Record, Clone, Default, Debug)]
pub struct FfiReactionPayload {
    pub reference: String,
    pub reference_inbox_id: String,
    pub action: FfiReactionAction,
    pub content: String,
    pub schema: FfiReactionSchema,
}

impl From<FfiReactionPayload> for ReactionV2 {
    fn from(reaction: FfiReactionPayload) -> Self {
        ReactionV2 {
            reference: reaction.reference,
            reference_inbox_id: reaction.reference_inbox_id,
            action: reaction.action.into(),
            content: reaction.content,
            schema: reaction.schema.into(),
        }
    }
}

impl From<ReactionV2> for FfiReactionPayload {
    fn from(reaction: ReactionV2) -> Self {
        FfiReactionPayload {
            reference: reaction.reference,
            reference_inbox_id: reaction.reference_inbox_id,
            action: match reaction.action {
                1 => FfiReactionAction::Added,
                2 => FfiReactionAction::Removed,
                _ => FfiReactionAction::Unknown,
            },
            content: reaction.content,
            schema: match reaction.schema {
                1 => FfiReactionSchema::Unicode,
                2 => FfiReactionSchema::Shortcode,
                3 => FfiReactionSchema::Custom,
                _ => FfiReactionSchema::Unknown,
            },
        }
    }
}

#[derive(uniffi::Enum, Clone, Default, PartialEq, Debug)]
pub enum FfiReactionAction {
    Unknown,
    #[default]
    Added,
    Removed,
}

impl From<FfiReactionAction> for i32 {
    fn from(action: FfiReactionAction) -> Self {
        match action {
            FfiReactionAction::Unknown => 0,
            FfiReactionAction::Added => 1,
            FfiReactionAction::Removed => 2,
        }
    }
}

impl From<ReactionAction> for FfiReactionAction {
    fn from(action: ReactionAction) -> Self {
        match action {
            ReactionAction::Unspecified => FfiReactionAction::Unknown,
            ReactionAction::Added => FfiReactionAction::Added,
            ReactionAction::Removed => FfiReactionAction::Removed,
        }
    }
}

impl From<FfiReactionAction> for ReactionAction {
    fn from(action: FfiReactionAction) -> Self {
        match action {
            FfiReactionAction::Unknown => ReactionAction::Unspecified,
            FfiReactionAction::Added => ReactionAction::Added,
            FfiReactionAction::Removed => ReactionAction::Removed,
        }
    }
}

#[derive(uniffi::Enum, Clone, Default, PartialEq, Debug)]
pub enum FfiReactionSchema {
    Unknown,
    #[default]
    Unicode,
    Shortcode,
    Custom,
}

impl From<FfiReactionSchema> for i32 {
    fn from(schema: FfiReactionSchema) -> Self {
        match schema {
            FfiReactionSchema::Unknown => 0,
            FfiReactionSchema::Unicode => 1,
            FfiReactionSchema::Shortcode => 2,
            FfiReactionSchema::Custom => 3,
        }
    }
}

impl From<ReactionSchema> for FfiReactionSchema {
    fn from(schema: ReactionSchema) -> Self {
        match schema {
            ReactionSchema::Unspecified => FfiReactionSchema::Unknown,
            ReactionSchema::Unicode => FfiReactionSchema::Unicode,
            ReactionSchema::Shortcode => FfiReactionSchema::Shortcode,
            ReactionSchema::Custom => FfiReactionSchema::Custom,
        }
    }
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct FfiMultiRemoteAttachment {
    pub attachments: Vec<FfiRemoteAttachmentInfo>,
}

#[derive(uniffi::Record, Clone, Default, Debug)]
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

// Reply FFI structures
#[derive(uniffi::Record, Clone, Default, Debug)]
pub struct FfiReply {
    pub reference: String,
    pub reference_inbox_id: Option<String>,
    pub content: FfiEncodedContent,
}

#[derive(uniffi::Record, Clone, Default, Debug)]
pub struct FfiTransactionMetadata {
    pub transaction_type: String,
    pub currency: String,
    pub amount: f64,
    pub decimals: u32,
    pub from_address: String,
    pub to_address: String,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct FfiTransactionReference {
    pub namespace: Option<String>,
    pub network_id: String,
    pub reference: String,
    pub metadata: Option<FfiTransactionMetadata>,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct FfiGroupUpdated {
    pub initiated_by_inbox_id: String,
    pub added_inboxes: Vec<FfiInbox>,
    pub removed_inboxes: Vec<FfiInbox>,
    pub metadata_field_changes: Vec<FfiMetadataFieldChange>,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct FfiInbox {
    pub inbox_id: String,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct FfiMetadataFieldChange {
    pub field_name: String,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct FfiReadReceipt {}

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

#[derive(uniffi::Enum, Clone, PartialEq, Debug)]
pub enum FfiGroupMessageKind {
    Application,
    MembershipChange,
}

#[derive(uniffi::Enum, Clone, Debug)]
pub enum FfiDeliveryStatus {
    Unpublished,
    Published,
    Failed,
}

#[derive(uniffi::Record, Clone, Debug)]
pub struct FfiDecodedMessageMetadata {
    pub id: Vec<u8>,
    pub sent_at_ns: i64,
    pub kind: FfiGroupMessageKind,
    pub sender_installation_id: Vec<u8>,
    pub sender_inbox_id: String,
    pub content_type: FfiContentTypeId,
    pub conversation_id: Vec<u8>,
}

#[derive(uniffi::Enum, Clone, Debug)]
pub enum FfiDecodedMessageContent {
    Text(FfiTextContent),
    Reply(FfiEnrichedReply),
    Reaction(FfiReactionPayload),
    Attachment(FfiAttachment),
    RemoteAttachment(FfiRemoteAttachment),
    MultiRemoteAttachment(FfiMultiRemoteAttachment),
    TransactionReference(FfiTransactionReference),
    GroupUpdated(FfiGroupUpdated),
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

impl From<FfiReply> for Reply {
    fn from(f: FfiReply) -> Self {
        Reply {
            reference: f.reference,
            reference_inbox_id: f.reference_inbox_id,
            content: f.content.into(),
        }
    }
}

impl From<Reply> for FfiReply {
    fn from(r: Reply) -> Self {
        FfiReply {
            reference: r.reference,
            reference_inbox_id: r.reference_inbox_id,
            content: r.content.into(),
        }
    }
}

impl From<Attachment> for FfiAttachment {
    fn from(attachment: Attachment) -> Self {
        FfiAttachment {
            filename: attachment.filename,
            mime_type: attachment.mime_type,
            content: attachment.content,
        }
    }
}

impl From<FfiAttachment> for Attachment {
    fn from(ffi: FfiAttachment) -> Self {
        Attachment {
            filename: ffi.filename,
            mime_type: ffi.mime_type,
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
            scheme: remote.scheme,
            content_length: remote.content_length as u64,
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
            scheme: ffi.scheme,
            content_length: ffi.content_length as usize,
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

impl From<ReadReceipt> for FfiReadReceipt {
    fn from(_ffi: ReadReceipt) -> Self {
        FfiReadReceipt {}
    }
}

impl From<FfiReadReceipt> for ReadReceipt {
    fn from(_ffi: FfiReadReceipt) -> Self {
        ReadReceipt {}
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
            version_major: type_id.version_major,
            version_minor: type_id.version_minor,
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

impl From<ProcessedReply> for FfiEnrichedReply {
    fn from(reply: ProcessedReply) -> Self {
        FfiEnrichedReply {
            in_reply_to: reply.in_reply_to.map(|m| Arc::new((*m).into())),
            content: content_to_optional_body(*reply.content),
            reference_id: reply.reference_id,
        }
    }
}

impl From<DeliveryStatus> for FfiDeliveryStatus {
    fn from(status: DeliveryStatus) -> Self {
        match status {
            DeliveryStatus::Unpublished => FfiDeliveryStatus::Unpublished,
            DeliveryStatus::Published => FfiDeliveryStatus::Published,
            DeliveryStatus::Failed => FfiDeliveryStatus::Failed,
        }
    }
}

impl From<FfiDeliveryStatus> for DeliveryStatus {
    fn from(status: FfiDeliveryStatus) -> Self {
        match status {
            FfiDeliveryStatus::Unpublished => DeliveryStatus::Unpublished,
            FfiDeliveryStatus::Published => DeliveryStatus::Published,
            FfiDeliveryStatus::Failed => DeliveryStatus::Failed,
        }
    }
}

impl From<DecodedMessageMetadata> for FfiDecodedMessageMetadata {
    fn from(metadata: DecodedMessageMetadata) -> Self {
        FfiDecodedMessageMetadata {
            id: metadata.id,
            sent_at_ns: metadata.sent_at_ns,
            kind: match metadata.kind {
                GroupMessageKind::Application => FfiGroupMessageKind::Application,
                GroupMessageKind::MembershipChange => FfiGroupMessageKind::MembershipChange,
            },
            sender_installation_id: metadata.sender_installation_id,
            conversation_id: metadata.group_id,
            sender_inbox_id: metadata.sender_inbox_id,
            content_type: metadata.content_type.into(),
        }
    }
}

// Main From implementation for MessageBody using the individual implementations

impl From<MessageBody> for FfiDecodedMessageContent {
    fn from(content: MessageBody) -> Self {
        match content {
            MessageBody::Text(text) => FfiDecodedMessageContent::Text(text.into()),
            MessageBody::Reply(reply) => FfiDecodedMessageContent::Reply(reply.into()),
            MessageBody::Reaction(reaction) => FfiDecodedMessageContent::Reaction(reaction.into()),
            MessageBody::Attachment(attachment) => {
                FfiDecodedMessageContent::Attachment(attachment.into())
            }
            MessageBody::RemoteAttachment(remote) => {
                FfiDecodedMessageContent::RemoteAttachment(remote.into())
            }
            MessageBody::MultiRemoteAttachment(multi) => {
                FfiDecodedMessageContent::MultiRemoteAttachment(multi.into())
            }
            MessageBody::TransactionReference(tx_ref) => {
                FfiDecodedMessageContent::TransactionReference(tx_ref.into())
            }
            MessageBody::GroupUpdated(updated) => {
                FfiDecodedMessageContent::GroupUpdated(updated.into())
            }
            MessageBody::ReadReceipt(receipt) => {
                FfiDecodedMessageContent::ReadReceipt(receipt.into())
            }
            MessageBody::Custom(encoded) => FfiDecodedMessageContent::Custom(encoded.into()),
        }
    }
}

// Helper function to convert MessageBody to Option<FfiProcessedMessageBody>
pub fn content_to_optional_body(content: MessageBody) -> Option<FfiDecodedMessageBody> {
    match content {
        MessageBody::Text(text) => Some(FfiDecodedMessageBody::Text(text.into())),
        MessageBody::Reply(_) => None,
        MessageBody::Reaction(reaction) => Some(FfiDecodedMessageBody::Reaction(reaction.into())),
        MessageBody::Attachment(attachment) => {
            Some(FfiDecodedMessageBody::Attachment(attachment.into()))
        }
        MessageBody::RemoteAttachment(remote) => {
            Some(FfiDecodedMessageBody::RemoteAttachment(remote.into()))
        }
        MessageBody::MultiRemoteAttachment(multi) => {
            Some(FfiDecodedMessageBody::MultiRemoteAttachment(multi.into()))
        }
        MessageBody::TransactionReference(tx_ref) => {
            Some(FfiDecodedMessageBody::TransactionReference(tx_ref.into()))
        }
        MessageBody::GroupUpdated(updated) => {
            Some(FfiDecodedMessageBody::GroupUpdated(updated.into()))
        }
        MessageBody::ReadReceipt(receipt) => {
            Some(FfiDecodedMessageBody::ReadReceipt(receipt.into()))
        }
        MessageBody::Custom(encoded) => Some(FfiDecodedMessageBody::Custom(encoded.into())),
    }
}

#[derive(uniffi::Object, Debug)]
pub struct FfiDecodedMessage {
    // Store raw data that we own completely
    id: Vec<u8>,
    sent_at_ns: i64,
    kind: FfiGroupMessageKind,
    sender_installation_id: Vec<u8>,
    sender_inbox_id: String,
    content_type: FfiContentTypeId,
    conversation_id: Vec<u8>,

    // Store the content directly - the Reply variant already uses Arc internally for circular refs
    content: FfiDecodedMessageContent,
    fallback_text: Option<String>,
    reactions: Vec<Arc<FfiDecodedMessage>>,
    delivery_status: FfiDeliveryStatus,
    num_replies: u64,
}

#[uniffi::export]
impl FfiDecodedMessage {
    // Return primitives directly - no cloning needed
    pub fn sent_at_ns(&self) -> i64 {
        self.sent_at_ns
    }

    pub fn num_replies(&self) -> u64 {
        self.num_replies
    }

    pub fn id(&self) -> Vec<u8> {
        self.id.clone()
    }

    pub fn sender_inbox_id(&self) -> String {
        self.sender_inbox_id.clone()
    }

    pub fn sender_installation_id(&self) -> Vec<u8> {
        self.sender_installation_id.clone()
    }

    pub fn conversation_id(&self) -> Vec<u8> {
        self.conversation_id.clone()
    }

    // Enums are cheap to clone
    pub fn delivery_status(&self) -> FfiDeliveryStatus {
        self.delivery_status.clone()
    }

    pub fn kind(&self) -> FfiGroupMessageKind {
        self.kind.clone()
    }

    pub fn content_type_id(&self) -> FfiContentTypeId {
        self.content_type.clone()
    }

    pub fn fallback_text(&self) -> Option<String> {
        self.fallback_text.clone()
    }

    pub fn content(&self) -> FfiDecodedMessageContent {
        self.content.clone()
    }

    pub fn reactions(&self) -> Vec<Arc<FfiDecodedMessage>> {
        self.reactions.clone()
    }

    pub fn has_reactions(&self) -> bool {
        !self.reactions.is_empty()
    }

    pub fn reaction_count(&self) -> u64 {
        self.reactions.len() as u64
    }
}

impl From<DecodedMessage> for FfiDecodedMessage {
    fn from(item: DecodedMessage) -> Self {
        let delivery_status = item.metadata.delivery_status.into();
        // Extract metadata fields directly, consuming the metadata
        let metadata: FfiDecodedMessageMetadata = item.metadata.into();

        FfiDecodedMessage {
            // Take ownership of all the data - no clones!
            id: metadata.id,
            sent_at_ns: metadata.sent_at_ns,
            kind: metadata.kind,
            conversation_id: metadata.conversation_id,
            sender_installation_id: metadata.sender_installation_id,
            sender_inbox_id: metadata.sender_inbox_id,
            delivery_status,
            content_type: metadata.content_type,
            content: item.content.into(),
            fallback_text: item.fallback_text,
            reactions: item
                .reactions
                .into_iter()
                .map(Into::into)
                .map(Arc::new)
                .collect(),
            num_replies: item.num_replies as u64,
        }
    }
}
