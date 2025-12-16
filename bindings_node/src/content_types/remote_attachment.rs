use napi::bindgen_prelude::{Error, Result, Uint8Array};
use napi_derive::napi;
use std::convert::TryFrom;
use xmtp_content_types::{ContentCodec, remote_attachment::RemoteAttachmentCodec};

use crate::ErrorWrapper;
use crate::encoded_content::{ContentTypeId, EncodedContent};

#[napi(object)]
pub struct RemoteAttachment {
  pub url: String,
  pub content_digest: String,
  pub secret: Uint8Array,
  pub salt: Uint8Array,
  pub nonce: Uint8Array,
  pub scheme: String,
  pub content_length: u32,
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

impl TryFrom<xmtp_content_types::remote_attachment::RemoteAttachment> for RemoteAttachment {
  type Error = Error;

  fn try_from(
    remote: xmtp_content_types::remote_attachment::RemoteAttachment,
  ) -> std::result::Result<Self, Self::Error> {
    let content_length = u32::try_from(remote.content_length).map_err(|_| {
      Error::from_reason(format!(
        "content_length {} exceeds maximum value of {} bytes",
        remote.content_length,
        u32::MAX
      ))
    })?;

    Ok(Self {
      url: remote.url,
      content_digest: remote.content_digest,
      secret: remote.secret.into(),
      salt: remote.salt.into(),
      nonce: remote.nonce.into(),
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
      secret: remote.secret.to_vec(),
      salt: remote.salt.to_vec(),
      nonce: remote.nonce.to_vec(),
      scheme: remote.scheme,
      content_length: remote.content_length as usize,
      filename: remote.filename,
    }
  }
}

#[napi]
pub fn remote_attachment_content_type() -> ContentTypeId {
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
