use std::collections::HashMap;

use prost::Message;

use super::{CodecError, ContentCodec};
use xmtp_proto::xmtp::mls::message_contents::content_types::DeleteMessage;
use xmtp_proto::xmtp::mls::message_contents::{ContentTypeId, EncodedContent};

pub struct DeleteMessageCodec;

impl DeleteMessageCodec {
    const AUTHORITY_ID: &'static str = "xmtp.org";
    pub const TYPE_ID: &'static str = "deleteMessage";
    pub const MAJOR_VERSION: u32 = 1;
    pub const MINOR_VERSION: u32 = 0;

    fn fallback() -> Option<String> {
        Some("Deleted a message".to_string())
    }
}

impl ContentCodec<DeleteMessage> for DeleteMessageCodec {
    fn content_type() -> ContentTypeId {
        ContentTypeId {
            authority_id: DeleteMessageCodec::AUTHORITY_ID.to_string(),
            type_id: DeleteMessageCodec::TYPE_ID.to_string(),
            version_major: DeleteMessageCodec::MAJOR_VERSION,
            version_minor: DeleteMessageCodec::MINOR_VERSION,
        }
    }

    fn encode(data: DeleteMessage) -> Result<EncodedContent, CodecError> {
        let mut buf = Vec::new();
        data.encode(&mut buf)
            .map_err(|e| CodecError::Encode(e.to_string()))?;

        Ok(EncodedContent {
            r#type: Some(DeleteMessageCodec::content_type()),
            parameters: HashMap::new(),
            fallback: Self::fallback(),
            compression: None,
            content: buf,
        })
    }

    fn decode(content: EncodedContent) -> Result<DeleteMessage, CodecError> {
        let decoded = DeleteMessage::decode(content.content.as_slice())
            .map_err(|e| CodecError::Decode(e.to_string()))?;

        Ok(decoded)
    }

    fn should_push() -> bool {
        false
    }
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);
    use super::*;

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_encode_decode() {
        let data = DeleteMessage {
            message_id: "test_message_id_123".to_string(),
        };

        let encoded = DeleteMessageCodec::encode(data.clone()).unwrap();
        assert_eq!(encoded.clone().r#type.unwrap().type_id, "deleteMessage");

        let decoded = DeleteMessageCodec::decode(encoded).unwrap();
        assert_eq!(decoded.message_id, data.message_id);
    }
}
