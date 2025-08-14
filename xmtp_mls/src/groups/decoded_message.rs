use xmtp_content_types::{
    attachment::Attachment, read_receipt::ReadReceipt, remote_attachment::RemoteAttachment,
    transaction_reference::TransactionReference,
};
use xmtp_db::group_message::{DeliveryStatus, GroupMessageKind};
use xmtp_proto::xmtp::mls::message_contents::{
    ContentTypeId, EncodedContent, GroupMembershipChanges, GroupUpdated,
    content_types::{MultiRemoteAttachment, ReactionV2},
};

#[derive(Debug, Clone)]
pub struct Reply {
    // The original message that this reply is in reply to.
    // This goes at most one level deep from the original message, and won't happen recursively if there are replies to replies to replies
    pub in_reply_to: Option<Box<DecodedMessage>>,
    pub content: Box<MessageBody>,
}

#[derive(Debug, Clone)]
pub enum ReactionAction {
    Added,
    Removed,
}

#[derive(Debug, Clone)]
pub enum ReactionSchema {
    Unicode,
    Shortcode,
    Custom,
}

#[derive(Debug, Clone)]
pub struct Reaction {
    pub metadata: DecodedMessageMetadata,
    pub action: ReactionAction,
    pub content: String,
    pub schema: ReactionSchema,
    pub reference: String,
    pub reference_inbox_id: String,
}

// Wrap text content in a struct to be consident with other content types
#[derive(Debug, Clone)]
pub struct Text {
    pub content: String,
}

#[derive(Debug, Clone)]
pub enum MessageBody {
    Text(Text),
    Reply(Reply),
    Reaction(ReactionV2),
    Attachment(Attachment),
    RemoteAttachment(RemoteAttachment),
    MultiRemoteAttachment(MultiRemoteAttachment),
    TransactionReference(TransactionReference),
    GroupUpdated(GroupUpdated),
    GroupMembershipChanges(GroupMembershipChanges),
    ReadReceipt(ReadReceipt),
    Custom(EncodedContent),
}

#[derive(Debug, Clone)]
pub struct DecodedMessageMetadata {
    // The message ID
    pub id: Vec<u8>,
    // The group ID
    pub group_id: Vec<u8>,
    // The timestamp of the message in nanoseconds
    pub sent_at_ns: i64,
    // The kind of message
    pub kind: GroupMessageKind,
    // The installation ID of the sender
    pub sender_installation_id: Vec<u8>,
    // The inbox ID of the sender
    pub sender_inbox_id: String,
    // The delivery status of the message
    pub delivery_status: DeliveryStatus,
    // The content type of the message
    pub content_type: ContentTypeId,
}

#[derive(Debug, Clone)]
pub struct DecodedMessage {
    pub metadata: DecodedMessageMetadata,
    // The content of the message
    pub content: MessageBody,
    // Fallback text for the message
    pub fallback_text: String,
    // A list of reactions
    pub reactions: Vec<Reaction>,
    // The number of replies to the message available
    pub num_replies: usize,
}
