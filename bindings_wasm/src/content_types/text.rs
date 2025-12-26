use crate::encoded_content::{ContentTypeId, EncodedContent};
use wasm_bindgen::JsError;
use wasm_bindgen::prelude::wasm_bindgen;
use xmtp_content_types::ContentCodec;
use xmtp_content_types::text::TextCodec;

#[wasm_bindgen(js_name = "contentTypeText")]
pub fn content_type_text() -> ContentTypeId {
  TextCodec::content_type().into()
}

#[wasm_bindgen(js_name = "encodeText")]
pub fn encode_text(text: String) -> Result<EncodedContent, JsError> {
  Ok(
    TextCodec::encode(text)
      .map_err(|e| JsError::new(&format!("{}", e)))?
      .into(),
  )
}
