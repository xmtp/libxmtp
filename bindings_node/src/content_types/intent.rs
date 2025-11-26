use napi::bindgen_prelude::{Result, Uint8Array};
use napi_derive::napi;
use prost::Message;
use serde_json::Value;
use std::collections::HashMap;
use xmtp_content_types::ContentCodec;
use xmtp_content_types::intent::IntentCodec;
use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

use crate::ErrorWrapper;

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
pub fn encode_intent(intent: Intent) -> Result<Uint8Array> {
  // Use IntentCodec to encode the intent
  let encoded = IntentCodec::encode(intent.into()).map_err(ErrorWrapper::from)?;

  // Encode the EncodedContent to bytes
  let mut buf = Vec::new();
  encoded.encode(&mut buf).map_err(ErrorWrapper::from)?;

  Ok(Uint8Array::from(buf.as_slice()))
}

#[napi]
pub fn decode_intent(bytes: Uint8Array) -> Result<Intent> {
  // Decode bytes into EncodedContent
  let encoded_content = EncodedContent::decode(bytes.as_ref()).map_err(ErrorWrapper::from)?;

  // Use IntentCodec to decode into Intent and convert to Intent
  Ok(
    IntentCodec::decode(encoded_content)
      .map(Into::into)
      .map_err(ErrorWrapper::from)?,
  )
}
