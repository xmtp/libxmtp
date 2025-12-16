use crate::encoded_content::{ContentTypeId, EncodedContent};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use tsify::Tsify;
use wasm_bindgen::JsError;
use wasm_bindgen::prelude::wasm_bindgen;
use xmtp_content_types::ContentCodec;
use xmtp_content_types::intent::IntentCodec;

#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi, missing_as_null, hashmap_as_object)]
#[serde(rename_all = "camelCase")]
pub struct Intent {
  pub id: String,
  pub action_id: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  #[tsify(optional, type = "Record<string, string | number | boolean | null>")]
  pub metadata: Option<HashMap<String, Value>>,
}

impl From<xmtp_content_types::intent::Intent> for Intent {
  fn from(intent: xmtp_content_types::intent::Intent) -> Self {
    Self {
      id: intent.id,
      action_id: intent.action_id,
      metadata: intent.metadata,
    }
  }
}

impl From<Intent> for xmtp_content_types::intent::Intent {
  fn from(intent: Intent) -> Self {
    Self {
      id: intent.id,
      action_id: intent.action_id,
      metadata: intent.metadata,
    }
  }
}

#[wasm_bindgen(js_name = "intentContentType")]
pub fn intent_content_type() -> ContentTypeId {
  IntentCodec::content_type().into()
}

#[wasm_bindgen(js_name = "encodeIntent")]
pub fn encode_intent(intent: Intent) -> Result<EncodedContent, JsError> {
  Ok(
    IntentCodec::encode(intent.into())
      .map_err(|e| JsError::new(&format!("{}", e)))?
      .into(),
  )
}
