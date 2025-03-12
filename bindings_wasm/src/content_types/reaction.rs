use js_sys::Uint8Array;
use prost::Message;
use serde::{Deserialize, Serialize};
use tsify_next::Tsify;
use wasm_bindgen::{prelude::wasm_bindgen, JsError};
use xmtp_content_types::reaction::ReactionCodec;
use xmtp_content_types::ContentCodec;
use xmtp_proto::xmtp::mls::message_contents::content_types::ReactionV2;
use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

#[derive(Tsify, Clone, Serialize, Deserialize)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct Reaction {
  pub reference: String,
  #[serde(rename = "referenceInboxId")]
  pub reference_inbox_id: String,
  pub action: ReactionAction,
  pub content: String,
  pub schema: ReactionSchema,
}

impl From<Reaction> for ReactionV2 {
  fn from(reaction: Reaction) -> Self {
    ReactionV2 {
      reference: reaction.reference,
      reference_inbox_id: reaction.reference_inbox_id,
      action: reaction.action.into(),
      content: reaction.content,
      schema: reaction.schema.into(),
    }
  }
}

impl From<ReactionV2> for Reaction {
  fn from(reaction: ReactionV2) -> Self {
    Reaction {
      reference: reaction.reference,
      reference_inbox_id: reaction.reference_inbox_id,
      action: match reaction.action {
        1 => ReactionAction::Added,
        2 => ReactionAction::Removed,
        _ => ReactionAction::Unknown,
      },
      content: reaction.content,
      schema: match reaction.schema {
        1 => ReactionSchema::Unicode,
        2 => ReactionSchema::Shortcode,
        3 => ReactionSchema::Custom,
        _ => ReactionSchema::Unknown,
      },
    }
  }
}

#[wasm_bindgen(js_name = "encodeReaction")]
pub fn encode_reaction(reaction: Reaction) -> Result<Uint8Array, JsError> {
  // Convert Reaction to Reaction
  let reaction: ReactionV2 = reaction.into();

  // Use ReactionCodec to encode the reaction
  let encoded = ReactionCodec::encode(reaction).map_err(|e| JsError::new(&format!("{}", e)))?;

  // Encode the EncodedContent to bytes
  let mut buf = Vec::new();
  encoded
    .encode(&mut buf)
    .map_err(|e| JsError::new(&format!("{}", e)))?;

  Ok(Uint8Array::from(buf.as_slice()))
}

#[wasm_bindgen(js_name = "decodeReaction")]
pub fn decode_reaction(bytes: Uint8Array) -> Result<Reaction, JsError> {
  // Decode bytes into EncodedContent
  let encoded_content = EncodedContent::decode(bytes.to_vec().as_slice())
    .map_err(|e| JsError::new(&format!("{}", e)))?;

  // Use ReactionCodec to decode into Reaction and convert to Reaction
  ReactionCodec::decode(encoded_content)
    .map(Into::into)
    .map_err(|e| JsError::new(&format!("{}", e)))
}

#[derive(Tsify, Copy, Default, PartialEq, Debug, Clone, Serialize, Deserialize)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub enum ReactionAction {
  Unknown = 0,
  #[default]
  Added = 1,
  Removed = 2,
}

impl From<ReactionAction> for i32 {
  fn from(action: ReactionAction) -> Self {
    match action {
      ReactionAction::Unknown => 0,
      ReactionAction::Added => 1,
      ReactionAction::Removed => 2,
    }
  }
}

#[derive(Tsify, Copy, Default, PartialEq, Debug, Clone, Serialize, Deserialize)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub enum ReactionSchema {
  Unknown = 0,
  #[default]
  Unicode = 1,
  Shortcode = 2,
  Custom = 3,
}

impl From<ReactionSchema> for i32 {
  fn from(schema: ReactionSchema) -> Self {
    match schema {
      ReactionSchema::Unknown => 0,
      ReactionSchema::Unicode => 1,
      ReactionSchema::Shortcode => 2,
      ReactionSchema::Custom => 3,
    }
  }
}
