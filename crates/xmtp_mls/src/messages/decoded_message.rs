use crate::groups::GroupError;
use crate::messages::enrichment::EnrichMessageError;
use prost::Message;
use xmtp_content_types::actions::{Actions, ActionsCodec};
use xmtp_content_types::group_updated::GroupUpdatedCodec;
use xmtp_content_types::intent::{Intent, IntentCodec};
use xmtp_content_types::leave_request::LeaveRequestCodec;
use xmtp_content_types::multi_remote_attachment::MultiRemoteAttachmentCodec;
use xmtp_content_types::reaction::{LegacyReactionCodec, ReactionCodec};
use xmtp_content_types::read_receipt::ReadReceiptCodec;
use xmtp_content_types::remote_attachment::RemoteAttachmentCodec;
use xmtp_content_types::reply::ReplyCodec;
use xmtp_content_types::transaction_reference::TransactionReferenceCodec;
use xmtp_content_types::wallet_send_calls::{WalletSendCalls, WalletSendCallsCodec};
use xmtp_content_types::{CodecError, ContentCodec, compression};
use xmtp_content_types::{
    attachment::{Attachment, AttachmentCodec},
    markdown::MarkdownCodec,
    read_receipt::ReadReceipt,
    remote_attachment::RemoteAttachment,
    text::TextCodec,
    transaction_reference::TransactionReference,
};
use xmtp_db::group_message::StoredGroupMessage;
use xmtp_db::group_message::{DeliveryStatus, GroupMessageKind};
use xmtp_proto::types::GroupId;
use xmtp_proto::xmtp::mls::message_contents::{
    ContentTypeId, EncodedContent, GroupUpdated,
    content_types::{LeaveRequest, MultiRemoteAttachment, ReactionV2},
};

#[derive(Debug, Clone)]
pub struct Reply {
    // The original message that this reply is in reply to.
    // This goes at most one level deep from the original message, and won't happen recursively if there are replies to replies to replies
    pub in_reply_to: Option<Box<DecodedMessage>>,
    pub content: Box<MessageBody>,
    pub reference_id: String,
}

// Wrap text content in a struct to be consistent with other content types
#[derive(Debug, Clone)]
pub struct Text {
    pub content: String,
}

// Wrap markdown content in a struct to be consistent with other content types
#[derive(Debug, Clone)]
pub struct Markdown {
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
    Markdown(Markdown),
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
    LeaveRequest(LeaveRequest),
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
    pub group_id: GroupId,
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
    // Time in nanoseconds the message was inserted into the database
    pub inserted_at_ns: i64,
    // Timestamp (in NS) after which the message must be deleted
    pub expires_at_ns: Option<i64>,
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

/// Maximum number of reply envelopes inside one message.
const MAX_REPLY_NESTING_DEPTH: usize = 8;

impl TryFrom<EncodedContent> for MessageBody {
    type Error = GroupError;

    fn try_from(value: EncodedContent) -> Result<Self, Self::Error> {
        Self::decode_with_budget(value, &mut compression::DecompressionBudget::new(), 0)
    }
}

impl MessageBody {
    fn decode_with_budget(
        value: EncodedContent,
        budget: &mut compression::DecompressionBudget,
        depth: usize,
    ) -> Result<Self, GroupError> {
        // implements: CTYPE-024, CTYPE-025
        let value = compression::decompress_with_budget(value, budget)?;
        let content_type = match value.r#type.as_ref() {
            Some(content_type) => content_type,
            None => return Err(CodecError::InvalidContentType.into()),
        };

