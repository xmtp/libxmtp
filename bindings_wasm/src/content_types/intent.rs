use crate::encoded_content::{ContentTypeId, EncodedContent};
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::{JsError, JsValue};
use xmtp_content_types::ContentCodec;
use xmtp_content_types::intent::IntentCodec as XmtpIntentCodec;

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
    let intent: xmtp_content_types::intent::Intent = intent.try_into()?;
    let encoded_content =
      XmtpIntentCodec::encode(intent).map_err(|e| JsError::new(&format!("{}", e)))?;
    Ok(encoded_content.into())
  }

  #[wasm_bindgen]
  pub fn decode(encoded_content: EncodedContent) -> Result<Intent, JsError> {
    let intent = XmtpIntentCodec::decode(encoded_content.into())
      .map_err(|e| JsError::new(&format!("{}", e)))?;
    intent.try_into()
  }

  #[wasm_bindgen(js_name = "shouldPush")]
  pub fn should_push() -> bool {
    XmtpIntentCodec::should_push()
  }
}
