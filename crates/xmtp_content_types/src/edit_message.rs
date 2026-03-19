use std::collections::HashMap;

use prost::Message;

use super::{CodecError, ContentCodec};
use xmtp_proto::xmtp::mls::message_contents::content_types::EditMessage;
use xmtp_proto::xmtp::mls::message_contents::{ContentTypeId, EncodedContent};

pub struct EditMessageCodec;

impl EditMessageCodec {
    const AUTHORITY_ID: &'static str = "xmtp.org";
    pub const TYPE_ID: &'static str = "editMessage";
    pub const MAJOR_VERSION: u32 = 1;
    pub const MINOR_VERSION: u32 = 0;
}

impl ContentCodec<EditMessage> for EditMessageCodec {
    fn content_type() -> ContentTypeId {
        ContentTypeId {
            authority_id: EditMessageCodec::AUTHORITY_ID.to_string(),
            type_id: EditMessageCodec::TYPE_ID.to_string(),
            version_major: EditMessageCodec::MAJOR_VERSION,
            version_minor: EditMessageCodec::MINOR_VERSION,
        }
    }

    fn encode(data: EditMessage) -> Result<EncodedContent, CodecError> {
        let mut buf = Vec::new();
        data.encode(&mut buf)
            .map_err(|e| CodecError::Encode(e.to_string()))?;

        Ok(EncodedContent {
            r#type: Some(EditMessageCodec::content_type()),
            parameters: HashMap::new(),
            fallback: None,
            compression: None,
            content: buf,
        })
    }

    fn decode(content: EncodedContent) -> Result<EditMessage, CodecError> {
        let decoded = EditMessage::decode(content.content.as_slice())
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
        let edited_content = EncodedContent {
            r#type: Some(ContentTypeId {
                authority_id: "xmtp.org".to_string(),
                type_id: "text".to_string(),
                version_major: 1,
                version_minor: 0,
            }),
            parameters: HashMap::new(),
            fallback: None,
            compression: None,
            content: b"edited text content".to_vec(),
        };

        let data = EditMessage {
            message_id: "test_message_id_123".to_string(),
            edited_content: Some(edited_content.clone()),
        };

        let encoded = EditMessageCodec::encode(data.clone()).unwrap();
        assert_eq!(encoded.clone().r#type.unwrap().type_id, "editMessage");

        let decoded = EditMessageCodec::decode(encoded).unwrap();
        assert_eq!(decoded.message_id, data.message_id);
        assert!(decoded.edited_content.is_some());
    }
}
