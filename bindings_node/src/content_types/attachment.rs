use napi::bindgen_prelude::{Result, Uint8Array};
use napi_derive::napi;
use prost::Message;
use xmtp_content_types::{ContentCodec, attachment::AttachmentCodec};
use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

use crate::ErrorWrapper;

#[derive(Clone)]
#[napi(object)]
pub struct Attachment {
  pub filename: Option<String>,
  pub mime_type: String,
  pub content: Vec<u8>,
}

impl From<xmtp_content_types::attachment::Attachment> for Attachment {
  fn from(attachment: xmtp_content_types::attachment::Attachment) -> Self {
    Self {
      filename: attachment.filename,
      mime_type: attachment.mime_type,
      content: attachment.content,
    }
  }
}

impl From<Attachment> for xmtp_content_types::attachment::Attachment {
  fn from(attachment: Attachment) -> Self {
    Self {
      filename: attachment.filename,
      mime_type: attachment.mime_type,
      content: attachment.content,
    }
  }
}

#[napi]
pub fn encode_attachment(attachment: Attachment) -> Result<Uint8Array> {
  // Use AttachmentCodec to encode the attachment
  let encoded = AttachmentCodec::encode(attachment.into()).map_err(ErrorWrapper::from)?;

  // Encode the EncodedContent to bytes
  let mut buf = Vec::new();
  encoded.encode(&mut buf).map_err(ErrorWrapper::from)?;

  Ok(buf.into())
}

#[napi]
pub fn decode_attachment(bytes: Uint8Array) -> Result<Attachment> {
  // Decode bytes into EncodedContent
  let encoded_content =
    EncodedContent::decode(bytes.to_vec().as_slice()).map_err(ErrorWrapper::from)?;

  // Use AttachmentCodec to decode into Attachment and convert to Attachment
  AttachmentCodec::decode(encoded_content)
    .map(Into::into)
    .map_err(|e| napi::Error::from_reason(e.to_string()))
}
