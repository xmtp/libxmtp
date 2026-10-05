use prost::Message as _;
use xmtp_content_types::encryption as core;
use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

use crate::{EncryptedEncodedContent, EncryptionKeys, XmtpError};

impl From<core::EncryptionKeys> for EncryptionKeys {
    fn from(value: core::EncryptionKeys) -> Self {
        Self {
            secret: value.secret,
            salt: value.salt,
            nonce: value.nonce,
            digest: value.digest,
            length: value.length,
        }
    }
}

impl From<EncryptionKeys> for core::EncryptionKeys {
    fn from(value: EncryptionKeys) -> Self {
        Self {
            secret: value.secret,
            salt: value.salt,
            nonce: value.nonce,
            digest: value.digest,
            length: value.length,
        }
    }
}

impl From<core::EncryptedEncodedContent> for EncryptedEncodedContent {
    fn from(value: core::EncryptedEncodedContent) -> Self {
        Self {
            ciphertext: value.ciphertext,
            keys: value.keys.into(),
        }
    }
}

#[xmtp_macro::sdk_export]
pub async fn encrypt_bytes(bytes: Vec<u8>) -> Result<EncryptedEncodedContent, XmtpError> {
    core::encrypt_bytes(&bytes)
        .map(Into::into)
        .map_err(XmtpError::from_core)
}

#[xmtp_macro::sdk_export]
pub async fn decrypt_bytes(
    ciphertext: Vec<u8>,
    keys: EncryptionKeys,
) -> Result<Vec<u8>, XmtpError> {
    core::decrypt_bytes(&ciphertext, &keys.into()).map_err(XmtpError::from_core)
}

#[xmtp_macro::sdk_export]
pub async fn encrypt_encoded_content(
    content: Vec<u8>,
) -> Result<EncryptedEncodedContent, XmtpError> {
    if content.is_empty() {
        return Err(XmtpError::invalid("encoded content is empty"));
    }
    let content = EncodedContent::decode(content.as_slice())
        .map_err(|_| XmtpError::invalid("invalid encoded content"))?;
    match content.r#type.as_ref() {
        None => return Err(XmtpError::invalid("encoded content has no content type")),
        Some(content_type)
            if content_type.authority_id.is_empty() || content_type.type_id.is_empty() =>
        {
            return Err(XmtpError::invalid(
                "encoded content type has an empty identifier",
            ));
        }
        Some(_) => {}
    }
    core::encrypt_encoded_content(content)
        .map(Into::into)
        .map_err(XmtpError::from_core)
}

#[xmtp_macro::sdk_export]
pub async fn decrypt_encoded_content(
    encrypted: EncryptedEncodedContent,
) -> Result<Vec<u8>, XmtpError> {
    core::decrypt_encoded_content(&encrypted.ciphertext, &encrypted.keys.into())
        .map(|content| content.encode_to_vec())
        .map_err(XmtpError::from_core)
}

#[cfg(not(target_arch = "wasm32"))]
#[xmtp_macro::sdk_export]
pub async fn encrypt_file(input: String, output: String) -> Result<EncryptionKeys, XmtpError> {
    tokio::task::spawn_blocking(move || {
        xmtp_content_types::file_encryption::encrypt_file(
            std::path::Path::new(&input),
            std::path::Path::new(&output),
        )
    })
    .await
    .map_err(XmtpError::from_core)?
    .map(Into::into)
    .map_err(XmtpError::from_core)
}

#[cfg(not(target_arch = "wasm32"))]
#[xmtp_macro::sdk_export]
pub async fn decrypt_file(
    input: String,
    output: String,
    keys: EncryptionKeys,
) -> Result<(), XmtpError> {
    tokio::task::spawn_blocking(move || {
        xmtp_content_types::file_encryption::decrypt_file(
            std::path::Path::new(&input),
            std::path::Path::new(&output),
            &keys.into(),
        )
    })
    .await
    .map_err(XmtpError::from_core)?
    .map_err(XmtpError::from_core)
}
