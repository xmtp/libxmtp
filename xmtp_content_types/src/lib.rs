pub mod group_updated;
pub mod membership_change;
pub mod reaction;
pub mod read_receipt;
pub mod remote_attachment;
pub mod reply;
pub mod text;
pub mod transaction_reference;

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
