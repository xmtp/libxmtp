use crate::encoded_content::{ContentTypeId, EncodedContent};
use wasm_bindgen::JsError;
use wasm_bindgen::prelude::wasm_bindgen;
use xmtp_content_types::ContentCodec;
use xmtp_content_types::markdown::MarkdownCodec;

#[wasm_bindgen(js_name = "contentTypeMarkdown")]
pub fn content_type_markdown() -> ContentTypeId {
  MarkdownCodec::content_type().into()
}

#[wasm_bindgen(js_name = "encodeMarkdown")]
pub fn encode_markdown(text: String) -> Result<EncodedContent, JsError> {
  Ok(
    MarkdownCodec::encode(text)
      .map_err(|e| JsError::new(&format!("{}", e)))?
      .into(),
  )
}
