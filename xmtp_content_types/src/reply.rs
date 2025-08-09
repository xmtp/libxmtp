use std::collections::HashMap;

use crate::{CodecError, ContentCodec};
use serde::{Deserialize, Serialize};

use xmtp_proto::xmtp::mls::message_contents::{ContentTypeId, EncodedContent};

pub struct ReplyCodec {}

/// Legacy content type id at https://github.com/xmtp/xmtp-js/blob/main/content-types/content-type-reply/src/Reply.ts
impl ReplyCodec {
    const AUTHORITY_ID: &'static str = "xmtp.org";
    pub const TYPE_ID: &'static str = "reply";
}

impl ContentCodec<Reply> for ReplyCodec {
    fn content_type() -> ContentTypeId {
        ContentTypeId {
            authority_id: Self::AUTHORITY_ID.to_string(),
            type_id: Self::TYPE_ID.to_string(),
            version_major: 1,
            version_minor: 0,
        }
    }

    fn encode(data: Reply) -> Result<EncodedContent, CodecError> {
        let json = serde_json::to_vec(&data)
            .map_err(|e| CodecError::Encode(format!("JSON encode error: {e}")))?;

        Ok(EncodedContent {
            r#type: Some(Self::content_type()),
            parameters: HashMap::new(),
            fallback: Some(Self::fallback(&data)),
            compression: None,
            content: json,
        })
    }

    fn decode(encoded: EncodedContent) -> Result<Reply, CodecError> {
        serde_json::from_slice(&encoded.content)
            .map_err(|e| CodecError::Decode(format!("JSON decode error: {e}")))
    }
}

impl ReplyCodec {
    fn fallback(content: &Reply) -> String {
        format!("[Reply to message] {}", content.content)
    }
}

/// The main content type for replies
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Reply {
    /// The message ID being replied to
    pub reference: String,

    /// The inbox ID of the user who sent the message being replied to
    #[serde(rename = "referenceInboxId", skip_serializing_if = "Option::is_none")]
    pub reference_inbox_id: Option<String>,

    /// The content of the reply
    pub content: String,
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);
    use super::*;

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_encode_decode_reply() {
        let reply = Reply {
            reference: "msg_123".to_string(),
            reference_inbox_id: Some("inbox_456".to_string()),
            content: "This is a reply".to_string(),
        };

        let encoded = ReplyCodec::encode(reply.clone()).unwrap();
        let decoded = ReplyCodec::decode(encoded).unwrap();

        assert_eq!(decoded.reference, reply.reference);
        assert_eq!(decoded.reference_inbox_id, reply.reference_inbox_id);
        assert_eq!(decoded.content, reply.content);
    }
}
