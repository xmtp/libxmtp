use super::encoded_content_type;
use prost::Message;
use xmtp_content_types::actions::{Actions, ActionsCodec};
use xmtp_content_types::group_updated::GroupUpdatedCodec;
use xmtp_content_types::intent::{Intent, IntentCodec};
use xmtp_content_types::leave_request::LeaveRequestCodec;
use xmtp_content_types::multi_remote_attachment::MultiRemoteAttachmentCodec;
use xmtp_content_types::reaction::{self, LegacyReactionCodec, ReactionCodec};
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

impl DeletedBy {
    /// Who deleted a message: its sender, or the admin with inbox
    /// `deleter_inbox_id`.
    pub fn new(deleter_inbox_id: &str, sender_inbox_id: &str) -> Self {
        if deleter_inbox_id == sender_inbox_id {
            DeletedBy::Sender
        } else {
            DeletedBy::Admin(deleter_inbox_id.to_string())
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
    // The latest reaction messages that decode as reactions, oldest first:
    // at most `MESSAGE_REACTION_LIMIT` reaction messages are read.
    pub reactions: Vec<DecodedMessage>,
    // The number of replies to the message available
    pub num_replies: usize,
    // Unexpired reaction messages to this message, including removals and
    // ones that do not decode. It is not a reaction tally.
    pub num_reaction_messages: usize,
}

#[cfg(feature = "conformance")]
#[path = "decode_counter.rs"]
pub mod decode_counter;

/// Maximum number of reply envelopes inside one message.
pub const MAX_REPLY_NESTING_DEPTH: usize = 8;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Origin {
    StoredTranscript,
    Application,
}

/// Strict decode of an in-memory envelope. A content failure is an error.
/// `Custom` raw bytes are this value's serialization, which is the input the
/// decoder consumed; use [`MessageBody::from_received_bytes`] for received
/// bytes that must be kept exactly.
impl TryFrom<EncodedContent> for MessageBody {
    type Error = CodecError;

    fn try_from(value: EncodedContent) -> Result<Self, Self::Error> {
        let bytes = value.encode_to_vec();
        Self::decode_bytes(
            &bytes,
            &mut compression::DecompressionBudget::new(),
            0,
            Origin::Application,
        )
        .map_err(CodecError::from)
    }
}

fn has_complete_type(content: &EncodedContent) -> Option<&ContentTypeId> {
    content
        .r#type
        .as_ref()
        .filter(|kind| !kind.authority_id.is_empty() && !kind.type_id.is_empty())
}

/// Whether a type is a reserved transcript under the standard authority, at any version.
pub(crate) fn is_reserved_transcript_type(content_type: &ContentTypeId) -> bool {
    content_type.authority_id == "xmtp.org"
        && matches!(
            content_type.type_id.as_str(),
            "group_updated" | "group_membership_change"
        )
}

/// Refuse reserved types at any depth, or replies that exceed the bounded scan.
/// Other malformed content does not identify a reserved transcript.
pub fn carries_reserved_transcript(raw: &[u8]) -> bool {
    let mut budget = compression::DecompressionBudget::new();
    let mut current = std::borrow::Cow::Borrowed(raw);
    let mut reply_depth = 0;
    loop {
        let Ok(Some(kind)) = encoded_content_type(&current) else {
            return false;
        };
        if kind.authority_id.is_empty() || kind.type_id.is_empty() {
            return false;
        }
        if is_reserved_transcript_type(&kind) {
            return true;
        }
        let reply = kind.authority_id == "xmtp.org"
            && kind.type_id == ReplyCodec::TYPE_ID
            && kind.version_major == ReplyCodec::MAJOR_VERSION;
        if !reply {
            return false;
        }
        if reply_depth >= MAX_REPLY_NESTING_DEPTH {
            return true;
        }
        reply_depth += 1;
        let Ok(envelope) = EncodedContent::decode(&*current) else {
            return false;
        };
        let Ok(value) = compression::decompress_with_budget(envelope, &mut budget) else {
            return budget.limit_exceeded();
        };
        current = std::borrow::Cow::Owned(value.content);
    }
}

impl MessageBody {
    /// Decode received content from its exact bytes. Every content failure
    /// returns `Undecodable` with those bytes; this never fails.
    // implements: CTYPE-008
    pub fn from_received_bytes(raw: &[u8]) -> Self {
        Self::from_received(EncodedContent::decode(raw).ok(), raw, Origin::Application)
    }

    /// Decode stored content using the kind assigned when the client wrote the row.
    pub fn from_stored_bytes(raw: &[u8], kind: GroupMessageKind) -> Self {
        let origin = match kind {
            GroupMessageKind::MembershipChange => Origin::StoredTranscript,
            GroupMessageKind::Application => Origin::Application,
        };
        Self::from_received(EncodedContent::decode(raw).ok(), raw, origin)
    }

    fn from_received(envelope: Option<EncodedContent>, raw: &[u8], origin: Origin) -> Self {
        let mut budget = compression::DecompressionBudget::new();
        let result = match envelope {
            Some(envelope) => Self::decode_envelope(envelope, raw, &mut budget, 0, origin),
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
        origin: Origin,
    ) -> Result<Self, ContentDecodeFailure> {
        // implements: CTYPE-033
        xmtp_content_types::decode_limits::check_protobuf_fields(raw)
            .map_err(ContentDecodeFailure::codec)?;
        let envelope = EncodedContent::decode(raw).map_err(|error| {
            ContentDecodeFailure::malformed(format!(
                "bytes are not a serialized EncodedContent: {error}"
            ))
        })?;
        Self::decode_envelope(envelope, raw, budget, depth, origin)
    }

    fn decode_envelope(
        envelope: EncodedContent,
        raw: &[u8],
        budget: &mut compression::DecompressionBudget,
        depth: usize,
        origin: Origin,
    ) -> Result<Self, ContentDecodeFailure> {
        // An untyped envelope is malformed before anything else is checked.
        let malformed_type =
            || ContentDecodeFailure::malformed("content type identifier is absent or incomplete");
        let content_type = has_complete_type(&envelope).ok_or_else(malformed_type)?;
        // implements: GMOD-053
        if origin == Origin::Application && is_reserved_transcript_type(content_type) {
            return Err(ContentDecodeFailure::codec(
                "transcript type in application content",
            ));
        }
        // implements: CTYPE-024, CTYPE-025, CTYPE-034
        let value = reaction::decompress_with_reaction_limit(envelope, budget)
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
                #[cfg(feature = "conformance")]
                decode_counter::record(&value.content);
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
                // The reference is sender-controlled and kept as received. A
                // value that is not a message id resolves to no parent; the
                // reply itself is valid content.
                let reference_id = value
                    .parameters
                    .get("reference")
                    .cloned()
                    .unwrap_or_default();
                // The decompressed outer content is the nested envelope's
                // exact serialization. A nested failure fails the outer body.
                // implements: CTYPE-027, CTYPE-028, CTYPE-029
                let content =
                    Self::decode_bytes(&value.content, budget, depth + 1, Origin::Application)
                        .map_err(|failure| {
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

impl DecodedMessage {
    /// The deleted-message placeholder for a marked or expired message: its
    /// identity and metadata, with no body, fallback, reactions or counts.
    /// A body decoded from nothing would read as an empty message or as a
    /// decode failure.
    ///
    /// `deleted_by` is the actor of the delete. Expiry cleanup has no actor.
    /// Its callers pass `DeletedBy::Sender`: `DeletedBy` has no expiry
    /// variant, and adding one changes the binding types. No admin removed
    /// the message, and `Admin` needs an inbox id.
    // implements: META-051, PROC-060
    pub fn placeholder(value: StoredGroupMessage, deleted_by: DeletedBy) -> Self {
        DecodedMessage {
            metadata: DecodedMessageMetadata {
                id: value.id,
                group_id: value.group_id,
                sent_at_ns: value.sent_at_ns,
                kind: value.kind,
                sender_installation_id: value.sender_installation_id,
                sender_inbox_id: value.sender_inbox_id,
                delivery_status: value.delivery_status,
                content_type: Some(crate::enrichment::deleted_message_content_type()),
                inserted_at_ns: value.inserted_at_ns,
                expires_at_ns: value.expire_at_ns,
            },
            content: MessageBody::DeletedMessage { deleted_by },
            fallback_text: None,
            reactions: Vec::new(),
            num_replies: 0,
            num_reaction_messages: 0,
        }
    }
}

/// Decode a stored message from its exact bytes. Content failures are kept
/// in the body, so every stored row produces a message.
impl From<StoredGroupMessage> for DecodedMessage {
    fn from(value: StoredGroupMessage) -> Self {
        // implements: PROC-060
        if let Some(deleter) = value.deleted_by_inbox_id.as_deref() {
            let deleted_by = DeletedBy::new(deleter, &value.sender_inbox_id);
            return Self::placeholder(value, deleted_by);
        }
        let raw = value.decrypted_message_bytes.as_slice();
        let envelope = EncodedContent::decode(raw).ok();
        let (content_type, fallback) = envelope
            .as_ref()
            .map(|envelope| (envelope.r#type.clone(), envelope.fallback.clone()))
            .unwrap_or_default();
        let content = MessageBody::from_stored_bytes(raw, value.kind);

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
            num_reaction_messages: 0,
        }
    }
}

#[cfg(test)]
mod tests;