        match (content_type.type_id.as_str(), content_type.version_major) {
            (TextCodec::TYPE_ID, TextCodec::MAJOR_VERSION) => {
                let text = TextCodec::decode(value)?;
                Ok(MessageBody::Text(Text { content: text }))
            }
            (MarkdownCodec::TYPE_ID, MarkdownCodec::MAJOR_VERSION) => {
                let markdown = MarkdownCodec::decode(value)?;
                Ok(MessageBody::Markdown(Markdown { content: markdown }))
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
                if depth >= MAX_REPLY_NESTING_DEPTH {
                    return Err(CodecError::Decode(format!(
                        "reply nesting exceeds {MAX_REPLY_NESTING_DEPTH} levels"
                    ))
                    .into());
                }
                let reply = ReplyCodec::decode(value)?;
                let content = Self::decode_with_budget(reply.content, budget, depth + 1)?;
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
            (GroupUpdatedCodec::TYPE_ID, GroupUpdatedCodec::MAJOR_VERSION)
                if content_type.authority_id == "xmtp.org" =>
            {
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
            (LeaveRequestCodec::TYPE_ID, LeaveRequestCodec::MAJOR_VERSION) => {
                let leave_request = LeaveRequestCodec::decode(value)?;
                Ok(MessageBody::LeaveRequest(leave_request))
            }

            _ => Ok(MessageBody::Custom(value)),
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

        let content = match MessageBody::decode_with_budget(
            encoded_content,
            &mut compression::DecompressionBudget::new(),
            0,
        ) {
            Ok(content) => content,
            // The original envelope stays available to the app on decode failure.
            // implements: CTYPE-008
            Err(_) => MessageBody::Custom(
                EncodedContent::decode(value.decrypted_message_bytes.as_slice())
                    .map_err(|_| CodecError::InvalidContentType)?,
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
            inserted_at_ns: value.inserted_at_ns,
            expires_at_ns: value.expire_at_ns,
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

#[cfg(test)]
mod tests {
    use super::*;
    use xmtp_content_types::{
        compression::compress,
        reply::{Reply as EncodedReply, ReplyCodec},
    };
    use xmtp_proto::xmtp::mls::message_contents::Compression;

    // verifies: CTYPE-024
    #[xmtp_common::test(unwrap_try = true)]
    async fn nested_content_decompressed_before_decode() {
        let inner = compress(TextCodec::encode("nested text".into())?, Compression::Gzip)?;
        let outer = ReplyCodec::encode(EncodedReply {
            reference: "0102".into(),
            reference_inbox_id: None,
            content: inner,
        })?;
        let outer = compress(outer, Compression::Deflate)?;
        let fields = crate::groups::QueryableContentFields::try_from(outer.clone())?;
        assert_eq!(fields.reference_id, Some(vec![1, 2]));
        let MessageBody::Reply(reply) = MessageBody::try_from(outer)? else {
            panic!("expected reply");
        };
        let MessageBody::Text(text) = *reply.content else {
            panic!("expected nested text");
        };
        assert_eq!(text.content, "nested text");
    }

    fn stored_message(content: EncodedContent) -> StoredGroupMessage {
        StoredGroupMessage {
            id: vec![1, 2, 3],
            group_id: GroupId::ONE,
            decrypted_message_bytes: content.encode_to_vec(),
            sent_at_ns: 1,
            kind: GroupMessageKind::Application,
            sender_installation_id: vec![4],
            sender_inbox_id: "inbox".into(),
            delivery_status: DeliveryStatus::Published,
            content_type: xmtp_db::group_message::ContentType::Text,
            version_major: 1,
            version_minor: 0,
            authority_id: "xmtp.org".into(),
            reference_id: None,
            sequence_id: 1,
            envelope_hash: None,
            expiry_ns: None,
            inserted_at_ns: 0,
            expire_at_ns: None,
            should_push: false,
            idempotency_key: String::new(),
        }
    }

    fn unknown_content() -> EncodedContent {
        let mut content = TextCodec::encode("custom payload".into()).unwrap();
        content.r#type.as_mut().unwrap().type_id = "custom".into();
        content
    }

    // verifies: CTYPE-001, CTYPE-008
    #[xmtp_common::test(unwrap_try = true)]
    async fn custom_authority_group_updated_stays_custom() {
        let mut content = GroupUpdatedCodec::encode(GroupUpdated::default())?;
        content.r#type.as_mut().unwrap().authority_id = "custom.example".into();
        let decoded = DecodedMessage::try_from(stored_message(content.clone()))?;
        assert!(
            matches!(decoded.content, MessageBody::Custom(actual) if actual == content),
            "custom authority selected the standard group-updated codec"
        );
    }

    // verifies: CTYPE-024
    #[xmtp_common::test(unwrap_try = true)]
    async fn compressed_custom_content_is_decompressed_at_both_levels() {
        let inner = compress(unknown_content(), Compression::Gzip)?;
        let MessageBody::Custom(top) =
            DecodedMessage::try_from(stored_message(inner.clone()))?.content
        else {
            panic!("expected top-level custom content");
        };
        assert_eq!(top.compression, None);
        assert_eq!(top.content, b"custom payload");

        let outer = ReplyCodec::encode(EncodedReply {
            reference: "0102".into(),
            reference_inbox_id: None,
            content: inner,
        })?;
        let MessageBody::Reply(reply) = MessageBody::try_from(outer)? else {
            panic!("expected reply");
        };
        let MessageBody::Custom(nested) = *reply.content else {
            panic!("expected nested custom content");
        };
        assert_eq!(nested.compression, None);
        assert_eq!(nested.content, b"custom payload");
    }

    fn nested_reply(mut content: EncodedContent, levels: usize, pad_to: usize) -> EncodedContent {
        for _ in 0..levels {
            let mut reply = ReplyCodec::encode(EncodedReply {
                reference: "0102".into(),
                reference_inbox_id: None,
                content,
            })
            .unwrap();
            if pad_to > reply.content.len() + 6 {
                // An unknown protobuf field pads this reply without changing its body.
                let padding = pad_to - reply.content.len() - 6;
                reply.content.extend_from_slice(&[0xa2, 0x06]);
                let mut length = padding;
                while length >= 0x80 {
                    reply.content.push((length as u8) | 0x80);
                    length >>= 7;
                }
                reply.content.push(length as u8);
                reply.content.resize(reply.content.len() + padding, 0);
            }
            content = compress(reply, Compression::Deflate).unwrap();
        }
        content
    }

    // verifies: CTYPE-008, CTYPE-025
    #[xmtp_common::test(unwrap_try = true)]
    async fn nested_compressed_reply_uses_one_budget() {
        let attack = nested_reply(
            TextCodec::encode("inner".into())?,
            20,
            compression::MAX_DECOMPRESSED_BYTES - 4096,
        );
        let bytes = attack.encode_to_vec();
        let mut budget = compression::DecompressionBudget::new();
        let error = MessageBody::decode_with_budget(attack, &mut budget, 0).unwrap_err();
        assert!(matches!(
            error,
            GroupError::CodecError(CodecError::Decode(message))
                if message.contains("decompressed content exceeds")
        ));
        assert!(budget.used() > 0);
        assert!(budget.peak_capacity() <= compression::MAX_DECOMPRESSED_BYTES + 65_536);
        eprintln!(
            "attack peak decompressed capacity: {} bytes",
            budget.peak_capacity()
        );

        let expected = EncodedContent::decode(bytes.as_slice())?;
        let decoded = DecodedMessage::try_from(stored_message(expected.clone()))?;
        let MessageBody::Custom(original) = decoded.content else {
            panic!("expected preserved decode failure");
        };
        assert_eq!(original, expected);
    }

    // verifies: CTYPE-008
    #[xmtp_common::test(unwrap_try = true)]
    async fn reply_depth_limit_preserves_message() {
        let content = nested_reply(
            TextCodec::encode("inner".into())?,
            MAX_REPLY_NESTING_DEPTH + 1,
            0,
        );
        let expected = content.clone();
        let decoded = DecodedMessage::try_from(stored_message(content))?;
        let MessageBody::Custom(original) = decoded.content else {
            panic!("expected preserved decode failure");
        };
        assert_eq!(original, expected);
    }

    // verifies: CTYPE-008, CTYPE-024
    #[xmtp_common::test(unwrap_try = true)]
    async fn compressed_decode_failure_preserves_message() {
        let mut content = TextCodec::encode("unchanged".into())?;
        content.compression = Some(99);
        let bytes = content.encode_to_vec();
        let message = stored_message(content);
        let decoded = DecodedMessage::try_from(message)?;
        assert_eq!(decoded.metadata.id, vec![1, 2, 3]);
        let MessageBody::Custom(custom) = decoded.content else {
            panic!("expected original custom content");
        };
        assert_eq!(custom.encode_to_vec(), bytes);
    }
}
