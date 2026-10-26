use std::collections::HashMap;

use crate::{
    CodecError, ContentCodec,
    attachment::{Attachment, AttachmentCodec},
    encryption::{self, EncryptionKeys},
};

use xmtp_proto::xmtp::mls::message_contents::{
    ContentTypeId, EncodedContent, content_types::RemoteAttachmentInfo,
};

pub struct RemoteAttachmentCodec {}

/// Check a content URL without DNS or transfer policy.
pub fn content_url_scheme(raw: &str) -> Result<&'static str, &'static str> {
    let url = url::Url::parse(raw).map_err(|_| "attachment URL is invalid")?;
    let host = url.host().ok_or("attachment URL has no host")?;
    let loopback = match host {
        url::Host::Domain(name) => is_loopback_name(name),
        url::Host::Ipv4(address) => address.is_loopback(),
        url::Host::Ipv6(address) => address.is_loopback(),
    };
    match url.scheme() {
        "https" => Ok("https://"),
        "http" if loopback => Ok("http://"),
        _ => Err("attachment URL requires HTTPS or loopback HTTP"),
    }
}

pub fn is_loopback_name(name: &str) -> bool {
    name.strip_suffix('.')
        .unwrap_or(name)
        .eq_ignore_ascii_case("localhost")
}

// implements: CTYPE-014
pub(crate) fn normalize_decoded_scheme(value: &mut RemoteAttachment) {
    if matches!(value.scheme.as_str(), "https" | "http") {
        value.scheme.push_str("://");
    }
}

// implements: CTYPE-014
pub(crate) fn validate_remote_attachment(value: &RemoteAttachment) -> Result<(), &'static str> {
    use crate::encryption::{AES_GCM_NONCE_SIZE, HKDF_SALT_SIZE, SECRET_SIZE};
    const SHA256_HEX_LENGTH: usize = 64;
    if value.content_digest.len() != SHA256_HEX_LENGTH
        || !value
            .content_digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("contentDigest requires 64 lowercase hexadecimal digits");
    }
    if value.secret.len() != SECRET_SIZE {
        return Err("secret requires 32 bytes");
    }
    if value.salt.len() != HKDF_SALT_SIZE {
        return Err("salt requires 32 bytes");
    }
    if value.nonce.len() != AES_GCM_NONCE_SIZE {
        return Err("nonce requires 12 bytes");
    }
    if value.scheme != content_url_scheme(&value.url)? {
        return Err("scheme does not match the permitted URL scheme");
    }
    Ok(())
}

/// Result of encrypting an attachment for remote storage.
///
/// Contains the encrypted bytes to upload and all metadata needed to create a `RemoteAttachment`.
#[derive(Clone)]
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
    pub content_length: u32,
    /// The filename of the attachment
    pub filename: Option<String>,
}

impl std::fmt::Debug for EncryptedAttachment {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EncryptedAttachment")
            .field("payload", &self.payload)
            .field("content_digest", &self.content_digest)
            .field("secret", &"<redacted>")
            .field("salt", &self.salt)
            .field("nonce", &self.nonce)
            .field("content_length", &self.content_length)
            .field("filename", &self.filename)
            .finish()
    }
}

/// Encrypts an attachment for storage as a remote attachment.
// implements: CTYPE-015
pub fn encrypt_attachment(attachment: Attachment) -> Result<EncryptedAttachment, CodecError> {
    let filename = attachment.filename.clone();

    // Encode the Attachment to EncodedContent
    let encoded_content = AttachmentCodec::encode(attachment)?;

    let encrypted = encryption::encrypt_encoded_content(encoded_content)?;
    let content_length = u32::try_from(encrypted.keys.length).map_err(|_| {
        CodecError::Encode(format!(
            "attachment size {} exceeds maximum of {} bytes",
            encrypted.keys.length,
            u32::MAX
        ))
    })?;

    Ok(EncryptedAttachment {
        content_length,
        payload: encrypted.ciphertext,
        content_digest: encrypted.keys.digest,
        secret: encrypted.keys.secret,
        salt: encrypted.keys.salt,
        nonce: encrypted.keys.nonce,
        filename,
    })
}

