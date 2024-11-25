pub mod group_updated;
pub mod membership_change;
pub mod reaction;
#[cfg(test)]
mod test_utils;
pub mod text;
pub enum ContentType {
    GroupMembershipChange,
    GroupUpdated,
    Reaction,
    ReadReceipt,
    RemoteAttachment,
    Reply,
    Text,
    TransactionReference,
}

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
