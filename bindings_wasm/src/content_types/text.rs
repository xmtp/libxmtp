use crate::encoded_content::EncodedContent;
use js_sys::Uint8Array;
use prost::Message;
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

#[wasm_bindgen(js_name = "encodeXmtpText")]
pub fn encode_text(text: String) -> Result<Uint8Array, JsError> {
  // Use TextCodec to encode the text
  let encoded = TextCodec::encode(text).map_err(|e| JsError::new(&format!("{}", e)))?;

  // Encode the EncodedContent to bytes
  let mut buf = Vec::new();
  encoded
    .encode(&mut buf)
    .map_err(|e| JsError::new(&format!("{}", e)))?;

  Ok(Uint8Array::from(buf.as_slice()))
}

// `decode` conflicts with some wasm function in bindgen/tests/somewhere
// breaking `bindings_wasm` tests
// PR: https://github.com/xmtp/libxmtp/pull/2863
#[wasm_bindgen(js_name = "decodeXmtpText")]
pub fn decode_text(encoded_content: EncodedContent) -> Result<String, JsError> {
  // Use TextCodec to decode into String
  TextCodec::decode(encoded_content.into()).map_err(|e| JsError::new(&format!("{}", e)))
}
