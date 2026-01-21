use super::attachment::Attachment;
use crate::ErrorWrapper;
use crate::messages::encoded_content::{ContentTypeId, EncodedContent};
use napi::bindgen_prelude::{Result, Uint8Array};
use napi_derive::napi;
use xmtp_content_types::{
  ContentCodec,
  remote_attachment::{
    RemoteAttachmentCodec, decrypt_attachment as xmtp_decrypt_attachment,
    encrypt_attachment as xmtp_encrypt_attachment,
  },
};

#[napi(object)]
pub struct RemoteAttachment {
  pub url: String,
  pub content_digest: String,
  pub secret: Uint8Array,
  pub salt: Uint8Array,
  pub nonce: Uint8Array,
  pub scheme: String,
  pub content_length: Option<u32>,
  pub filename: Option<String>,
}

impl Clone for RemoteAttachment {
  fn clone(&self) -> Self {
    Self {
      url: self.url.clone(),
      content_digest: self.content_digest.clone(),
      secret: Uint8Array::from(self.secret.to_vec()),
      salt: Uint8Array::from(self.salt.to_vec()),
      nonce: Uint8Array::from(self.nonce.to_vec()),
      scheme: self.scheme.clone(),
      content_length: self.content_length,
      filename: self.filename.clone(),
    }
  }
}

impl From<xmtp_content_types::remote_attachment::RemoteAttachment> for RemoteAttachment {
  fn from(remote: xmtp_content_types::remote_attachment::RemoteAttachment) -> Self {
    Self {
      url: remote.url,
      content_digest: remote.content_digest,
      secret: remote.secret.into(),
      salt: remote.salt.into(),
      nonce: remote.nonce.into(),
      scheme: remote.scheme,
      content_length: remote.content_length,
      filename: remote.filename,
    }
  }
}

impl From<RemoteAttachment> for xmtp_content_types::remote_attachment::RemoteAttachment {
  fn from(remote: RemoteAttachment) -> Self {
    Self {
      url: remote.url,
      content_digest: remote.content_digest,
      secret: remote.secret.to_vec(),
      salt: remote.salt.to_vec(),
      nonce: remote.nonce.to_vec(),
      scheme: remote.scheme,
      content_length: remote.content_length,
      filename: remote.filename,
    }
  }
}

#[napi]
pub fn content_type_remote_attachment() -> ContentTypeId {
  RemoteAttachmentCodec::content_type().into()
}

#[napi]
pub fn encode_remote_attachment(remote_attachment: RemoteAttachment) -> Result<EncodedContent> {
  Ok(
    RemoteAttachmentCodec::encode(remote_attachment.into())
      .map_err(ErrorWrapper::from)?
      .into(),
  )
}

/// Result of encrypting an attachment for remote storage.
#[napi(object)]
pub struct EncryptedAttachment {
  /// The encrypted bytes to upload to the remote server
  pub payload: Uint8Array,
  /// SHA-256 digest of the encrypted bytes (hex-encoded)
  pub content_digest: String,
  /// The 32-byte secret key needed for decryption
  pub secret: Uint8Array,
  /// The 32-byte salt used in key derivation
  pub salt: Uint8Array,
  /// The 12-byte nonce used in encryption
  pub nonce: Uint8Array,
  /// The length of the encrypted content
  pub content_length: u32,
  /// The filename of the attachment
  pub filename: Option<String>,
}

impl Clone for EncryptedAttachment {
  fn clone(&self) -> Self {
    Self {
      payload: Uint8Array::from(self.payload.to_vec()),
      content_digest: self.content_digest.clone(),
      secret: Uint8Array::from(self.secret.to_vec()),
      salt: Uint8Array::from(self.salt.to_vec()),
      nonce: Uint8Array::from(self.nonce.to_vec()),
      content_length: self.content_length,
      filename: self.filename.clone(),
    }
  }
}

impl From<xmtp_content_types::remote_attachment::EncryptedAttachment> for EncryptedAttachment {
  fn from(encrypted: xmtp_content_types::remote_attachment::EncryptedAttachment) -> Self {
    Self {
      payload: encrypted.payload.into(),
      content_digest: encrypted.content_digest,
      secret: encrypted.secret.into(),
      salt: encrypted.salt.into(),
      nonce: encrypted.nonce.into(),
      content_length: encrypted.content_length,
      filename: encrypted.filename,
    }
  }
}

/// Encrypts an attachment for storage as a remote attachment.
#[napi]
pub fn encrypt_attachment(attachment: Attachment) -> Result<EncryptedAttachment> {
  Ok(
    xmtp_encrypt_attachment(attachment.into())
      .map_err(ErrorWrapper::from)?
      .into(),
  )
}

/// Decrypts an encrypted payload from a remote attachment.
#[napi]
pub fn decrypt_attachment(
  encrypted_bytes: Uint8Array,
  remote_attachment: RemoteAttachment,
) -> Result<Attachment> {
  let decrypted = xmtp_decrypt_attachment(&encrypted_bytes, &remote_attachment.into())
    .map_err(ErrorWrapper::from)?;
  Ok(decrypted.into())
}