/// Decrypts an attachment that was encrypted with [`encrypt_attachment`].
// implements: CTYPE-015
pub fn decrypt_attachment(
    encrypted_bytes: &[u8],
    remote_attachment: &RemoteAttachment,
) -> Result<Attachment, CodecError> {
    let keys = EncryptionKeys {
        secret: remote_attachment.secret.clone(),
        salt: remote_attachment.salt.clone(),
        nonce: remote_attachment.nonce.clone(),
        digest: remote_attachment.content_digest.clone(),
        length: remote_attachment
            .content_length
            .map_or(encrypted_bytes.len() as u64, u64::from),
    };
    let encoded_content = encryption::decrypt_encoded_content(encrypted_bytes, &keys)?;

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
        validate_remote_attachment(&data).map_err(|reason| CodecError::Encode(reason.into()))?;
        let fallback = Self::fallback(&data);
        let mut parameters = [
            ("contentDigest", data.content_digest),
            ("salt", hex::encode(data.salt)),
            ("nonce", hex::encode(data.nonce)),
            ("secret", hex::encode(data.secret)),
            ("scheme", data.scheme),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect::<HashMap<_, _>>();

        if let Some(content_length) = data.content_length {
            parameters.insert("contentLength".to_string(), content_length.to_string());
        }

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
        let required = |name| {
            parameters
                .get(name)
                .ok_or_else(|| CodecError::Decode(format!("missing {name} parameter")))
        };

        let content_digest = required("contentDigest")?.clone();
        let salt = hex::decode(required("salt")?)
            .map_err(|e| CodecError::Decode(format!("invalid hex in salt parameter: {e}")))?;
        let nonce = hex::decode(required("nonce")?)
            .map_err(|e| CodecError::Decode(format!("invalid hex in nonce parameter: {e}")))?;
        let secret = hex::decode(required("secret")?)
            .map_err(|e| CodecError::Decode(format!("invalid hex in secret parameter: {e}")))?;
        let scheme = required("scheme")?.clone();
        for name in ["secret", "salt", "nonce"] {
            if !required(name)?
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            {
                return Err(CodecError::Decode(format!(
                    "{name} requires lowercase hexadecimal digits"
                )));
            }
        }
        let content_length = parameters
            .get("contentLength")
            .map(|s| {
                if s.is_empty() || !s.bytes().all(|byte| byte.is_ascii_digit()) {
                    return Err(CodecError::Decode(
                        "contentLength requires decimal digits".into(),
                    ));
                }
                s.parse().map_err(|e| {
                    CodecError::Decode(format!("invalid contentLength parameter: {e}"))
                })
            })
            .transpose()?;

        let filename = parameters.get("filename").cloned();

        let url =
            String::from_utf8(encoded.content).map_err(|e| CodecError::Decode(e.to_string()))?;

        let mut attachment = RemoteAttachment {
            filename,
            url,
            content_digest,
            secret,
            nonce,
            salt,
            scheme,
            content_length,
        };
        normalize_decoded_scheme(&mut attachment);
        validate_remote_attachment(&attachment)
            .map_err(|reason| CodecError::Decode(reason.into()))?;
        Ok(attachment)
    }

    fn should_push() -> bool {
        true
    }
}

