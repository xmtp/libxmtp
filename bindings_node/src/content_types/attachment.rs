use napi::bindgen_prelude::{Result, Uint8Array};
use napi_derive::napi;
use xmtp_content_types::{ContentCodec, attachment::AttachmentCodec as XmtpAttachmentCodec};

use crate::{
  ErrorWrapper,
  encoded_content::{ContentTypeId, EncodedContent},
};

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
pub struct AttachmentCodec {}

#[napi]
impl AttachmentCodec {
  #[napi]
  pub fn content_type() -> ContentTypeId {
    XmtpAttachmentCodec::content_type().into()
  }

  #[napi]
  pub fn encode(attachment: Attachment) -> Result<EncodedContent> {
    let encoded_content =
      XmtpAttachmentCodec::encode(attachment.into()).map_err(ErrorWrapper::from)?;
    Ok(encoded_content.into())
  }

  #[napi]
  pub fn decode(encoded_content: EncodedContent) -> Result<Attachment> {
    Ok(
      XmtpAttachmentCodec::decode(encoded_content.into())
        .map(Into::into)
        .map_err(ErrorWrapper::from)?,
    )
  }

  #[napi]
  pub fn should_push() -> bool {
    XmtpAttachmentCodec::should_push()
  }
}
