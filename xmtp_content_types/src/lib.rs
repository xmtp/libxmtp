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
