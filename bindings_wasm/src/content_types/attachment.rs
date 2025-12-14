use crate::encoded_content::{ContentTypeId, EncodedContent};
use js_sys::Uint8Array;
use wasm_bindgen::{JsError, prelude::wasm_bindgen};
use xmtp_content_types::ContentCodec;
use xmtp_content_types::attachment::AttachmentCodec as XmtpAttachmentCodec;

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
pub struct Attachment {
  pub filename: Option<String>,
  #[wasm_bindgen(js_name = "mimeType")]
  pub mime_type: String,
  pub content: Uint8Array,
}

#[wasm_bindgen]
impl Attachment {
  #[wasm_bindgen(constructor)]
  pub fn new(
    filename: Option<String>,
    #[wasm_bindgen(js_name = "mimeType")] mime_type: String,
    content: Uint8Array,
  ) -> Self {
    Self {
      filename,
      mime_type,
      content,
    }
  }
}

impl From<xmtp_content_types::attachment::Attachment> for Attachment {
  fn from(attachment: xmtp_content_types::attachment::Attachment) -> Self {
    Self {
      filename: attachment.filename,
      mime_type: attachment.mime_type,
      content: attachment.content.as_slice().into(),
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

#[wasm_bindgen]
pub struct AttachmentCodec;

#[wasm_bindgen]
impl AttachmentCodec {
  #[wasm_bindgen(js_name = "contentType")]
  pub fn content_type() -> ContentTypeId {
    XmtpAttachmentCodec::content_type().into()
  }

  #[wasm_bindgen]
  pub fn encode(attachment: Attachment) -> Result<EncodedContent, JsError> {
    let encoded_content = XmtpAttachmentCodec::encode(attachment.into())
      .map_err(|e| JsError::new(&format!("{}", e)))?;
    Ok(encoded_content.into())
  }

  #[wasm_bindgen]
  pub fn decode(encoded_content: EncodedContent) -> Result<Attachment, JsError> {
    XmtpAttachmentCodec::decode(encoded_content.into())
      .map(Into::into)
      .map_err(|e| JsError::new(&format!("{}", e)))
  }

  #[wasm_bindgen(js_name = "shouldPush")]
  pub fn should_push() -> bool {
    XmtpAttachmentCodec::should_push()
  }
}
