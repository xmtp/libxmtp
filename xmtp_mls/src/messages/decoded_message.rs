use crate::groups::GroupError;
use crate::messages::enrichment::EnrichMessageError;
use prost::Message;
use xmtp_content_types::actions::{Actions, ActionsCodec};
use xmtp_content_types::delete_message::DeleteMessageCodec;
use xmtp_content_types::group_updated::GroupUpdatedCodec;
use xmtp_content_types::intent::{Intent, IntentCodec};
use xmtp_content_types::multi_remote_attachment::MultiRemoteAttachmentCodec;
use xmtp_content_types::reaction::{LegacyReactionCodec, ReactionCodec};
use xmtp_content_types::read_receipt::ReadReceiptCodec;
use xmtp_content_types::remote_attachment::RemoteAttachmentCodec;
use xmtp_content_types::reply::ReplyCodec;
use xmtp_content_types::transaction_reference::TransactionReferenceCodec;
use xmtp_content_types::wallet_send_calls::{WalletSendCalls, WalletSendCallsCodec};
use xmtp_content_types::{CodecError, ContentCodec};
use xmtp_content_types::{
    attachment::{Attachment, AttachmentCodec},
    read_receipt::ReadReceipt,
    remote_attachment::RemoteAttachment,
    text::TextCodec,
    transaction_reference::TransactionReference,
};
use xmtp_db::group_message::StoredGroupMessage;
use xmtp_db::group_message::{DeliveryStatus, GroupMessageKind};
use xmtp_proto::xmtp::mls::message_contents::content_types::DeleteMessage;
use xmtp_proto::xmtp::mls::message_contents::{
    ContentTypeId, EncodedContent, GroupUpdated,
    content_types::{MultiRemoteAttachment, ReactionV2},
};

#[derive(Debug, Clone)]
pub struct Reply {
    // The original message that this reply is in reply to.
    // This goes at most one level deep from the original message, and won't happen recursively if there are replies to replies to replies
    pub in_reply_to: Option<Box<DecodedMessage>>,
    pub content: Box<MessageBody>,
    pub reference_id: String,
}

// Wrap text content in a struct to be consident with other content types
#[derive(Debug, Clone)]
pub struct Text {
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeletedBy {
    /// Deleted by the original sender
    Sender,
    /// Deleted by a super admin
    Admin(String), // inbox_id of the admin who deleted the message
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
    ReadReceipt(ReadReceipt),
    WalletSendCalls(WalletSendCalls),
    Intent(Option<Intent>),
    Actions(Option<Actions>),
    /// The actual DeleteMessage content type (not shown in message lists)
    DeleteMessage(DeleteMessage),
    /// Placeholder for a message that has been deleted (shown in message lists)
    DeletedMessage {
        deleted_by: DeletedBy,
    },
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
    pub fallback_text: Option<String>,
    // A list of reactions
    pub reactions: Vec<DecodedMessage>,
    // The number of replies to the message available
    pub num_replies: usize,
}

impl TryFrom<EncodedContent> for MessageBody {
    type Error = GroupError;

    fn try_from(value: EncodedContent) -> Result<Self, Self::Error> {
        let content_type = match value.r#type.as_ref() {
            Some(content_type) => content_type,
            None => return Err(CodecError::InvalidContentType.into()),
        };

