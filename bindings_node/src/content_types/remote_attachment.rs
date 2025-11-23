use napi::bindgen_prelude::{Error, Result, Uint8Array};
use napi_derive::napi;
use prost::Message;
use std::convert::TryFrom;
use xmtp_content_types::{ContentCodec, remote_attachment::RemoteAttachmentCodec};
use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

use crate::ErrorWrapper;

#[derive(Clone)]
#[napi(object)]
pub struct RemoteAttachment {
  pub url: String,
  pub content_digest: String,
  pub secret: Vec<u8>,
  pub salt: Vec<u8>,
  pub nonce: Vec<u8>,
  pub scheme: String,
  pub content_length: u32,
  pub filename: Option<String>,
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

#[napi]
pub fn encode_remote_attachment(remote_attachment: RemoteAttachment) -> Result<Uint8Array> {
  // Use RemoteAttachmentCodec to encode the remote attachment
  let encoded =
    RemoteAttachmentCodec::encode(remote_attachment.into()).map_err(ErrorWrapper::from)?;

  // Encode the EncodedContent to bytes
  let mut buf = Vec::new();
  encoded.encode(&mut buf).map_err(ErrorWrapper::from)?;

  Ok(buf.into())
}

#[napi]
pub fn decode_remote_attachment(bytes: Uint8Array) -> Result<RemoteAttachment> {
  // Decode bytes into EncodedContent
  let encoded_content = EncodedContent::decode(bytes.as_ref()).map_err(ErrorWrapper::from)?;

  // Use RemoteAttachmentCodec to decode into RemoteAttachment
  let attachment = RemoteAttachmentCodec::decode(encoded_content).map_err(ErrorWrapper::from)?;

  // Convert to bindings type with error handling
  RemoteAttachment::try_from(attachment)
}
