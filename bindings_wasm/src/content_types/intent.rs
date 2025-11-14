use js_sys::Uint8Array;
use prost::Message;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::{JsError, JsValue};
use xmtp_content_types::ContentCodec;
use xmtp_content_types::intent::IntentCodec;
use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
pub struct Intent {
  pub id: String,
  #[wasm_bindgen(js_name = "actionId")]
  pub action_id: String,
  pub metadata: JsValue,
}

#[wasm_bindgen]
impl Intent {
  #[wasm_bindgen(constructor)]
  pub fn new(id: String, action_id: String, metadata: JsValue) -> Self {
    Self {
      id,
      action_id,
      metadata,
    }
  }
}

impl From<xmtp_content_types::intent::Intent> for Intent {
  fn from(intent: xmtp_content_types::intent::Intent) -> Self {
    let metadata = intent
      .metadata
      .and_then(|map| serde_wasm_bindgen::to_value(&map).ok())
      .unwrap_or(JsValue::UNDEFINED);

    Self {
      id: intent.id,
      action_id: intent.action_id,
      metadata,
    }
  }
}

impl From<Intent> for xmtp_content_types::intent::Intent {
  fn from(intent: Intent) -> Self {
    let metadata = if intent.metadata.is_null() || intent.metadata.is_undefined() {
      None
    } else {
      serde_wasm_bindgen::from_value(intent.metadata).ok()
    };

    Self {
      id: intent.id,
      action_id: intent.action_id,
      metadata,
    }
  }
}

#[wasm_bindgen(js_name = "encodeIntent")]
pub fn encode_intent(intent: Intent) -> Result<Uint8Array, JsError> {
  // Use IntentCodec to encode the intent
  let encoded = IntentCodec::encode(intent.into()).map_err(|e| JsError::new(&format!("{}", e)))?;

  // Encode the EncodedContent to bytes
  let mut buf = Vec::new();
  encoded
    .encode(&mut buf)
    .map_err(|e| JsError::new(&format!("{}", e)))?;

  Ok(Uint8Array::from(buf.as_slice()))
}

#[wasm_bindgen(js_name = "decodeIntent")]
pub fn decode_intent(bytes: Uint8Array) -> Result<Intent, JsError> {
  // Decode bytes into EncodedContent
  let encoded_content = EncodedContent::decode(bytes.to_vec().as_slice())
    .map_err(|e| JsError::new(&format!("{}", e)))?;

  // Use IntentCodec to decode into Intent and convert to Intent
  IntentCodec::decode(encoded_content)
    .map(Into::into)
    .map_err(|e| JsError::new(&format!("{}", e)))
}
