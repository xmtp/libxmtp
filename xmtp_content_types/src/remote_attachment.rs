use std::collections::HashMap;

use crate::{CodecError, ContentCodec, utils::get_param_or_default};
use serde::{Deserialize, Serialize};

use xmtp_proto::xmtp::mls::message_contents::{ContentTypeId, EncodedContent};

pub struct RemoteAttachmentCodec {}

/// Legacy content type id at https://github.com/xmtp/xmtp-js/blob/main/content-types/content-type-remote-attachment/src/RemoteAttachment.ts
impl RemoteAttachmentCodec {
    const AUTHORITY_ID: &'static str = "xmtp.org";
    pub const TYPE_ID: &'static str = "remoteStaticAttachment";
    pub const MAJOR_VERSION: u32 = 1;
    pub const MINOR_VERSION: u32 = 0;
}

impl ContentCodec<RemoteAttachment> for RemoteAttachmentCodec {
    fn content_type() -> ContentTypeId {
        ContentTypeId {
            authority_id: Self::AUTHORITY_ID.to_string(),
            type_id: Self::TYPE_ID.to_string(),
            version_major: RemoteAttachmentCodec::MAJOR_VERSION,
            version_minor: RemoteAttachmentCodec::MINOR_VERSION,
        }
    }

    fn encode(data: RemoteAttachment) -> Result<EncodedContent, CodecError> {
        let mut parameters = [
            ("contentDigest", data.content_digest),
            ("salt", hex::encode(data.salt)),
            ("nonce", hex::encode(data.nonce)),
            ("secret", hex::encode(data.secret)),
            ("scheme", data.scheme),
            ("contentLength", data.content_length.to_string()),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect::<HashMap<_, _>>();

        if let Some(filename) = data.filename {
            parameters.insert("filename".to_string(), filename);
        }

        Ok(EncodedContent {
            r#type: Some(Self::content_type()),
            parameters,
            fallback: None,
            compression: None,
            content: data.url.into_bytes(),
        })
    }

    fn decode(encoded: EncodedContent) -> Result<RemoteAttachment, CodecError> {
        // Extract parameters
        let parameters: &HashMap<String, String> = &encoded.parameters;

        let content_digest = get_param_or_default(parameters, "contentDigest").to_string();
        let salt = hex::decode(get_param_or_default(parameters, "salt")).unwrap_or_else(|_| vec![]);
        let nonce =
            hex::decode(get_param_or_default(parameters, "nonce")).unwrap_or_else(|_| vec![]);
        let secret =
            hex::decode(get_param_or_default(parameters, "secret")).unwrap_or_else(|_| vec![]);
        let scheme = get_param_or_default(parameters, "scheme").to_string();
        let content_length = get_param_or_default(parameters, "contentLength")
            .parse()
            .unwrap_or(0);

        let filename = parameters.get("filename").cloned();

        let url =
            String::from_utf8(encoded.content).map_err(|e| CodecError::Decode(e.to_string()))?;

        Ok(RemoteAttachment {
            filename,
            url,
            content_digest,
            secret,
            nonce,
            salt,
            scheme,
            content_length,
        })
    }
}

/// The main content type for remote attachments
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RemoteAttachment {
    /// The filename of the remote attachment
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,

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

    /// The scheme used to fetch the file
    pub scheme: String,

    pub content_length: usize,
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
            content_length: 1024,
            url: "https://example.com/file.pdf".to_string(),
            content_digest: "abc123".to_string(),
            secret: vec![1, 2, 3, 4],
            nonce: vec![5, 6, 7, 8],
            salt: vec![9, 10, 11, 12],
            scheme: "https".to_string(),
        };

        let encoded = RemoteAttachmentCodec::encode(remote_attachment.clone()).unwrap();
        let decoded = RemoteAttachmentCodec::decode(encoded).unwrap();

        assert_eq!(decoded.filename, remote_attachment.filename);
        assert_eq!(decoded.scheme, remote_attachment.scheme);
        assert_eq!(decoded.content_length, remote_attachment.content_length);
        assert_eq!(decoded.url, remote_attachment.url);
        assert_eq!(decoded.content_digest, remote_attachment.content_digest);
        assert_eq!(decoded.secret, remote_attachment.secret);
        assert_eq!(decoded.nonce, remote_attachment.nonce);
        assert_eq!(decoded.salt, remote_attachment.salt);
    }
}
