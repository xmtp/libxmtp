use napi::bindgen_prelude::Result;
use napi_derive::napi;
use serde_json::Value;
use std::collections::HashMap;
use xmtp_content_types::ContentCodec;
use xmtp_content_types::intent::IntentCodec as XmtpIntentCodec;

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
pub struct IntentCodec {}

#[napi]
impl IntentCodec {
  #[napi]
  pub fn content_type() -> ContentTypeId {
    XmtpIntentCodec::content_type().into()
  }

  #[napi]
  pub fn encode(intent: Intent) -> Result<EncodedContent> {
    let encoded_content = XmtpIntentCodec::encode(intent.into()).map_err(ErrorWrapper::from)?;
    Ok(encoded_content.into())
  }

  #[napi]
  pub fn decode(encoded_content: EncodedContent) -> Result<Intent> {
    Ok(
      XmtpIntentCodec::decode(encoded_content.into())
        .map(Into::into)
        .map_err(ErrorWrapper::from)?,
    )
  }

  #[napi]
  pub fn should_push() -> bool {
    XmtpIntentCodec::should_push()
  }
}
