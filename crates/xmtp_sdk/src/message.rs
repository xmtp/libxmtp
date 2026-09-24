use prost::Message as _;
use xmtp_content_types::{ContentCodec, text::TextCodec};
use xmtp_db::group_message::{
    DeliveryStatus as StoredDeliveryStatus, GroupMessageKind, StoredGroupMessage,
};
use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

use crate::{ConversationID, InboxID, MessageID, Timestamp, XmtpError};

#[derive(Clone, Debug, uniffi::Enum)]
pub enum MessageKind {
    Application,
    MembershipChange,
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum DeliveryStatus {
    Unpublished,
    Published,
    Failed,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct ContentTypeId {
    pub authority_id: String,
    pub type_id: String,
    pub version_major: u32,
    pub version_minor: u32,
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum MessageContent {
    Text(String),
    Unknown { encoded: Vec<u8> },
}

/// A message value contains records and enums only.
#[derive(Clone, Debug, uniffi::Record)]
pub struct MessageData {
    pub id: MessageID,
    pub client_key: u64,
    pub conversation_id: ConversationID,
    pub sender_inbox_id: InboxID,
    pub sent_at: Timestamp,
    pub kind: MessageKind,
    pub delivery_status: DeliveryStatus,
    pub content_type: ContentTypeId,
    pub fallback: Option<String>,
    pub content: MessageContent,
}

/// The host runtime lifts this value to a Message class.
#[derive(Clone, Debug)]
pub struct Message(pub MessageData);

uniffi::custom_newtype!(Message, MessageData);

impl Message {
    pub(crate) fn from_stored(
        value: StoredGroupMessage,
        client_key: u64,
    ) -> Result<Self, XmtpError> {
        let encoded = EncodedContent::decode(value.decrypted_message_bytes.as_slice()).ok();
        let content_type = encoded
            .as_ref()
            .and_then(|content| content.r#type.clone())
            .unwrap_or_default();
        let fallback = encoded.as_ref().and_then(|content| content.fallback.clone());
        let content = match encoded {
            Some(encoded)
                if content_type.authority_id == "xmtp.org"
                    && content_type.type_id == TextCodec::TYPE_ID =>
            {
                TextCodec::decode(encoded)
                    .map(MessageContent::Text)
                    .unwrap_or(MessageContent::Unknown {
                        encoded: value.decrypted_message_bytes,
                    })
            }
            _ => MessageContent::Unknown {
                encoded: value.decrypted_message_bytes,
            },
        };
        let kind = match value.kind {
            GroupMessageKind::Application => MessageKind::Application,
            GroupMessageKind::MembershipChange => MessageKind::MembershipChange,
        };
        let delivery_status = match value.delivery_status {
            StoredDeliveryStatus::Unpublished => DeliveryStatus::Unpublished,
            StoredDeliveryStatus::Published => DeliveryStatus::Published,
            StoredDeliveryStatus::Failed => DeliveryStatus::Failed,
        };
        Ok(Self(MessageData {
            id: MessageID::from_bytes(&value.id)?,
            client_key,
            conversation_id: value.group_id.into(),
            sender_inbox_id: InboxID::try_from(value.sender_inbox_id)?,
            sent_at: Timestamp(value.sent_at_ns),
            kind,
            delivery_status,
            content_type: ContentTypeId {
                authority_id: content_type.authority_id,
                type_id: content_type.type_id,
                version_major: content_type.version_major,
                version_minor: content_type.version_minor,
            },
            fallback,
            content,
        }))
    }
}
