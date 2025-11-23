use napi::bindgen_prelude::{Result, Uint8Array};
use napi_derive::napi;
use prost::Message;
use xmtp_content_types::ContentCodec;
use xmtp_content_types::multi_remote_attachment::MultiRemoteAttachmentCodec;
use xmtp_proto::xmtp::mls::message_contents::EncodedContent;
use xmtp_proto::xmtp::mls::message_contents::content_types::{
  MultiRemoteAttachment as XmtpMultiRemoteAttachment,
  RemoteAttachmentInfo as XmtpRemoteAttachmentInfo,
};

use crate::ErrorWrapper;

#[napi(object)]
pub struct RemoteAttachmentInfo {
  pub secret: Uint8Array,
  pub content_digest: String,
  pub nonce: Uint8Array,
  pub scheme: String,
  pub url: String,
  pub salt: Uint8Array,
  pub content_length: Option<u32>,
  pub filename: Option<String>,
}

impl From<RemoteAttachmentInfo> for XmtpRemoteAttachmentInfo {
  fn from(remote_attachment_info: RemoteAttachmentInfo) -> Self {
    XmtpRemoteAttachmentInfo {
      content_digest: remote_attachment_info.content_digest,
      secret: remote_attachment_info.secret.to_vec(),
      nonce: remote_attachment_info.nonce.to_vec(),
      salt: remote_attachment_info.salt.to_vec(),
      scheme: remote_attachment_info.scheme,
      url: remote_attachment_info.url,
      content_length: remote_attachment_info.content_length,
      filename: remote_attachment_info.filename,
    }
  }
}

impl From<XmtpRemoteAttachmentInfo> for RemoteAttachmentInfo {
  fn from(remote_attachment_info: XmtpRemoteAttachmentInfo) -> Self {
    RemoteAttachmentInfo {
      secret: remote_attachment_info.secret.into(),
      content_digest: remote_attachment_info.content_digest,
      nonce: remote_attachment_info.nonce.into(),
      scheme: remote_attachment_info.scheme,
      url: remote_attachment_info.url,
      salt: remote_attachment_info.salt.into(),
      content_length: remote_attachment_info.content_length,
      filename: remote_attachment_info.filename,
    }
  }
}

#[napi(object)]
pub struct MultiRemoteAttachment {
  pub attachments: Vec<RemoteAttachmentInfo>,
}

impl From<MultiRemoteAttachment> for XmtpMultiRemoteAttachment {
  fn from(multi_remote_attachment: MultiRemoteAttachment) -> Self {
    XmtpMultiRemoteAttachment {
      attachments: multi_remote_attachment
        .attachments
        .into_iter()
        .map(Into::into)
        .collect(),
    }
  }
}

impl From<XmtpMultiRemoteAttachment> for MultiRemoteAttachment {
  fn from(multi_remote_attachment: XmtpMultiRemoteAttachment) -> Self {
    MultiRemoteAttachment {
      attachments: multi_remote_attachment
        .attachments
        .into_iter()
        .map(Into::into)
        .collect(),
    }
  }
}

#[napi]
pub fn encode_multi_remote_attachment(
  multi_remote_attachment: MultiRemoteAttachment,
) -> Result<Uint8Array> {
  // Convert MultiRemoteAttachment to MultiRemoteAttachment
  let multi_remote_attachment: XmtpMultiRemoteAttachment = multi_remote_attachment.into();

  // Use MultiRemoteAttachmentCodec to encode the attachments
  let encoded =
    MultiRemoteAttachmentCodec::encode(multi_remote_attachment).map_err(ErrorWrapper::from)?;

  // Encode the EncodedContent to bytes
  let mut buf = Vec::new();
  encoded.encode(&mut buf).map_err(ErrorWrapper::from)?;

  Ok(buf.into())
}

#[napi]
pub fn decode_multi_remote_attachment(bytes: Uint8Array) -> Result<MultiRemoteAttachment> {
  // Decode bytes into EncodedContent
  let encoded_content = EncodedContent::decode(bytes.as_ref()).map_err(ErrorWrapper::from)?;

  // Use MultiRemoteAttachmentCodec to decode into MultiRemoteAttachment and convert to MultiRemoteAttachment
  MultiRemoteAttachmentCodec::decode(encoded_content)
    .map(Into::into)
    .map_err(|e| napi::Error::from_reason(e.to_string()))
}

// Additional types for enriched messages using Vec<u8> instead of Uint8Array
#[derive(Clone)]
#[napi(object)]
pub struct RemoteAttachmentInfoPayload {
  pub url: String,
  pub content_digest: String,
  pub secret: Vec<u8>,
  pub salt: Vec<u8>,
  pub nonce: Vec<u8>,
  pub scheme: String,
  pub content_length: Option<u32>,
  pub filename: Option<String>,
}

impl From<xmtp_proto::xmtp::mls::message_contents::content_types::RemoteAttachmentInfo>
  for RemoteAttachmentInfoPayload
{
  fn from(
    info: xmtp_proto::xmtp::mls::message_contents::content_types::RemoteAttachmentInfo,
  ) -> Self {
    Self {
      url: info.url,
      content_digest: info.content_digest,
      secret: info.secret,
      salt: info.salt,
      nonce: info.nonce,
      scheme: info.scheme,
      content_length: info.content_length,
      filename: info.filename,
    }
  }
}

#[derive(Clone)]
#[napi(object)]
pub struct MultiRemoteAttachmentPayload {
  pub attachments: Vec<RemoteAttachmentInfoPayload>,
}

impl From<xmtp_proto::xmtp::mls::message_contents::content_types::MultiRemoteAttachment>
  for MultiRemoteAttachmentPayload
{
  fn from(
    multi: xmtp_proto::xmtp::mls::message_contents::content_types::MultiRemoteAttachment,
  ) -> Self {
    Self {
      attachments: multi.attachments.into_iter().map(|a| a.into()).collect(),
    }
  }
}