        match (content_type.type_id.as_str(), content_type.version_major) {
            (TextCodec::TYPE_ID, TextCodec::MAJOR_VERSION) => {
                let text = TextCodec::decode(value)?;
                Ok(MessageBody::Text(Text { content: text }))
            }
            (AttachmentCodec::TYPE_ID, AttachmentCodec::MAJOR_VERSION) => {
                let attachment = AttachmentCodec::decode(value)?;
                Ok(MessageBody::Attachment(attachment))
            }
            (RemoteAttachmentCodec::TYPE_ID, RemoteAttachmentCodec::MAJOR_VERSION) => {
                let remote_attachment = RemoteAttachmentCodec::decode(value)?;
                Ok(MessageBody::RemoteAttachment(remote_attachment))
            }
            (ReplyCodec::TYPE_ID, ReplyCodec::MAJOR_VERSION) => {
                let reply = ReplyCodec::decode(value)?;
                let content: MessageBody = reply.content.try_into()?;
                Ok(MessageBody::Reply(Reply {
                    in_reply_to: None,
                    content: Box::new(content),
                    reference_id: reply.reference,
                }))
            }
            (ReactionCodec::TYPE_ID, ReactionCodec::MAJOR_VERSION) => {
                let reaction = ReactionCodec::decode(value)?;
                Ok(MessageBody::Reaction(reaction))
            }
            (LegacyReactionCodec::TYPE_ID, LegacyReactionCodec::MAJOR_VERSION) => {
                let reaction = LegacyReactionCodec::decode(value)?;
                Ok(MessageBody::Reaction(reaction.into()))
            }
            (MultiRemoteAttachmentCodec::TYPE_ID, MultiRemoteAttachmentCodec::MAJOR_VERSION) => {
                let multi_remote_attachment = MultiRemoteAttachmentCodec::decode(value)?;
                Ok(MessageBody::MultiRemoteAttachment(multi_remote_attachment))
            }
            (TransactionReferenceCodec::TYPE_ID, TransactionReferenceCodec::MAJOR_VERSION) => {
                let transaction_reference = TransactionReferenceCodec::decode(value)?;
                Ok(MessageBody::TransactionReference(transaction_reference))
            }
            (GroupUpdatedCodec::TYPE_ID, GroupUpdatedCodec::MAJOR_VERSION) => {
                let group_updated = GroupUpdatedCodec::decode(value)?;
                Ok(MessageBody::GroupUpdated(group_updated))
            }
            (ReadReceiptCodec::TYPE_ID, ReadReceiptCodec::MAJOR_VERSION) => {
                let read_receipt = ReadReceiptCodec::decode(value)?;
                Ok(MessageBody::ReadReceipt(read_receipt))
            }
            (WalletSendCallsCodec::TYPE_ID, WalletSendCallsCodec::MAJOR_VERSION) => {
                let wallet_send_calls = WalletSendCallsCodec::decode(value)?;
                Ok(MessageBody::WalletSendCalls(wallet_send_calls))
            }
            (IntentCodec::TYPE_ID, IntentCodec::MAJOR_VERSION) => {
                let intent = IntentCodec::decode(value)?;
                Ok(MessageBody::Intent(Some(intent)))
            }
            (ActionsCodec::TYPE_ID, ActionsCodec::MAJOR_VERSION) => {
                let actions = ActionsCodec::decode(value)?;
                Ok(MessageBody::Actions(Some(actions)))
            }
            (DeleteMessageCodec::TYPE_ID, DeleteMessageCodec::MAJOR_VERSION) => {
                let delete_message = DeleteMessageCodec::decode(value)?;
                Ok(MessageBody::DeleteMessage(delete_message))
            }

            _ => Err(CodecError::CodecNotFound(content_type.clone()).into()),
        }
    }
}

impl TryFrom<StoredGroupMessage> for DecodedMessage {
    type Error = EnrichMessageError;

    fn try_from(value: StoredGroupMessage) -> Result<Self, Self::Error> {
        // Decode the message content from the stored bytes
        // If we can't get past this part, we return an error
        let encoded_content = EncodedContent::decode(&mut value.decrypted_message_bytes.as_slice())
            .map_err(|_| CodecError::InvalidContentType)?;
        let content_type_id = encoded_content.r#type.clone().unwrap_or_default();
        let fallback = encoded_content.fallback.clone();

        let content = match encoded_content.try_into() {
            Ok(content) => content,
            // TODO:(nm)
            // Rather than clone the encoded content by default, I am re-decoding the bytes
            // That feels dumb and wrong. Will figure out a better solution.
            Err(_) => MessageBody::Custom(
                EncodedContent::decode(&mut value.decrypted_message_bytes.as_slice())
                    .map_err(|e| CodecError::Decode(e.to_string()))?,
            ),
        };

        // Create the metadata
        let metadata = DecodedMessageMetadata {
            id: value.id,
            group_id: value.group_id,
            sent_at_ns: value.sent_at_ns,
            kind: value.kind,
            sender_installation_id: value.sender_installation_id,
            sender_inbox_id: value.sender_inbox_id,
            delivery_status: value.delivery_status,
            content_type: content_type_id,
        };

        // For now, we'll set default values for reactions and replies
        // These could be populated later if needed
        let reactions = Vec::new();
        let num_replies = 0;

        Ok(DecodedMessage {
            metadata,
            content,
            fallback_text: fallback,
            reactions,
            num_replies,
        })
    }
}
