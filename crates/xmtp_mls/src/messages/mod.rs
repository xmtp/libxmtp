use prost::Message;
use xmtp_proto::xmtp::mls::message_contents::ContentTypeId;

pub mod decoded_message;
pub mod enrichment;

#[cfg(test)]
mod tests;

/// Decode only the content type of encoded content. prost skips the other
/// fields without copying them, so a payload that holds a device-sync HMAC
/// root key is never copied.
pub(crate) fn encoded_content_type(
    message: &[u8],
) -> Result<Option<ContentTypeId>, prost::DecodeError> {
    #[derive(Clone, PartialEq, prost::Message)]
    struct TypeOnly {
        #[prost(message, optional, tag = "1")]
        r#type: Option<ContentTypeId>,
    }
    Ok(TypeOnly::decode(message)?.r#type)
}
