use napi::bindgen_prelude::{Result, Uint8Array};
use napi_derive::napi;
use prost::Message;
use xmtp_content_types::{ContentCodec, attachment::AttachmentCodec};

use crate::{ErrorWrapper, encoded_content::EncodedContent};

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
pub fn decode_attachment(encoded_content: EncodedContent) -> Result<Attachment> {
  Ok(
    AttachmentCodec::decode(encoded_content.into())
      .map(Into::into)
      .map_err(ErrorWrapper::from)?,
  )
}
