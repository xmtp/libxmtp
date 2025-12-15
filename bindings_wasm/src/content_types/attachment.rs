use crate::encoded_content::{ContentTypeId, EncodedContent};
use serde::{Deserialize, Serialize};
use tsify::Tsify;
use wasm_bindgen::{JsError, prelude::wasm_bindgen};
use xmtp_content_types::ContentCodec;
use xmtp_content_types::attachment::AttachmentCodec as XmtpAttachmentCodec;

#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
  pub filename: Option<String>,
  pub mime_type: String,
  #[serde(with = "serde_bytes")]
  #[tsify(type = "Uint8Array")]
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
