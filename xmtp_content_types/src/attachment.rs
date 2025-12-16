use std::collections::HashMap;

use crate::{CodecError, ContentCodec, utils::get_param_or_default};
use serde::{Deserialize, Serialize};

use xmtp_proto::xmtp::mls::message_contents::{ContentTypeId, EncodedContent};

pub struct AttachmentCodec {}

/// Legacy content type id at <https://github.com/xmtp/xmtp-js/blob/main/content-types/content-type-remote-attachment/src/Attachment.ts>
impl AttachmentCodec {
    const AUTHORITY_ID: &'static str = "xmtp.org";
    pub const TYPE_ID: &'static str = "attachment";
    pub const MAJOR_VERSION: u32 = 1;
    pub const MINOR_VERSION: u32 = 0;
}

impl AttachmentCodec {
    fn fallback(content: &Attachment) -> Option<String> {
        Some(format!(
            "Can't display {}. This app doesn't support attachments.",
            content
                .filename
                .clone()
                .unwrap_or("this content".to_string())
        ))
    }
}

impl ContentCodec<Attachment> for AttachmentCodec {
    fn content_type() -> ContentTypeId {
        ContentTypeId {
            authority_id: Self::AUTHORITY_ID.to_string(),
            type_id: Self::TYPE_ID.to_string(),
            version_major: AttachmentCodec::MAJOR_VERSION,
            version_minor: AttachmentCodec::MINOR_VERSION,
        }
    }

    fn encode(data: Attachment) -> Result<EncodedContent, CodecError> {
        let fallback = Self::fallback(&data);
        let mut parameters = [("mimeType", data.mime_type)]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect::<HashMap<_, _>>();

        if let Some(filename) = data.filename {
            parameters.insert("filename".to_string(), filename);
        }

        Ok(EncodedContent {
            r#type: Some(Self::content_type()),
            parameters,
            fallback,
            compression: None,
            content: data.content,
        })
    }

    fn decode(encoded: EncodedContent) -> Result<Attachment, CodecError> {
        let parameters: &HashMap<String, String> = &encoded.parameters;

        Ok(Attachment {
            filename: parameters.get("filename").map(|f| f.to_string()),
            mime_type: get_param_or_default(parameters, "mimeType").to_string(),
            content: encoded.content,
        })
    }

    fn should_push() -> bool {
        true
    }
}

/// The main content type for attachments
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Attachment {
    /// The filename of the attachment
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,

    /// The MIME type of the attachment
    pub mime_type: String,

    /// The content of the attachment (base64 encoded)
    pub content: Vec<u8>,
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);
    use super::*;

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_encode_decode_attachment() {
        let attachment = Attachment {
            filename: Some("test.txt".to_string()),
            mime_type: "text/plain".to_string(),
            content: vec![1, 2, 3, 4],
        };

        let encoded = AttachmentCodec::encode(attachment.clone()).unwrap();
        let decoded = AttachmentCodec::decode(encoded).unwrap();

        assert_eq!(decoded.filename, attachment.filename);
        assert_eq!(decoded.mime_type, attachment.mime_type);
        assert_eq!(decoded.content, attachment.content);
    }
}
