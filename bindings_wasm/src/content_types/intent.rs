use crate::encoded_content::{ContentTypeId, EncodedContent};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use tsify::Tsify;
use wasm_bindgen::JsError;
use wasm_bindgen::prelude::wasm_bindgen;
use xmtp_content_types::ContentCodec;
use xmtp_content_types::intent::IntentCodec as XmtpIntentCodec;

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

#[wasm_bindgen]
pub struct IntentCodec;

#[wasm_bindgen]
impl IntentCodec {
  #[wasm_bindgen(js_name = "contentType")]
  pub fn content_type() -> ContentTypeId {
    XmtpIntentCodec::content_type().into()
  }

  #[wasm_bindgen]
  pub fn encode(intent: Intent) -> Result<EncodedContent, JsError> {
    let encoded_content =
      XmtpIntentCodec::encode(intent.into()).map_err(|e| JsError::new(&format!("{}", e)))?;
    Ok(encoded_content.into())
  }

  #[wasm_bindgen]
  pub fn decode(encoded_content: EncodedContent) -> Result<Intent, JsError> {
    XmtpIntentCodec::decode(encoded_content.into())
      .map(Into::into)
      .map_err(|e| JsError::new(&format!("{}", e)))
  }

  #[wasm_bindgen(js_name = "shouldPush")]
  pub fn should_push() -> bool {
    XmtpIntentCodec::should_push()
  }
}
