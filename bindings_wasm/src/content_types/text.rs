use crate::error::{ErrorCode, WasmError};
use js_sys::Uint8Array;
use prost::Message;
use wasm_bindgen::prelude::wasm_bindgen;
use xmtp_content_types::ContentCodec;
use xmtp_content_types::text::TextCodec;
use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

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

#[wasm_bindgen(js_name = "encodeText")]
pub fn encode_text(text: String) -> Result<Uint8Array, WasmError> {
  // Use TextCodec to encode the text
  let encoded =
    TextCodec::encode(text).map_err(|e| WasmError::from_error(ErrorCode::ContentType, e))?;

  // Encode the EncodedContent to bytes
  let mut buf = Vec::new();
  encoded
    .encode(&mut buf)
    .map_err(|e| WasmError::from_error(ErrorCode::Encoding, e))?;

  Ok(Uint8Array::from(buf.as_slice()))
}

#[wasm_bindgen(js_name = "decodeText")]
pub fn decode_text(bytes: Uint8Array) -> Result<String, WasmError> {
  // Decode bytes into EncodedContent
  let encoded_content = EncodedContent::decode(bytes.to_vec().as_slice())
    .map_err(|e| WasmError::from_error(ErrorCode::Encoding, e))?;

  // Use TextCodec to decode into String
  TextCodec::decode(encoded_content).map_err(|e| WasmError::from_error(ErrorCode::ContentType, e))
}
