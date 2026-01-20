use crate::ErrorWrapper;
use crate::messages::encoded_content::{ContentTypeId, EncodedContent};
use napi::bindgen_prelude::{Result, Uint8Array};
use napi_derive::napi;
use xmtp_content_types::{ContentCodec, attachment::AttachmentCodec};

#[napi(object)]
pub struct Attachment {
  pub filename: Option<String>,
  pub mime_type: String,
  pub content: Uint8Array,
}

impl Clone for Attachment {
  fn clone(&self) -> Self {
    Self {
      filename: self.filename.clone(),
      mime_type: self.mime_type.clone(),
      content: Uint8Array::from(self.content.to_vec()),
    }
  }
}

impl From<xmtp_content_types::attachment::Attachment> for Attachment {
  fn from(attachment: xmtp_content_types::attachment::Attachment) -> Self {
    Self {
      filename: attachment.filename,
      mime_type: attachment.mime_type,
      content: attachment.content.into(),
    }
  }
}

impl From<Attachment> for xmtp_content_types::attachment::Attachment {
  fn from(attachment: Attachment) -> Self {
    Self {
      filename: attachment.filename,
      mime_type: attachment.mime_type,
      content: attachment.content.to_vec(),
    }
  }
}

#[napi]
pub fn content_type_attachment() -> ContentTypeId {
  AttachmentCodec::content_type().into()
}

#[napi]
pub fn encode_attachment(attachment: Attachment) -> Result<EncodedContent> {
  Ok(
    AttachmentCodec::encode(attachment.into())
      .map_err(ErrorWrapper::from)?
      .into(),
  )
}
