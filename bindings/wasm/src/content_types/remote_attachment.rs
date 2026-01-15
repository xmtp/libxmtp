use crate::encoded_content::{ContentTypeId, EncodedContent};
use serde::{Deserialize, Serialize};
use std::convert::TryInto;
use tsify::Tsify;
use wasm_bindgen::JsError;
use wasm_bindgen::prelude::wasm_bindgen;
use xmtp_content_types::ContentCodec;
use xmtp_content_types::remote_attachment::{
  RemoteAttachmentCodec, decrypt_attachment as xmtp_decrypt_attachment,
  encrypt_attachment as xmtp_encrypt_attachment,
};

use super::attachment::Attachment;

#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct RemoteAttachment {
  pub url: String,
  pub content_digest: String,
  #[serde(with = "serde_bytes")]
  #[tsify(type = "Uint8Array")]
  pub secret: Vec<u8>,
  #[serde(with = "serde_bytes")]
  #[tsify(type = "Uint8Array")]
  pub salt: Vec<u8>,
  #[serde(with = "serde_bytes")]
  #[tsify(type = "Uint8Array")]
  pub nonce: Vec<u8>,
  pub scheme: String,
  pub content_length: u32,
  #[serde(skip_serializing_if = "Option::is_none")]
  #[tsify(optional)]
  pub filename: Option<String>,
}

impl TryFrom<xmtp_content_types::remote_attachment::RemoteAttachment> for RemoteAttachment {
  type Error = JsError;

  fn try_from(
    remote: xmtp_content_types::remote_attachment::RemoteAttachment,
  ) -> std::result::Result<Self, Self::Error> {
    let content_length = u32::try_from(remote.content_length).map_err(|_| {
      JsError::new(&format!(
        "content_length {} exceeds maximum value of {} bytes",
        remote.content_length,
        u32::MAX
      ))
    })?;

    Ok(Self {
      url: remote.url,
      content_digest: remote.content_digest,
      secret: remote.secret,
      salt: remote.salt,
      nonce: remote.nonce,
      scheme: remote.scheme,
      content_length,
      filename: remote.filename,
    })
  }
}

impl From<RemoteAttachment> for xmtp_content_types::remote_attachment::RemoteAttachment {
  fn from(remote: RemoteAttachment) -> Self {
    Self {
      url: remote.url,
      content_digest: remote.content_digest,
      secret: remote.secret,
      salt: remote.salt,
      nonce: remote.nonce,
      scheme: remote.scheme,
      content_length: remote.content_length as usize,
      filename: remote.filename,
    }
  }
}

#[wasm_bindgen(js_name = "contentTypeRemoteAttachment")]
pub fn content_type_remote_attachment() -> ContentTypeId {
  RemoteAttachmentCodec::content_type().into()
}

#[wasm_bindgen(js_name = "encodeRemoteAttachment")]
pub fn encode_remote_attachment(
  remote_attachment: RemoteAttachment,
) -> Result<EncodedContent, JsError> {
  Ok(
    RemoteAttachmentCodec::encode(remote_attachment.into())
      .map_err(|e| JsError::new(&format!("{}", e)))?
      .into(),
  )
}

/// Result of encrypting an attachment for remote storage.
#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct EncryptedAttachment {
  /// The encrypted bytes to upload to the remote server
  #[serde(with = "serde_bytes")]
  #[tsify(type = "Uint8Array")]
  pub payload: Vec<u8>,
  /// SHA-256 digest of the encrypted bytes (hex-encoded)
  pub content_digest: String,
  /// The 32-byte secret key needed for decryption
  #[serde(with = "serde_bytes")]
  #[tsify(type = "Uint8Array")]
  pub secret: Vec<u8>,
  /// The 32-byte salt used in key derivation
  #[serde(with = "serde_bytes")]
  #[tsify(type = "Uint8Array")]
  pub salt: Vec<u8>,
  /// The 12-byte nonce used in encryption
  #[serde(with = "serde_bytes")]
  #[tsify(type = "Uint8Array")]
  pub nonce: Vec<u8>,
  /// The length of the encrypted content
  pub content_length: u32,
  /// The filename of the attachment
  #[serde(skip_serializing_if = "Option::is_none")]
  #[tsify(optional)]
  pub filename: Option<String>,
}

impl TryFrom<xmtp_content_types::remote_attachment::EncryptedAttachment> for EncryptedAttachment {
  type Error = JsError;

  fn try_from(
    encrypted: xmtp_content_types::remote_attachment::EncryptedAttachment,
  ) -> std::result::Result<Self, Self::Error> {
    let content_length = u32::try_from(encrypted.content_length).map_err(|_| {
      JsError::new(&format!(
        "content_length {} exceeds maximum value of {} bytes",
        encrypted.content_length,
        u32::MAX
      ))
    })?;

    Ok(Self {
      payload: encrypted.payload,
      content_digest: encrypted.content_digest,
      secret: encrypted.secret,
      salt: encrypted.salt,
      nonce: encrypted.nonce,
      content_length,
      filename: encrypted.filename,
    })
  }
}

/// Encrypts an attachment for storage as a remote attachment.
#[wasm_bindgen(js_name = "encryptAttachment")]
pub fn encrypt_attachment(attachment: Attachment) -> Result<EncryptedAttachment, JsError> {
  xmtp_encrypt_attachment(attachment.into())
    .map_err(|e| JsError::new(&format!("{}", e)))?
    .try_into()
}

/// Decrypts an encrypted payload from a remote attachment.
#[wasm_bindgen(js_name = "decryptAttachment")]
pub fn decrypt_attachment(
  #[wasm_bindgen(js_name = "encryptedBytes")] encrypted_bytes: &[u8],
  #[wasm_bindgen(js_name = "remoteAttachment")] remote_attachment: RemoteAttachment,
) -> Result<Attachment, JsError> {
  let decrypted = xmtp_decrypt_attachment(encrypted_bytes, &remote_attachment.into())
    .map_err(|e| JsError::new(&format!("{}", e)))?;
  Ok(decrypted.into())
}
