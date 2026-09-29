use crate::groups::GroupError;
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

/// Why received content could not be decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentDecodeFailureKind {
    /// The bytes are not a typed `EncodedContent`: protobuf parsing failed,
    /// or the content type is absent or incomplete.
    MalformedEnvelope,
    /// A complete typed envelope whose compression, payload, or nested
    /// content failed to decode.
    CodecDecodeFailed,
}

/// The typed cause kept beside undecodable content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentDecodeFailure {
    pub kind: ContentDecodeFailureKind,
    pub message: String,
}

impl ContentDecodeFailure {
    fn malformed(message: impl Into<String>) -> Self {
        Self {
            kind: ContentDecodeFailureKind::MalformedEnvelope,
            message: message.into(),
        }
    }

    fn codec(message: impl ToString) -> Self {
        Self {
            kind: ContentDecodeFailureKind::CodecDecodeFailed,
            message: message.to_string(),
        }
    }
}

impl From<ContentDecodeFailure> for CodecError {
    fn from(failure: ContentDecodeFailure) -> Self {
        match failure.kind {
            ContentDecodeFailureKind::MalformedEnvelope => CodecError::InvalidContentType,
            ContentDecodeFailureKind::CodecDecodeFailed => CodecError::Decode(failure.message),
        }
    }
}

/// A complete typed envelope that no core codec decodes. A host codec may.
#[derive(Debug, Clone, PartialEq)]
pub struct CustomContent {
    /// The decompressed envelope: the input for a host codec.
    pub encoded: EncodedContent,
    /// The exact received serialization of this envelope, before decompression.
    pub raw_bytes: Vec<u8>,
}

/// Received content the client could not decode, kept with its exact bytes.
#[derive(Debug, Clone, PartialEq)]
pub struct UndecodableContent {
    /// The parsed envelope, when the bytes parsed as protobuf. This is
    /// evidence, not codec input: `content` is still compressed when
    /// `compression` is set, and its type can be incomplete.
    pub encoded: Option<EncodedContent>,
    /// The exact received bytes.
    pub raw_bytes: Vec<u8>,
    pub failure: ContentDecodeFailure,
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
    /// A complete typed envelope with no core codec. A nested custom body
    /// keeps its own exact bytes.
    Custom(CustomContent),
    /// Content that failed to decode, with its exact bytes and typed cause.
    /// A nested failure makes the outer body undecodable, so this variant
    /// appears only at the top level.
    Undecodable(UndecodableContent),
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
    /// The received content type identifier, exactly as the envelope carried
    /// it. Absent when the bytes did not parse or carried no identifier; it
    /// can be incomplete. The client never substitutes another type.
    pub content_type: Option<ContentTypeId>,
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

/// Strict decode of an in-memory envelope. A content failure is an error.
/// `Custom` raw bytes are this value's serialization, which is the input the
/// decoder consumed; use [`MessageBody::from_received_bytes`] for received
/// bytes that must be kept exactly.
impl TryFrom<EncodedContent> for MessageBody {
    type Error = GroupError;

    fn try_from(value: EncodedContent) -> Result<Self, Self::Error> {
        let bytes = value.encode_to_vec();
        Self::decode_bytes(&bytes, &mut compression::DecompressionBudget::new(), 0)
            .map_err(|failure| GroupError::CodecError(failure.into()))
    }
}

fn has_complete_type(content: &EncodedContent) -> Option<&ContentTypeId> {
    content
        .r#type
        .as_ref()
        .filter(|kind| !kind.authority_id.is_empty() && !kind.type_id.is_empty())
}

impl MessageBody {
    /// Decode received content from its exact bytes. Every content failure
    /// returns `Undecodable` with those bytes; this never fails.
    // implements: CTYPE-008
    pub fn from_received_bytes(raw: &[u8]) -> Self {
        Self::from_received(EncodedContent::decode(raw).ok(), raw)
    }

    fn from_received(envelope: Option<EncodedContent>, raw: &[u8]) -> Self {
        let mut budget = compression::DecompressionBudget::new();
        let result = match envelope {
            Some(envelope) => Self::decode_envelope(envelope, raw, &mut budget, 0),
            None => Err(ContentDecodeFailure::malformed(
                "bytes are not a serialized EncodedContent",
            )),
        };
        result.unwrap_or_else(|failure| {
            MessageBody::Undecodable(UndecodableContent {
                encoded: EncodedContent::decode(raw).ok(),
                raw_bytes: raw.to_vec(),
                failure,
            })
        })
    }

