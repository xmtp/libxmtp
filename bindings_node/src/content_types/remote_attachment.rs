use napi::bindgen_prelude::{Result, Uint8Array};
use napi_derive::napi;
use prost::Message;
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
  pub content_length: i64,
  pub filename: Option<String>,
}

impl From<xmtp_content_types::remote_attachment::RemoteAttachment> for RemoteAttachment {
  fn from(remote: xmtp_content_types::remote_attachment::RemoteAttachment) -> Self {
    Self {
      url: remote.url,
      content_digest: remote.content_digest,
      secret: remote.secret,
      salt: remote.salt,
      nonce: remote.nonce,
      scheme: remote.scheme,
      content_length: remote.content_length as i64,
      filename: remote.filename,
    }
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
  let encoded_content =
    EncodedContent::decode(bytes.to_vec().as_slice()).map_err(ErrorWrapper::from)?;

  // Use RemoteAttachmentCodec to decode into RemoteAttachment and convert to RemoteAttachment
  RemoteAttachmentCodec::decode(encoded_content)
    .map(Into::into)
    .map_err(|e| napi::Error::from_reason(e.to_string()))
}
