use crate::encoded_content::EncodedContent;
use wasm_bindgen::{JsError, prelude::wasm_bindgen};
use xmtp_content_types::ContentCodec;
use xmtp_content_types::text::TextCodec;

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
pub struct TextContent {
  pub content: String,
}

impl From<xmtp_mls::messages::decoded_message::Text> for TextContent {
  fn from(text: xmtp_mls::messages::decoded_message::Text) -> Self {
    Self {
      content: text.content,
    }
  }
}

#[wasm_bindgen(js_name = "encodeTextContent")]
pub fn encode_text(text: String) -> Result<EncodedContent, JsError> {
  let encoded_content = TextCodec::encode(text).map_err(|e| JsError::new(&format!("{}", e)))?;
  Ok(encoded_content.into())
}

// `decode` conflicts with some wasm function in bindgen/tests/somewhere
// breaking `bindings_wasm` tests
// PR: https://github.com/xmtp/libxmtp/pull/2863
#[wasm_bindgen(js_name = "decodeTextContent")]
pub fn decode_text(encoded_content: EncodedContent) -> Result<String, JsError> {
  TextCodec::decode(encoded_content.into()).map_err(|e| JsError::new(&format!("{}", e)))
}