    fn decode_bytes(
        raw: &[u8],
        budget: &mut compression::DecompressionBudget,
        depth: usize,
    ) -> Result<Self, ContentDecodeFailure> {
        let envelope = EncodedContent::decode(raw).map_err(|error| {
            ContentDecodeFailure::malformed(format!(
                "bytes are not a serialized EncodedContent: {error}"
            ))
        })?;
        Self::decode_envelope(envelope, raw, budget, depth)
    }

    fn decode_envelope(
        envelope: EncodedContent,
        raw: &[u8],
        budget: &mut compression::DecompressionBudget,
        depth: usize,
    ) -> Result<Self, ContentDecodeFailure> {
        // An untyped envelope is malformed before anything else is checked.
        let malformed_type =
            || ContentDecodeFailure::malformed("content type identifier is absent or incomplete");
        has_complete_type(&envelope).ok_or_else(malformed_type)?;
        // implements: CTYPE-024, CTYPE-025
        let value = compression::decompress_with_budget(envelope, budget)
            .map_err(ContentDecodeFailure::codec)?;
        let content_type = has_complete_type(&value).ok_or_else(malformed_type)?;
        fn decoded<T>(result: Result<T, CodecError>) -> Result<T, ContentDecodeFailure> {
            result.map_err(ContentDecodeFailure::codec)
        }

        if content_type.authority_id != "xmtp.org" {
            return match (
                content_type.authority_id.as_str(),
                content_type.type_id.as_str(),
                content_type.version_major,
            ) {
                ("coinbase.com", IntentCodec::TYPE_ID, IntentCodec::MAJOR_VERSION) => Ok(
                    MessageBody::Intent(Some(decoded(IntentCodec::decode(value))?)),
                ),
                ("coinbase.com", ActionsCodec::TYPE_ID, ActionsCodec::MAJOR_VERSION) => Ok(
                    MessageBody::Actions(Some(decoded(ActionsCodec::decode(value))?)),
                ),
                _ => Ok(MessageBody::Custom(CustomContent {
                    encoded: value,
                    raw_bytes: raw.to_vec(),
                })),
            };
        }

        match (content_type.type_id.as_str(), content_type.version_major) {
            (TextCodec::TYPE_ID, TextCodec::MAJOR_VERSION) => {
                let text = decoded(TextCodec::decode(value))?;
                Ok(MessageBody::Text(Text { content: text }))
            }
            (MarkdownCodec::TYPE_ID, MarkdownCodec::MAJOR_VERSION) => {
                let markdown = decoded(MarkdownCodec::decode(value))?;
                Ok(MessageBody::Markdown(Markdown { content: markdown }))
            }
            (AttachmentCodec::TYPE_ID, AttachmentCodec::MAJOR_VERSION) => {
                let attachment = decoded(AttachmentCodec::decode(value))?;
                Ok(MessageBody::Attachment(attachment))
            }
            (RemoteAttachmentCodec::TYPE_ID, RemoteAttachmentCodec::MAJOR_VERSION) => {
                let remote_attachment = decoded(RemoteAttachmentCodec::decode(value))?;
                Ok(MessageBody::RemoteAttachment(remote_attachment))
            }
            (ReplyCodec::TYPE_ID, ReplyCodec::MAJOR_VERSION) => {
                if depth >= MAX_REPLY_NESTING_DEPTH {
                    return Err(ContentDecodeFailure::codec(format!(
                        "reply nesting exceeds {MAX_REPLY_NESTING_DEPTH} levels"
                    )));
                }
                // The reference is sender-controlled. A value that is not a
                // message id is a content failure, not missing history.
                let reference_id = value
                    .parameters
                    .get("reference")
                    .cloned()
                    .unwrap_or_default();
                if reference_id.is_empty() || hex::decode(&reference_id).is_err() {
                    return Err(ContentDecodeFailure::codec(
                        "reply reference is not a hex message id",
                    ));
                }
                // The decompressed outer content is the nested envelope's
                // exact serialization. A nested failure fails the outer body.
                // implements: CTYPE-027, CTYPE-028, CTYPE-029
                let content =
                    Self::decode_bytes(&value.content, budget, depth + 1).map_err(|failure| {
                        ContentDecodeFailure::codec(format!(
                            "nested reply content: {}",
                            failure.message
                        ))
                    })?;
                Ok(MessageBody::Reply(Reply {
                    in_reply_to: None,
                    content: Box::new(content),
                    reference_id,
                }))
            }
            (ReactionCodec::TYPE_ID, ReactionCodec::MAJOR_VERSION) => {
                let reaction = decoded(ReactionCodec::decode(value))?;
                Ok(MessageBody::Reaction(reaction))
            }
            (LegacyReactionCodec::TYPE_ID, LegacyReactionCodec::MAJOR_VERSION) => {
                let reaction = decoded(LegacyReactionCodec::decode(value))?;
                Ok(MessageBody::Reaction(reaction.into()))
            }
            (MultiRemoteAttachmentCodec::TYPE_ID, MultiRemoteAttachmentCodec::MAJOR_VERSION) => {
                let multi_remote_attachment = decoded(MultiRemoteAttachmentCodec::decode(value))?;
                Ok(MessageBody::MultiRemoteAttachment(multi_remote_attachment))
            }
            (TransactionReferenceCodec::TYPE_ID, TransactionReferenceCodec::MAJOR_VERSION) => {
                let transaction_reference = decoded(TransactionReferenceCodec::decode(value))?;
                Ok(MessageBody::TransactionReference(transaction_reference))
            }
            (GroupUpdatedCodec::TYPE_ID, GroupUpdatedCodec::MAJOR_VERSION) => {
                let group_updated = decoded(GroupUpdatedCodec::decode(value))?;
                Ok(MessageBody::GroupUpdated(group_updated))
            }
            (ReadReceiptCodec::TYPE_ID, ReadReceiptCodec::MAJOR_VERSION) => {
                let read_receipt = decoded(ReadReceiptCodec::decode(value))?;
                Ok(MessageBody::ReadReceipt(read_receipt))
            }
            (WalletSendCallsCodec::TYPE_ID, WalletSendCallsCodec::MAJOR_VERSION) => {
                let wallet_send_calls = decoded(WalletSendCallsCodec::decode(value))?;
                Ok(MessageBody::WalletSendCalls(wallet_send_calls))
            }
            (LeaveRequestCodec::TYPE_ID, LeaveRequestCodec::MAJOR_VERSION) => {
                let leave_request = decoded(LeaveRequestCodec::decode(value))?;
                Ok(MessageBody::LeaveRequest(leave_request))
            }

            _ => Ok(MessageBody::Custom(CustomContent {
                encoded: value,
                raw_bytes: raw.to_vec(),
            })),
        }
    }
}

/// Decode a stored message from its exact bytes. Content failures are kept
/// in the body, so every stored row produces a message.
impl From<StoredGroupMessage> for DecodedMessage {
    fn from(value: StoredGroupMessage) -> Self {
        let raw = value.decrypted_message_bytes.as_slice();
        let envelope = EncodedContent::decode(raw).ok();
        let (content_type, fallback) = envelope
            .as_ref()
            .map(|envelope| (envelope.r#type.clone(), envelope.fallback.clone()))
            .unwrap_or_default();
        let content = MessageBody::from_received(envelope, raw);

        let metadata = DecodedMessageMetadata {
            id: value.id,
            group_id: value.group_id,
            sent_at_ns: value.sent_at_ns,
            kind: value.kind,
            sender_installation_id: value.sender_installation_id,
            sender_inbox_id: value.sender_inbox_id,
            delivery_status: value.delivery_status,
            content_type,
            inserted_at_ns: value.inserted_at_ns,
            expires_at_ns: value.expire_at_ns,
        };

        DecodedMessage {
            metadata,
            content,
            fallback_text: fallback,
            // Enrichment fills reactions and reply counts.
            reactions: Vec::new(),
            num_replies: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xmtp_content_types::{
        actions::{Action, Actions},
        attachment::Attachment,
        compression::compress,
        intent::Intent,
        reaction::LegacyReaction,
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
    async fn standard_codecs_match_authority_for_all_types() {
        macro_rules! check {
            ($content:expr, $standard:pat) => {{
                let mut standard = $content;
                let id = standard.r#type.as_mut().expect("encoded type");
                id.version_minor = 9;
                let name = format!("{}/{}", id.authority_id, id.type_id);
                let decoded = DecodedMessage::from(stored_message(standard.clone()));
                assert!(matches!(decoded.content, $standard), "{name} did not decode");

                if standard.r#type.as_ref().expect("encoded type").authority_id != "xmtp.org" {
                    let mut wrong_standard = standard.clone();
                    wrong_standard.r#type.as_mut().expect("encoded type").authority_id =
                        "xmtp.org".into();
                    let decoded = DecodedMessage::from(stored_message(wrong_standard.clone()));
                    assert!(
                        matches!(decoded.content, MessageBody::Custom(actual) if actual.encoded == wrong_standard),
                        "{name} selected the wrong standard authority"
                    );
                }

                let mut custom = standard;
                custom.r#type.as_mut().expect("encoded type").authority_id =
                    "custom.example".into();
                let decoded = DecodedMessage::from(stored_message(custom.clone()));
                assert!(
                    matches!(decoded.content, MessageBody::Custom(actual) if actual.encoded == custom),
                    "{name} selected a codec for custom.example"
                );
            }};
        }

        check!(TextCodec::encode("text".into())?, MessageBody::Text(_));
        check!(
            MarkdownCodec::encode("markdown".into())?,
            MessageBody::Markdown(_)
        );
        check!(
            AttachmentCodec::encode(Attachment {
                filename: None,
                mime_type: "text/plain".into(),
                content: vec![1],
            })?,
            MessageBody::Attachment(_)
        );
        check!(
            RemoteAttachmentCodec::encode(RemoteAttachment::default())?,
            MessageBody::RemoteAttachment(_)
        );
        check!(
            ReplyCodec::encode(EncodedReply {
                reference: "0102".into(),
                reference_inbox_id: None,
                content: TextCodec::encode("reply".into())?,
            })?,
            MessageBody::Reply(_)
        );
        check!(
            ReactionCodec::encode(ReactionV2::default())?,
            MessageBody::Reaction(_)
        );
        check!(
            LegacyReactionCodec::encode(LegacyReaction {
                action: "added".into(),
                reference: "0102".into(),
                reference_inbox_id: None,
                schema: "unicode".into(),
                content: "👍".into(),
            })?,
            MessageBody::Reaction(_)
        );
        check!(
            MultiRemoteAttachmentCodec::encode(MultiRemoteAttachment::default())?,
            MessageBody::MultiRemoteAttachment(_)
        );
        check!(
            TransactionReferenceCodec::encode(TransactionReference {
                namespace: None,
                network_id: "1".into(),
                reference: "0x123".into(),
                metadata: None,
            })?,
            MessageBody::TransactionReference(_)
        );
        check!(
            GroupUpdatedCodec::encode(GroupUpdated::default())?,
            MessageBody::GroupUpdated(_)
        );
        check!(
            ReadReceiptCodec::encode(ReadReceipt {})?,
            MessageBody::ReadReceipt(_)
        );
        check!(
            WalletSendCallsCodec::encode(WalletSendCalls {
                version: "1".into(),
                chain_id: "1".into(),
                from: "0x123".into(),
                calls: vec![],
                capabilities: None,
            })?,
            MessageBody::WalletSendCalls(_)
        );
        check!(
            IntentCodec::encode(Intent {
                id: "one".into(),
                action_id: "two".into(),
                metadata: None,
            })?,
            MessageBody::Intent(_)
        );
        check!(
            ActionsCodec::encode(Actions {
                id: "one".into(),
                description: "actions".into(),
                actions: vec![Action {
                    id: "two".into(),
                    label: "two".into(),
                    image_url: None,
                    style: None,
                    expires_at: None,
                }],
                expires_at: None,
            })?,
            MessageBody::Actions(_)
        );
        check!(
            LeaveRequestCodec::encode(LeaveRequest::default())?,
            MessageBody::LeaveRequest(_)
        );
    }

    // verifies: CTYPE-024
    #[xmtp_common::test(unwrap_try = true)]
    async fn compressed_custom_content_is_decompressed_at_both_levels() {
        let inner = compress(unknown_content(), Compression::Gzip)?;
        let MessageBody::Custom(top) = DecodedMessage::from(stored_message(inner.clone())).content
        else {
            panic!("expected top-level custom content");
        };
        assert_eq!(top.encoded.compression, None);
        assert_eq!(top.encoded.content, b"custom payload");
        assert_eq!(top.raw_bytes, inner.encode_to_vec());

        let outer = ReplyCodec::encode(EncodedReply {
            reference: "0102".into(),
            reference_inbox_id: None,
            content: inner,
        })?;
        let MessageBody::Reply(reply) = MessageBody::try_from(outer.clone())? else {
            panic!("expected reply");
        };
        let MessageBody::Custom(nested) = *reply.content else {
            panic!("expected nested custom content");
        };
        assert_eq!(nested.encoded.compression, None);
        assert_eq!(nested.encoded.content, b"custom payload");
        assert_eq!(nested.raw_bytes, outer.content);
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
        let failure = MessageBody::decode_bytes(&bytes, &mut budget, 0).unwrap_err();
        assert_eq!(failure.kind, ContentDecodeFailureKind::CodecDecodeFailed);
        assert!(failure.message.contains("decompressed content exceeds"));
        assert!(budget.used() > 0);
        assert!(budget.peak_capacity() <= compression::MAX_DECOMPRESSED_BYTES + 65_536);
        eprintln!(
            "attack peak decompressed capacity: {} bytes",
            budget.peak_capacity()
        );

        let expected = EncodedContent::decode(bytes.as_slice())?;
        // Decode from the exact bytes: re-encoding a parameter map can
        // reorder its entries.
        let decoded = DecodedMessage::from(stored_bytes(bytes.clone()));
        let MessageBody::Undecodable(original) = decoded.content else {
            panic!("expected preserved decode failure");
        };
        assert_eq!(original.raw_bytes, bytes);
        assert_eq!(original.encoded, Some(expected));
        assert_eq!(
            original.failure.kind,
            ContentDecodeFailureKind::CodecDecodeFailed
        );
    }

    // verifies: CTYPE-008
    #[xmtp_common::test(unwrap_try = true)]
    async fn reply_depth_limit_preserves_message() {
        let content = nested_reply(
            TextCodec::encode("inner".into())?,
            MAX_REPLY_NESTING_DEPTH + 1,
            0,
        );
        let bytes = content.encode_to_vec();
        let decoded = DecodedMessage::from(stored_bytes(bytes.clone()));
        let MessageBody::Undecodable(original) = decoded.content else {
            panic!("expected preserved decode failure");
        };
        assert_eq!(original.raw_bytes, bytes);
        assert_eq!(original.encoded, Some(content));
        assert!(original.failure.message.contains("reply nesting exceeds"));
    }

    // verifies: CTYPE-008, CTYPE-024
    #[xmtp_common::test(unwrap_try = true)]
    async fn compressed_decode_failure_preserves_message() {
        let mut content = TextCodec::encode("unchanged".into())?;
        content.compression = Some(99);
        let bytes = content.encode_to_vec();
        let message = stored_message(content.clone());
        let decoded = DecodedMessage::from(message);
        assert_eq!(decoded.metadata.id, vec![1, 2, 3]);
        assert_eq!(decoded.metadata.content_type, content.r#type);
        let MessageBody::Undecodable(undecodable) = decoded.content else {
            panic!("expected original undecodable content");
        };
        assert_eq!(undecodable.raw_bytes, bytes);
        assert_eq!(undecodable.encoded, Some(content));
        assert_eq!(
            undecodable.failure.kind,
            ContentDecodeFailureKind::CodecDecodeFailed
        );
        assert!(undecodable.failure.message.contains("unknown compression"));
    }

    fn stored_bytes(bytes: Vec<u8>) -> StoredGroupMessage {
        let mut message = stored_message(EncodedContent::default());
        message.decrypted_message_bytes = bytes;
        message
    }

    /// Append an unknown field (number 200, length-delimited) so the exact
    /// serialization differs from the parsed value's re-encoding.
    fn with_unknown_field(mut bytes: Vec<u8>) -> Vec<u8> {
        bytes.extend_from_slice(&[0xc2, 0x0c, 0x03, b'x', b'y', b'z']);
        bytes
    }

    // verifies: CTYPE-008, CTYPE-009
    #[xmtp_common::test(unwrap_try = true)]
    async fn malformed_bytes_are_kept_with_their_cause() {
        let decoded = DecodedMessage::from(stored_bytes(vec![0xff]));
        assert_eq!(decoded.metadata.id, vec![1, 2, 3]);
        assert_eq!(decoded.metadata.content_type, None);
        assert_eq!(decoded.fallback_text, None);
        let MessageBody::Undecodable(undecodable) = decoded.content else {
            panic!("expected undecodable content, got {:?}", decoded.content);
        };
        assert_eq!(undecodable.raw_bytes, vec![0xff]);
        assert_eq!(undecodable.encoded, None);
        assert_eq!(
            undecodable.failure.kind,
            ContentDecodeFailureKind::MalformedEnvelope
        );
    }

    // verifies: CTYPE-008, CTYPE-009
    #[xmtp_common::test(unwrap_try = true)]
    async fn absent_or_partial_type_is_malformed_and_keeps_the_partial_identifier() {
        let mut untyped = TextCodec::encode("text".into())?;
        untyped.r#type = None;
        untyped.fallback = Some("kept".into());
        let mut partial = untyped.clone();
        partial.r#type = Some(ContentTypeId {
            authority_id: String::new(),
            type_id: "text".into(),
            version_major: 1,
            version_minor: 0,
        });
        // Invalid compression does not change the cause of an untyped envelope.
        let mut untyped_compressed = untyped.clone();
        untyped_compressed.compression = Some(99);
        let mut partial_compressed = partial.clone();
        partial_compressed.compression = Some(99);
        for content in [untyped, partial, untyped_compressed, partial_compressed] {
            let bytes = content.encode_to_vec();
            let decoded = DecodedMessage::from(stored_bytes(bytes.clone()));
            assert_eq!(decoded.metadata.content_type, content.r#type);
            assert_eq!(decoded.fallback_text.as_deref(), Some("kept"));
            let MessageBody::Undecodable(undecodable) = decoded.content else {
                panic!("expected undecodable content, got {:?}", decoded.content);
            };
            assert_eq!(undecodable.raw_bytes, bytes);
            assert_eq!(undecodable.encoded, Some(content));
            assert_eq!(
                undecodable.failure.kind,
                ContentDecodeFailureKind::MalformedEnvelope
            );
        }
    }

    // verifies: CTYPE-008, CTYPE-027
    #[xmtp_common::test(unwrap_try = true)]
    async fn exact_bytes_survive_at_both_levels() {
        let nested_bytes = with_unknown_field(unknown_content().encode_to_vec());
        let mut outer = ReplyCodec::encode(EncodedReply {
            reference: "0102".into(),
            reference_inbox_id: None,
            content: unknown_content(),
        })?;
        outer.content = nested_bytes.clone();
        let outer_bytes = with_unknown_field(outer.encode_to_vec());

        let MessageBody::Custom(top) =
            DecodedMessage::from(stored_bytes(nested_bytes.clone())).content
        else {
            panic!("expected custom content");
        };
        assert_eq!(top.raw_bytes, nested_bytes);
        assert_ne!(top.encoded.encode_to_vec(), nested_bytes);

        let MessageBody::Reply(reply) = DecodedMessage::from(stored_bytes(outer_bytes)).content
        else {
            panic!("expected reply");
        };
        assert_eq!(reply.reference_id, "0102");
        let MessageBody::Custom(nested) = *reply.content else {
            panic!("expected nested custom content");
        };
        assert_eq!(nested.raw_bytes, nested_bytes);
        assert_eq!(nested.encoded.content, b"custom payload");
    }

    // verifies: CTYPE-008, CTYPE-029
    #[xmtp_common::test(unwrap_try = true)]
    async fn nested_failures_make_the_outer_reply_undecodable() {
        let reply_with = |nested: Vec<u8>| {
            let mut outer = ReplyCodec::encode(EncodedReply {
                reference: "0102".into(),
                reference_inbox_id: None,
                content: TextCodec::encode("placeholder".into()).unwrap(),
            })
            .unwrap();
            outer.content = nested;
            outer.fallback = Some("outer fallback".into());
            outer
        };
        let mut untyped = TextCodec::encode("nested".into())?;
        untyped.r#type = None;
        let mut bad_compression = TextCodec::encode("nested".into())?;
        bad_compression.compression = Some(99);
        let mut bad_text = TextCodec::encode("nested".into())?;
        bad_text.content = vec![0xff, 0xfe];
        let cases = [
            ("malformed", vec![0xff]),
            ("untyped", untyped.encode_to_vec()),
            ("bad compression", bad_compression.encode_to_vec()),
            ("failed standard", bad_text.encode_to_vec()),
        ];
        for (name, nested) in cases {
            let outer = reply_with(nested);
            let bytes = outer.encode_to_vec();
            let decoded = DecodedMessage::from(stored_bytes(bytes.clone()));
            assert_eq!(
                decoded.fallback_text.as_deref(),
                Some("outer fallback"),
                "{name}"
            );
            assert_eq!(decoded.metadata.content_type, outer.r#type, "{name}");
            let MessageBody::Undecodable(undecodable) = decoded.content else {
                panic!(
                    "{name}: expected undecodable outer reply, got {:?}",
                    decoded.content
                );
            };
            assert_eq!(undecodable.raw_bytes, bytes, "{name}");
            assert_eq!(undecodable.encoded, Some(outer), "{name}");
            assert_eq!(
                undecodable.failure.kind,
                ContentDecodeFailureKind::CodecDecodeFailed,
                "{name}"
            );
            assert!(undecodable.failure.message.contains("nested"), "{name}");
        }
    }

    // verifies: CTYPE-008, CTYPE-029
    #[xmtp_common::test(unwrap_try = true)]
    async fn invalid_reply_reference_is_a_decode_failure() {
        for reference in ["not-valid-hex!@#", ""] {
            let outer = ReplyCodec::encode(EncodedReply {
                reference: reference.into(),
                reference_inbox_id: None,
                content: TextCodec::encode("reply".into())?,
            })?;
            let bytes = outer.encode_to_vec();
            let decoded = DecodedMessage::from(stored_bytes(bytes.clone()));
            let MessageBody::Undecodable(undecodable) = decoded.content else {
                panic!(
                    "{reference:?}: expected undecodable reply, got {:?}",
                    decoded.content
                );
            };
            assert_eq!(undecodable.raw_bytes, bytes);
            assert_eq!(
                undecodable.failure.kind,
                ContentDecodeFailureKind::CodecDecodeFailed
            );
            assert!(undecodable.failure.message.contains("reference"));
        }
    }

    // verifies: CTYPE-028
    #[xmtp_common::test(unwrap_try = true)]
    async fn nested_type_wins_over_the_outer_content_type_parameter() {
        let mut outer = ReplyCodec::encode(EncodedReply {
            reference: "0102".into(),
            reference_inbox_id: None,
            content: TextCodec::encode("placeholder".into())?,
        })?;
        assert_eq!(outer.parameters["contentType"], "xmtp.org/text:1.0");
        outer.content = ReactionCodec::encode(ReactionV2 {
            reference: "0102".into(),
            content: "👍".into(),
            ..Default::default()
        })?
        .encode_to_vec();
        let MessageBody::Reply(reply) = MessageBody::try_from(outer)? else {
            panic!("expected reply");
        };
        assert!(
            matches!(*reply.content, MessageBody::Reaction(ref reaction) if reaction.content == "👍"),
            "the outer contentType parameter selected the codec: {:?}",
            reply.content
        );
    }

    // verifies: CTYPE-009
    #[xmtp_common::test(unwrap_try = true)]
    async fn strict_decode_reports_failures_as_errors() {
        let mut untyped = TextCodec::encode("text".into())?;
        untyped.r#type = None;
        assert!(matches!(
            MessageBody::try_from(untyped),
            Err(GroupError::CodecError(CodecError::InvalidContentType))
        ));
        let mut bad_text = TextCodec::encode("text".into())?;
        bad_text.content = vec![0xff, 0xfe];
        assert!(matches!(
            MessageBody::try_from(bad_text),
            Err(GroupError::CodecError(CodecError::Decode(_)))
        ));
    }
}
