use crate::encoded_content::EncodedContent;
use js_sys::Uint8Array;
use prost::Message;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::{JsError, JsValue};
use xmtp_content_types::ContentCodec;
use xmtp_content_types::intent::IntentCodec;

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
  pub fn new(
    id: String,
    #[wasm_bindgen(js_name = actionId)] action_id: String,
    metadata: JsValue,
  ) -> Self {
    Self {
      id,
      action_id,
      metadata,
    }
  }
}

impl TryFrom<xmtp_content_types::intent::Intent> for Intent {
  type Error = JsError;

  fn try_from(intent: xmtp_content_types::intent::Intent) -> Result<Self, Self::Error> {
    let metadata = if let Some(data) = intent.metadata {
      serde_wasm_bindgen::to_value(&data)
        .map_err(|e| JsError::new(&format!("Failed to serialize Intent metadata: {}", e)))?
    } else {
      JsValue::UNDEFINED
    };

    Ok(Self {
      id: intent.id,
      action_id: intent.action_id,
      metadata,
    })
  }
}

impl TryFrom<Intent> for xmtp_content_types::intent::Intent {
  type Error = JsError;

  fn try_from(intent: Intent) -> Result<Self, Self::Error> {
    let metadata = if intent.metadata.is_null() || intent.metadata.is_undefined() {
      None
    } else {
      Some(
        serde_wasm_bindgen::from_value(intent.metadata)
          .map_err(|e| JsError::new(&format!("Failed to deserialize Intent metadata: {}", e)))?,
      )
    };

    Ok(Self {
      id: intent.id,
      action_id: intent.action_id,
      metadata,
    })
  }
}

#[wasm_bindgen(js_name = "encodeIntent")]
pub fn encode_intent(intent: Intent) -> Result<Uint8Array, JsError> {
  // Convert Intent and use IntentCodec to encode
  let intent: xmtp_content_types::intent::Intent = intent.try_into()?;
  let encoded = IntentCodec::encode(intent).map_err(|e| JsError::new(&format!("{}", e)))?;

  // Encode the EncodedContent to bytes
  let mut buf = Vec::new();
  encoded
    .encode(&mut buf)
    .map_err(|e| JsError::new(&format!("{}", e)))?;

  Ok(Uint8Array::from(buf.as_slice()))
}

#[wasm_bindgen(js_name = "decodeIntent")]
pub fn decode_intent(encoded_content: EncodedContent) -> Result<Intent, JsError> {
  // Use IntentCodec to decode into Intent and convert to WASM Intent
  let intent =
    IntentCodec::decode(encoded_content.into()).map_err(|e| JsError::new(&format!("{}", e)))?;

  intent.try_into()
}
