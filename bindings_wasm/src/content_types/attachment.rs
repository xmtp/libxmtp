use crate::encoded_content::{ContentTypeId, EncodedContent};
use serde::{Deserialize, Serialize};
use tsify::Tsify;
use wasm_bindgen::JsError;
use wasm_bindgen::prelude::wasm_bindgen;
use xmtp_content_types::ContentCodec;
use xmtp_content_types::attachment::AttachmentCodec;

#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
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

#[wasm_bindgen(js_name = "contentTypeAttachment")]
pub fn content_type_attachment() -> ContentTypeId {
  AttachmentCodec::content_type().into()
}

#[wasm_bindgen(js_name = "encodeAttachment")]
pub fn encode_attachment(attachment: Attachment) -> Result<EncodedContent, JsError> {
  Ok(
    AttachmentCodec::encode(attachment.into())
      .map_err(|e| JsError::new(&format!("{}", e)))?
      .into(),
  )
}
