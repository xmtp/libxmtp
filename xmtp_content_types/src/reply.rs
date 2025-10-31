use std::collections::HashMap;

use crate::{CodecError, ContentCodec, utils::get_param_or_default};
use prost::Message;
use serde::{Deserialize, Serialize};

use xmtp_proto::xmtp::mls::message_contents::{ContentTypeId, EncodedContent};

pub struct ReplyCodec;

/// Legacy content type id at <https://github.com/xmtp/xmtp-js/blob/main/content-types/content-type-reply/src/Reply.ts>
impl ReplyCodec {
    const AUTHORITY_ID: &str = "xmtp.org";
    pub const TYPE_ID: &str = "reply";
    pub const MAJOR_VERSION: u32 = 1;
    pub const MINOR_VERSION: u32 = 0;
}

impl ContentCodec<Reply> for ReplyCodec {
    fn content_type() -> ContentTypeId {
        ContentTypeId {
            authority_id: Self::AUTHORITY_ID.to_string(),
            type_id: Self::TYPE_ID.to_string(),
            version_major: Self::MAJOR_VERSION,
            version_minor: Self::MINOR_VERSION,
        }
    }

    fn encode(data: Reply) -> Result<EncodedContent, CodecError> {
        let inner_type = &data.content.r#type;
        // Set the reference and reference inbox ID as parameters.
        let mut parameters = HashMap::new();
        parameters.insert("reference".to_string(), data.reference);
        if let Some(content_type) = inner_type {
            parameters.insert(
                "contentType".to_string(),
                format!(
                    "{}/{}:{}.{}",
                    content_type.authority_id,
                    content_type.type_id,
                    content_type.version_major,
                    content_type.version_minor
                ),
            );
        }
        if let Some(reference_inbox_id) = data.reference_inbox_id {
            parameters.insert("referenceInboxId".to_string(), reference_inbox_id);
        }

        let content_bytes = data.content.encode_to_vec();

        Ok(EncodedContent {
            r#type: Some(Self::content_type()),
            parameters,
            content: content_bytes,
            ..Default::default()
        })
    }

    fn decode(encoded: EncodedContent) -> Result<Reply, CodecError> {
        let inner_content = EncodedContent::decode(encoded.content.as_slice())
            .map_err(|e| CodecError::Decode(e.to_string()))?;

        let reference = get_param_or_default(&encoded.parameters, "reference").to_string();

        let reference_inbox_id = encoded
            .parameters
            .get("referenceInboxId")
            .map(|id| id.to_string());

        Ok(Reply {
            reference,
            reference_inbox_id,
            content: inner_content,
        })
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
    pub content: EncodedContent,
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);
    use crate::text::TextCodec;

    use super::*;

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_encode_decode_reply() {
        let text_message = TextCodec::encode("This is a reply".to_string()).unwrap();
        let reply = Reply {
            reference: "msg_123".to_string(),
            reference_inbox_id: Some("inbox_456".to_string()),
            content: text_message,
        };

        let encoded = ReplyCodec::encode(reply.clone()).unwrap();
        let decoded = ReplyCodec::decode(encoded).unwrap();

        assert_eq!(decoded.reference, reply.reference);
        assert_eq!(decoded.reference_inbox_id, reply.reference_inbox_id);
        assert_eq!(decoded.content, reply.content);
    }
}
