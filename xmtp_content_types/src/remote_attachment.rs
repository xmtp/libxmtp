use std::collections::HashMap;

use prost::Message;

use crate::{
    CodecError, ContentCodec,
    attachment::{Attachment, AttachmentCodec},
    encryption::{self, EncryptedPayload, SECRET_SIZE},
    utils::get_param_or_default,
};
use serde::{Deserialize, Serialize};

use xmtp_proto::xmtp::mls::message_contents::{ContentTypeId, EncodedContent};

pub struct RemoteAttachmentCodec {}

/// Result of encrypting an attachment for remote storage.
///
/// Contains the encrypted bytes to upload and all metadata needed to create a `RemoteAttachment`.
#[derive(Debug, Clone)]
pub struct EncryptedAttachment {
    /// The encrypted bytes to upload to the remote server
    pub payload: Vec<u8>,
    /// SHA-256 digest of the encrypted bytes (hex-encoded)
    pub content_digest: String,
    /// The 32-byte secret key needed for decryption
    pub secret: Vec<u8>,
    /// The 32-byte salt used in key derivation
    pub salt: Vec<u8>,
    /// The 12-byte nonce used in encryption
    pub nonce: Vec<u8>,
    /// The length of the encrypted content
    pub content_length: usize,
    /// The filename of the attachment
    pub filename: Option<String>,
}

/// Encrypts an attachment for storage as a remote attachment.
pub fn encrypt_attachment(attachment: Attachment) -> Result<EncryptedAttachment, CodecError> {
    let filename = attachment.filename.clone();

    // Encode the Attachment to EncodedContent
    let encoded_content = AttachmentCodec::encode(attachment)?;

    // Serialize EncodedContent to bytes
    let mut encoded_bytes = Vec::new();
    encoded_content
        .encode(&mut encoded_bytes)
        .map_err(|e| CodecError::Encode(format!("failed to encode attachment: {e}")))?;

    // Generate a random 32-byte secret
    let secret: [u8; SECRET_SIZE] = xmtp_common::rand_array();

    // Encrypt the encoded content
    let encrypted_payload = encryption::encrypt(&encoded_bytes, &secret)?;

    // Compute SHA-256 digest of the encrypted payload
    let digest = encryption::sha256(&encrypted_payload.payload);
    let content_digest = hex::encode(digest);

    Ok(EncryptedAttachment {
        content_length: encrypted_payload.payload.len(),
        payload: encrypted_payload.payload,
        content_digest,
        secret: secret.to_vec(),
        salt: encrypted_payload.salt,
        nonce: encrypted_payload.nonce,
        filename,
    })
}

/// Decrypts an attachment that was encrypted with [`encrypt_attachment`].
pub fn decrypt_attachment(
    encrypted_bytes: &[u8],
    remote_attachment: &RemoteAttachment,
) -> Result<Attachment, CodecError> {
    // Verify content digest
    let actual_digest = hex::encode(encryption::sha256(encrypted_bytes));
    if actual_digest != remote_attachment.content_digest {
        return Err(CodecError::Decode(format!(
            "content digest mismatch: expected {}, got {}",
            remote_attachment.content_digest, actual_digest
        )));
    }

    // Reconstruct the encrypted payload
    let encrypted_payload = EncryptedPayload {
        payload: encrypted_bytes.to_vec(),
        salt: remote_attachment.salt.clone(),
        nonce: remote_attachment.nonce.clone(),
    };

    // Decrypt
    let decrypted_bytes = encryption::decrypt(&encrypted_payload, &remote_attachment.secret)?;

    // Decode the EncodedContent
    let encoded_content = EncodedContent::decode(decrypted_bytes.as_slice())
        .map_err(|e| CodecError::Decode(format!("failed to decode EncodedContent: {e}")))?;

    // Decode the Attachment
    AttachmentCodec::decode(encoded_content)
}

/// Legacy content type id at <https://github.com/xmtp/xmtp-js/blob/main/content-types/content-type-remote-attachment/src/RemoteAttachment.ts>
impl RemoteAttachmentCodec {
    const AUTHORITY_ID: &'static str = "xmtp.org";
    pub const TYPE_ID: &'static str = "remoteStaticAttachment";
    pub const MAJOR_VERSION: u32 = 1;
    pub const MINOR_VERSION: u32 = 0;
}

impl RemoteAttachmentCodec {
    fn fallback(content: &RemoteAttachment) -> Option<String> {
        Some(format!(
            "Can't display {}. This app doesn't support remote attachments.",
            content
                .filename
                .clone()
                .unwrap_or("this content".to_string())
        ))
    }
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
        let fallback = Self::fallback(&data);
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
            fallback,
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

    fn should_push() -> bool {
        true
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

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_encrypt_decrypt_attachment_roundtrip() {
        let original_content = b"This is a test attachment content";
        let filename = Some("test.txt".to_string());
        let mime_type = "text/plain".to_string();

        let attachment = Attachment {
            filename: filename.clone(),
            mime_type: mime_type.clone(),
            content: original_content.to_vec(),
        };

        // Encrypt the attachment
        let encrypted = encrypt_attachment(attachment).unwrap();

        // Verify filename is preserved
        assert_eq!(encrypted.filename, filename);

        // Create a RemoteAttachment with the encryption metadata
        let remote_attachment = RemoteAttachment {
            filename: encrypted.filename.clone(),
            url: "https://example.com/file.txt".to_string(),
            content_digest: encrypted.content_digest,
            secret: encrypted.secret,
            salt: encrypted.salt,
            nonce: encrypted.nonce,
            scheme: "https".to_string(),
            content_length: encrypted.content_length,
        };

        // Decrypt the attachment
        let decrypted = decrypt_attachment(&encrypted.payload, &remote_attachment).unwrap();

        assert_eq!(original_content.as_slice(), decrypted.content.as_slice());
        assert_eq!(decrypted.filename, filename);
        assert_eq!(decrypted.mime_type, mime_type);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_decrypt_with_wrong_digest_fails() {
        let attachment = Attachment {
            filename: None,
            mime_type: "application/octet-stream".to_string(),
            content: b"Test content".to_vec(),
        };
        let encrypted = encrypt_attachment(attachment).unwrap();

        let remote_attachment = RemoteAttachment {
            filename: None,
            url: "https://example.com/file".to_string(),
            content_digest: "wrong_digest".to_string(), // Wrong digest
            secret: encrypted.secret,
            salt: encrypted.salt,
            nonce: encrypted.nonce,
            scheme: "https".to_string(),
            content_length: encrypted.content_length,
        };

        let result = decrypt_attachment(&encrypted.payload, &remote_attachment);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("content digest mismatch")
        );
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_decrypt_with_wrong_secret_fails() {
        let attachment = Attachment {
            filename: None,
            mime_type: "application/octet-stream".to_string(),
            content: b"Test content".to_vec(),
        };
        let encrypted = encrypt_attachment(attachment).unwrap();

        let wrong_secret: [u8; SECRET_SIZE] = xmtp_common::rand_array();
        let remote_attachment = RemoteAttachment {
            filename: None,
            url: "https://example.com/file".to_string(),
            content_digest: encrypted.content_digest,
            secret: wrong_secret.to_vec(), // Wrong secret
            salt: encrypted.salt,
            nonce: encrypted.nonce,
            scheme: "https".to_string(),
            content_length: encrypted.content_length,
        };

        let result = decrypt_attachment(&encrypted.payload, &remote_attachment);
        assert!(result.is_err());
    }
}
