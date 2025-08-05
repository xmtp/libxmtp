use std::collections::HashMap;

use crate::{CodecError, ContentCodec};
use serde::{Deserialize, Serialize};

use xmtp_proto::xmtp::mls::message_contents::{ContentTypeId, EncodedContent};

pub struct RemoteAttachmentCodec {}

/// Legacy content type id at https://github.com/xmtp/xmtp-js/blob/main/content-types/content-type-remote-attachment/src/RemoteAttachment.ts
impl RemoteAttachmentCodec {
    const AUTHORITY_ID: &'static str = "xmtp.org";
    pub const TYPE_ID: &'static str = "remoteStaticAttachment";
}

impl ContentCodec<RemoteAttachment> for RemoteAttachmentCodec {
    fn content_type() -> ContentTypeId {
        ContentTypeId {
            authority_id: Self::AUTHORITY_ID.to_string(),
            type_id: Self::TYPE_ID.to_string(),
            version_major: 1,
            version_minor: 0,
        }
    }

    fn encode(data: RemoteAttachment) -> Result<EncodedContent, CodecError> {
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

    fn decode(encoded: EncodedContent) -> Result<RemoteAttachment, CodecError> {
        serde_json::from_slice(&encoded.content)
            .map_err(|e| CodecError::Decode(format!("JSON decode error: {e}")))
    }
}

impl RemoteAttachmentCodec {
    fn fallback(content: &RemoteAttachment) -> String {
        if let Some(filename) = &content.filename {
            format!("[Remote attachment] {}", filename)
        } else {
            "[Remote attachment]".to_string()
        }
    }
}

/// The main content type for remote attachments
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RemoteAttachment {
    /// The filename of the remote attachment
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,

    /// The MIME type of the remote attachment
    pub mime_type: String,

    /// The size of the remote attachment in bytes
    pub size: u64,

    /// The URL where the remote attachment is stored
    pub url: String,

    /// The content digest (SHA256 hash) of the remote attachment
    pub content_digest: String,

    /// The secret key for decrypting the remote attachment
    pub secret: Vec<u8>,

    /// The nonce used for encryption
    pub nonce: Vec<u8>,

    /// The salt used for encryption
    pub salt: Vec<u8>,
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);
    use super::*;

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_encode_decode_remote_attachment() {
        let remote_attachment = RemoteAttachment {
            filename: Some("test.pdf".to_string()),
            mime_type: "application/pdf".to_string(),
            size: 1024,
            url: "https://example.com/file.pdf".to_string(),
            content_digest: "abc123".to_string(),
            secret: vec![1, 2, 3, 4],
            nonce: vec![5, 6, 7, 8],
            salt: vec![9, 10, 11, 12],
        };

        let encoded = RemoteAttachmentCodec::encode(remote_attachment.clone()).unwrap();
        let decoded = RemoteAttachmentCodec::decode(encoded).unwrap();

        assert_eq!(decoded.filename, remote_attachment.filename);
        assert_eq!(decoded.mime_type, remote_attachment.mime_type);
        assert_eq!(decoded.size, remote_attachment.size);
        assert_eq!(decoded.url, remote_attachment.url);
        assert_eq!(decoded.content_digest, remote_attachment.content_digest);
        assert_eq!(decoded.secret, remote_attachment.secret);
        assert_eq!(decoded.nonce, remote_attachment.nonce);
        assert_eq!(decoded.salt, remote_attachment.salt);
    }
}
