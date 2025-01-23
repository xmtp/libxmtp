pub mod attachment;
pub mod group_updated;
pub mod membership_change;
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
