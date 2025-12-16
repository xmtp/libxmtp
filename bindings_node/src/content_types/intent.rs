use napi::bindgen_prelude::Result;
use napi_derive::napi;
use serde_json::Value;
use std::collections::HashMap;
use xmtp_content_types::ContentCodec;
use xmtp_content_types::intent::IntentCodec;

use crate::ErrorWrapper;
use crate::encoded_content::{ContentTypeId, EncodedContent};

#[derive(Clone)]
#[napi(object)]
pub struct Intent {
  pub id: String,
  pub action_id: String,
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

#[napi]
pub fn intent_content_type() -> ContentTypeId {
  IntentCodec::content_type().into()
}

#[napi]
pub fn encode_intent(intent: Intent) -> Result<EncodedContent> {
  Ok(
    IntentCodec::encode(intent.into())
      .map_err(ErrorWrapper::from)?
      .into(),
  )
}