/// The main content type for remote attachments.
///
/// This is a type alias for [`RemoteAttachmentInfo`] from the proto definitions,
/// allowing the same type to be used for both single remote attachments and
/// entries in [`MultiRemoteAttachment`].
pub type RemoteAttachment = RemoteAttachmentInfo;

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::encryption::SECRET_SIZE;

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    // verifies: CTYPE-007
    fn test_encode_decode_remote_attachment() {
        let remote_attachment = RemoteAttachment {
            filename: Some("test.pdf".to_string()),
            content_length: Some(1024),
            url: "https://example.com/file.pdf".to_string(),
            content_digest: "ab".repeat(32),
            secret: vec![1; 32],
            nonce: vec![2; 12],
            salt: vec![3; 32],
            scheme: "https://".to_string(),
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
    // verifies: CTYPE-015
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
            scheme: "https://".to_string(),
            content_length: Some(encrypted.content_length),
        };

        // Decrypt the attachment
        let decrypted = decrypt_attachment(&encrypted.payload, &remote_attachment).unwrap();

        assert_eq!(original_content.as_slice(), decrypted.content.as_slice());
        assert_eq!(decrypted.filename, filename);
        assert_eq!(decrypted.mime_type, mime_type);
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn remote_attachment_decryption_requires_advertised_length() {
        const TEST_CONTENT_BYTES: usize = 2 * 1024 * 1024;
        let content = vec![0x5a; TEST_CONTENT_BYTES];
        let encrypted = encrypt_attachment(Attachment {
            filename: None,
            mime_type: "application/octet-stream".into(),
            content: content.clone(),
        })?;
        assert!(encrypted.payload.len() > TEST_CONTENT_BYTES);

        for advertised_length in [Some(1), None, Some(encrypted.content_length)] {
            let metadata = RemoteAttachment {
                filename: None,
                url: "https://example.com/large-attachment".into(),
                content_digest: encrypted.content_digest.clone(),
                secret: encrypted.secret.clone(),
                salt: encrypted.salt.clone(),
                nonce: encrypted.nonce.clone(),
                scheme: "https://".into(),
                content_length: advertised_length,
            };
            let decoded = RemoteAttachmentCodec::decode(RemoteAttachmentCodec::encode(metadata)?)?;
            assert_eq!(decoded.content_length, advertised_length);
            if advertised_length == Some(1) {
                assert!(matches!(
                    decrypt_attachment(&encrypted.payload, &decoded),
                    Err(CodecError::Decode(message))
                        if message == "encrypted content length mismatch"
                ));
            } else {
                assert_eq!(
                    decrypt_attachment(&encrypted.payload, &decoded)?.content,
                    content
                );
            }
        }
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    // verifies: CTYPE-015
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
            scheme: "https://".to_string(),
            content_length: Some(encrypted.content_length),
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
    // verifies: CTYPE-015
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
            scheme: "https://".to_string(),
            content_length: Some(encrypted.content_length),
        };

        let result = decrypt_attachment(&encrypted.payload, &remote_attachment);
        assert!(result.is_err());
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    // verifies: CTYPE-014
    fn test_decode_with_invalid_salt_hex() {
        let encoded = EncodedContent {
            r#type: Some(RemoteAttachmentCodec::content_type()),
            parameters: [
                (
                    "contentDigest",
                    "abababababababababababababababababababababababababababababababab",
                ),
                ("salt", "not_valid_hex"),
                ("nonce", "030303030303030303030303"),
                (
                    "secret",
                    "0101010101010101010101010101010101010101010101010101010101010101",
                ),
                ("scheme", "https://"),
                ("contentLength", "100"),
            ]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
            fallback: None,
            compression: None,
            content: b"https://example.com/file".to_vec(),
        };

        let result = RemoteAttachmentCodec::decode(encoded);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("invalid hex in salt")
        );
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_decode_with_invalid_nonce_hex() {
        let encoded = EncodedContent {
            r#type: Some(RemoteAttachmentCodec::content_type()),
            parameters: [
                (
                    "contentDigest",
                    "abababababababababababababababababababababababababababababababab",
                ),
                (
                    "salt",
                    "0202020202020202020202020202020202020202020202020202020202020202",
                ),
                ("nonce", "not_valid_hex"),
                (
                    "secret",
                    "0101010101010101010101010101010101010101010101010101010101010101",
                ),
                ("scheme", "https://"),
                ("contentLength", "100"),
            ]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
            fallback: None,
            compression: None,
            content: b"https://example.com/file".to_vec(),
        };

        let result = RemoteAttachmentCodec::decode(encoded);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("invalid hex in nonce")
        );
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_decode_with_invalid_secret_hex() {
        let encoded = EncodedContent {
            r#type: Some(RemoteAttachmentCodec::content_type()),
            parameters: [
                (
                    "contentDigest",
                    "abababababababababababababababababababababababababababababababab",
                ),
                (
                    "salt",
                    "0202020202020202020202020202020202020202020202020202020202020202",
                ),
                ("nonce", "030303030303030303030303"),
                ("secret", "not_valid_hex"),
                ("scheme", "https://"),
                ("contentLength", "100"),
            ]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
            fallback: None,
            compression: None,
            content: b"https://example.com/file".to_vec(),
        };

        let result = RemoteAttachmentCodec::decode(encoded);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("invalid hex in secret")
        );
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    // verifies: CTYPE-014
    fn test_decode_with_invalid_content_length() {
        let encoded = EncodedContent {
            r#type: Some(RemoteAttachmentCodec::content_type()),
            parameters: [
                (
                    "contentDigest",
                    "abababababababababababababababababababababababababababababababab",
                ),
                (
                    "salt",
                    "0202020202020202020202020202020202020202020202020202020202020202",
                ),
                ("nonce", "030303030303030303030303"),
                (
                    "secret",
                    "0101010101010101010101010101010101010101010101010101010101010101",
                ),
                ("scheme", "https://"),
                ("contentLength", "not_a_number"),
            ]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
            fallback: None,
            compression: None,
            content: b"https://example.com/file".to_vec(),
        };

        let result = RemoteAttachmentCodec::decode(encoded);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("contentLength"));
    }

    // verifies: CTYPE-014
    #[xmtp_common::test(unwrap_try = true)]
    fn catalogue_values_are_checked_on_single_and_multi_encode_and_decode() {
        use crate::multi_remote_attachment::MultiRemoteAttachmentCodec;
        use prost::Message;
        use xmtp_proto::xmtp::mls::message_contents::content_types::MultiRemoteAttachment;
        let sample = || RemoteAttachment {
            url: "https://example.com/file?token=value".into(),
            content_digest: "ab".repeat(32),
            secret: vec![1; 32],
            salt: vec![2; 32],
            nonce: vec![3; 12],
            scheme: "https://".into(),
            content_length: Some(0),
            filename: None,
        };
        let raw = |value: &RemoteAttachment| EncodedContent {
            r#type: Some(RemoteAttachmentCodec::content_type()),
            parameters: HashMap::from([
                ("contentDigest".into(), value.content_digest.clone()),
                ("secret".into(), hex::encode(&value.secret)),
                ("salt".into(), hex::encode(&value.salt)),
                ("nonce".into(), hex::encode(&value.nonce)),
                ("scheme".into(), value.scheme.clone()),
                (
                    "contentLength".into(),
                    value.content_length.unwrap_or(0).to_string(),
                ),
            ]),
            content: value.url.as_bytes().to_vec(),
            ..Default::default()
        };
        for url in [
            "https://example.com/file?token=value",
            "http://localhost/file",
            "http://127.0.0.1/file",
            "http://[::1]/file",
        ] {
            for length in [Some(0), Some(u32::MAX), None] {
                let mut value = sample();
                value.url = url.into();
                value.scheme = if url.starts_with("https") {
                    "https://"
                } else {
                    "http://"
                }
                .into();
                value.content_length = length;
                let mut encoded = RemoteAttachmentCodec::encode(value.clone())?;
                encoded
                    .parameters
                    .insert("ignored".into(), "invalid".into());
                assert_eq!(RemoteAttachmentCodec::decode(encoded)?, value);
                let multi = MultiRemoteAttachment {
                    attachments: vec![value],
                };
                assert_eq!(
                    MultiRemoteAttachmentCodec::decode(MultiRemoteAttachmentCodec::encode(
                        multi.clone()
                    )?)?,
                    multi
                );
            }
        }
        let mut invalid = Vec::new();
        for digest in [
            String::new(),
            "ab".repeat(31),
            "ab".repeat(33),
            "AB".repeat(32),
            "gg".repeat(32),
        ] {
            let mut value = sample();
            value.content_digest = digest;
            invalid.push(value);
        }
        for (field, size) in [("secret", 32), ("salt", 32), ("nonce", 12)] {
            for length in [0, size - 1, size + 1] {
                let mut value = sample();
                match field {
                    "secret" => value.secret = vec![1; length],
                    "salt" => value.salt = vec![2; length],
                    _ => value.nonce = vec![3; length],
                }
                invalid.push(value);
            }
        }
        for (url, scheme) in [
            ("not a URL", "https://"),
            ("http://example.com/file", "http://"),
            ("ftp://localhost/file", "ftp://"),
            ("https://example.com/file", "http://"),
        ] {
            let mut value = sample();
            value.url = url.into();
            value.scheme = scheme.into();
            invalid.push(value);
        }
        for value in invalid {
            assert!(matches!(
                RemoteAttachmentCodec::encode(value.clone()),
                Err(CodecError::Encode(_))
            ));
            assert!(matches!(
                RemoteAttachmentCodec::decode(raw(&value)),
                Err(CodecError::Decode(_))
            ));
            let multi = MultiRemoteAttachment {
                attachments: vec![sample(), value],
            };
            assert!(matches!(
                MultiRemoteAttachmentCodec::encode(multi.clone()),
                Err(CodecError::Encode(_))
            ));
            assert!(matches!(
                MultiRemoteAttachmentCodec::decode(EncodedContent {
                    r#type: Some(MultiRemoteAttachmentCodec::content_type()),
                    content: multi.encode_to_vec(),
                    ..Default::default()
                }),
                Err(CodecError::Decode(_))
            ));
        }
        for (field, text) in [
            ("secret", "AB".repeat(32)),
            ("salt", "AB".repeat(32)),
            ("nonce", "AB".repeat(12)),
            ("contentLength", "-1".into()),
            ("contentLength", "+1".into()),
            ("contentLength", "4294967296".into()),
            ("contentLength", "1.0".into()),
            ("contentLength", "".into()),
        ] {
            let mut encoded = raw(&sample());
            encoded.parameters.insert(field.into(), text);
            assert!(
                matches!(
                    RemoteAttachmentCodec::decode(encoded),
                    Err(CodecError::Decode(_))
                ),
                "{field}"
            );
        }
    }

    // verifies: CTYPE-014
    #[xmtp_common::test(unwrap_try = true)]
    fn legacy_scheme_reads_normalize_without_changing_the_wire() {
        use crate::multi_remote_attachment::MultiRemoteAttachmentCodec;
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        use prost::Message;
        use xmtp_proto::xmtp::mls::message_contents::content_types::MultiRemoteAttachment;
        // Original wire fixture from commit 334dd7f45b915da51dfc4e7a2c66c01145241442.
        let original = STANDARD.decode("CiQKCHhtdHAub3JnEhZyZW1vdGVTdGF0aWNBdHRhY2htZW50GAESUQoNY29udGVudERpZ2VzdBJAYWJjZDEyMzQ1Njc4OTBhYmNkMTIzNDU2Nzg5MGFiY2QxMjM0NTY3ODkwYWJjZDEyMzQ1Njc4OTBhYmNkMTIzNBJICgRzYWx0EkAwMjAyMDIwMjAyMDIwMjAyMDIwMjAyMDIwMjAyMDIwMjAyMDIwMjAyMDIwMjAyMDIwMjAyMDIwMjAyMDIwMjAyEiEKBW5vbmNlEhgwMzAzMDMwMzAzMDMwMzAzMDMwMzAzMDMSSgoGc2VjcmV0EkAwMTAxMDEwMTAxMDEwMTAxMDEwMTAxMDEwMTAxMDEwMTAxMDEwMTAxMDEwMTAxMDEwMTAxMDEwMTAxMDEwMTAxEg8KBnNjaGVtZRIFaHR0cHMSFQoNY29udGVudExlbmd0aBIEMTAyNBIKCghmaWxlbmFtZSIcaHR0cHM6Ly9leGFtcGxlLmNvbS9maWxlLnBkZg==")?;
        let legacy = EncodedContent::decode(original.as_slice())?;
        assert_eq!(legacy.parameters["scheme"], "https");
        let decoded = RemoteAttachmentCodec::decode(legacy.clone())?;
        assert_eq!(decoded.scheme, "https://");
        assert_eq!(decoded.url, "https://example.com/file.pdf");
        assert_eq!(legacy.parameters["scheme"], "https");
        assert_eq!(
            RemoteAttachmentCodec::encode(decoded)?.parameters["scheme"],
            "https://"
        );
        for (url, bare, canonical) in [
            ("https://example.com/file", "https", "https://"),
            ("http://localhost/file", "http", "http://"),
            ("http://127.0.0.1/file", "http", "http://"),
            ("http://[::1]/file", "http", "http://"),
        ] {
            let value = RemoteAttachment {
                url: url.into(),
                content_digest: "ab".repeat(32),
                secret: vec![1; 32],
                salt: vec![2; 32],
                nonce: vec![3; 12],
                scheme: canonical.into(),
                ..Default::default()
            };
            let mut legacy = RemoteAttachmentCodec::encode(value.clone())?;
            legacy.parameters.insert("scheme".into(), bare.into());
            let original = legacy.encode_to_vec();
            let decoded = RemoteAttachmentCodec::decode(legacy.clone())?;
            assert_eq!(decoded, value);
            assert_eq!(legacy.encode_to_vec(), original);
            assert_eq!(legacy.parameters["scheme"], bare);
            assert_eq!(
                RemoteAttachmentCodec::encode(decoded)?.parameters["scheme"],
                canonical
            );
            let mut legacy_value = value.clone();
            legacy_value.scheme = bare.into();
            assert!(matches!(
                RemoteAttachmentCodec::encode(legacy_value.clone()),
                Err(CodecError::Encode(_))
            ));
            let multi = MultiRemoteAttachment {
                attachments: vec![legacy_value],
            };
            assert!(matches!(
                MultiRemoteAttachmentCodec::encode(multi.clone()),
                Err(CodecError::Encode(_))
            ));
            let outer = EncodedContent {
                r#type: Some(MultiRemoteAttachmentCodec::content_type()),
                content: multi.encode_to_vec(),
                ..Default::default()
            };
            let decoded = MultiRemoteAttachmentCodec::decode(outer.clone())?;
            assert_eq!(decoded.attachments[0], value);
            assert_eq!(outer.content, multi.encode_to_vec());
            assert_eq!(
                MultiRemoteAttachmentCodec::decode(MultiRemoteAttachmentCodec::encode(decoded)?)?
                    .attachments[0]
                    .scheme,
                canonical
            );
        }
        let value = RemoteAttachment {
            url: "http://example.com/file".into(),
            content_digest: "ab".repeat(32),
            secret: vec![1; 32],
            salt: vec![2; 32],
            nonce: vec![3; 12],
            scheme: "http".into(),
            ..Default::default()
        };
        let valid = RemoteAttachment {
            url: "https://example.com/file".into(),
            scheme: "https://".into(),
            ..value.clone()
        };
        let mut legacy = RemoteAttachmentCodec::encode(valid)?;
        legacy.content = value.url.as_bytes().to_vec();
        legacy.parameters.insert("scheme".into(), "http".into());
        assert!(matches!(
            RemoteAttachmentCodec::decode(legacy),
            Err(CodecError::Decode(_))
        ));
        let multi = MultiRemoteAttachment {
            attachments: vec![value],
        };
        assert!(matches!(
            MultiRemoteAttachmentCodec::decode(EncodedContent {
                r#type: Some(MultiRemoteAttachmentCodec::content_type()),
                content: multi.encode_to_vec(),
                ..Default::default()
            }),
            Err(CodecError::Decode(_))
        ));
    }
}
