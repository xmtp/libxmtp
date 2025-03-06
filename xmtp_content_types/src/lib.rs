pub mod attachment;
pub mod group_updated;
pub mod membership_change;
pub mod multi_remote_attachment;
pub mod reaction;
pub mod read_receipt;
pub mod remote_attachment;
pub mod reply;
pub mod text;
pub mod transaction_reference;

use prost::Message;
use thiserror::Error;
use xmtp_proto::xmtp::mls::message_contents::{ContentTypeId, EncodedContent};

#[derive(Debug, Error)]
pub enum CodecError {
    #[error("encode error {0}")]
    Encode(String),
    #[error("decode error {0}")]
    Decode(String),
}

pub enum ContentType {
    Text,
    GroupMembershipChange,
    GroupUpdated,
    Reaction,
    ReadReceipt,
    Reply,
    Attachment,
    RemoteAttachment,
    MultiRemoteAttachment,
    TransactionReference,
}

impl TryFrom<&str> for ContentType {
    type Error = String;

    fn try_from(type_id: &str) -> Result<Self, Self::Error> {
        match type_id {
            text::TextCodec::TYPE_ID => Ok(Self::Text),
            membership_change::GroupMembershipChangeCodec::TYPE_ID => {
                Ok(Self::GroupMembershipChange)
            }
            group_updated::GroupUpdatedCodec::TYPE_ID => Ok(Self::GroupUpdated),
            reaction::ReactionCodec::TYPE_ID => Ok(Self::Reaction),
            read_receipt::ReadReceiptCodec::TYPE_ID => Ok(Self::ReadReceipt),
            reply::ReplyCodec::TYPE_ID => Ok(Self::Reply),
            attachment::AttachmentCodec::TYPE_ID => Ok(Self::Attachment),
            remote_attachment::RemoteAttachmentCodec::TYPE_ID => Ok(Self::RemoteAttachment),
            multi_remote_attachment::MultiRemoteAttachmentCodec::TYPE_ID => {
                Ok(Self::MultiRemoteAttachment)
            }
            transaction_reference::TransactionReferenceCodec::TYPE_ID => {
                Ok(Self::TransactionReference)
            }
            _ => Err(format!("Unknown content type ID: {}", type_id)),
        }
    }
}

// Represents whether this message type should send pushed notification when received by a user
pub fn should_push(content_type_id: String) -> bool {
    let content_type = ContentType::try_from(content_type_id.as_str()).ok();
    if let Some(content_type) = content_type {
        match content_type {
            ContentType::Text => true,
            ContentType::GroupMembershipChange => false,
            ContentType::GroupUpdated => false,
            ContentType::Reaction => false,
            ContentType::ReadReceipt => false,
            ContentType::Reply => true,
            ContentType::Attachment => true,
            ContentType::RemoteAttachment => true,
            ContentType::MultiRemoteAttachment => true,
            ContentType::TransactionReference => true,
        }
    } else {
        tracing::debug!("LOPI falling out here");
        true
    }
}

pub trait ContentCodec<T> {
    fn content_type() -> ContentTypeId;
    fn encode(content: T) -> Result<EncodedContent, CodecError>;
    fn decode(content: EncodedContent) -> Result<T, CodecError>;
}

pub fn encoded_content_to_bytes(content: EncodedContent) -> Vec<u8> {
    let mut buf = Vec::new();
    content.encode(&mut buf).unwrap();
    buf
}

pub fn bytes_to_encoded_content(bytes: Vec<u8>) -> EncodedContent {
    EncodedContent::decode(&mut bytes.as_slice()).unwrap()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn test_encoded_content_conversion() {
        // Create a sample EncodedContent
        let original = EncodedContent {
            r#type: Some(ContentTypeId {
                authority_id: "".to_string(),
                type_id: "test".to_string(),
                version_major: 0,
                version_minor: 0,
            }),
            parameters: HashMap::new(),
            compression: None,
            content: vec![1, 2, 3, 4],
            fallback: Some("test".to_string()),
        };

        // Convert to bytes
        let bytes = encoded_content_to_bytes(original.clone());

        // Convert back to EncodedContent
        let recovered = bytes_to_encoded_content(bytes);

        // Verify the recovered content matches the original
        assert_eq!(recovered.content, original.content);
        assert_eq!(recovered.fallback, original.fallback);
    }
}
