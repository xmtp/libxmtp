use js_sys::Uint8Array;
use prost::Message;
use wasm_bindgen::{JsError, prelude::wasm_bindgen};
use xmtp_content_types::ContentCodec;
use xmtp_content_types::reaction::ReactionCodec;
use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

#[wasm_bindgen]
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub enum ReactionAction {
  Unknown,
  #[default]
  Added,
  Removed,
}

impl From<ReactionAction> for String {
  fn from(action: ReactionAction) -> Self {
    match action {
      ReactionAction::Unknown => "unknown".to_string(),
      ReactionAction::Added => "added".to_string(),
      ReactionAction::Removed => "removed".to_string(),
    }
  }
}

#[wasm_bindgen]
#[derive(Copy, Clone, Default, PartialEq)]
pub enum ReactionSchema {
  Unknown,
  #[default]
  Unicode,
  Shortcode,
  Custom,
}

impl From<ReactionSchema> for String {
  fn from(schema: ReactionSchema) -> Self {
    match schema {
      ReactionSchema::Unknown => "unknown".to_string(),
      ReactionSchema::Unicode => "unicode".to_string(),
      ReactionSchema::Shortcode => "shortcode".to_string(),
      ReactionSchema::Custom => "custom".to_string(),
    }
  }
}

#[wasm_bindgen(getter_with_clone)]
pub struct Reaction {
  pub reference: String,
  #[wasm_bindgen(js_name = "referenceInboxId")]
  pub reference_inbox_id: Option<String>,
  pub action: ReactionAction,
  pub content: String,
  pub schema: ReactionSchema,
}

impl From<xmtp_content_types::reaction::Reaction> for Reaction {
  fn from(reaction: xmtp_content_types::reaction::Reaction) -> Self {
    Self {
      action: match reaction.action.as_str() {
        "added" => ReactionAction::Added,
        "removed" => ReactionAction::Removed,
        _ => ReactionAction::Unknown,
      },
      content: reaction.content,
      reference_inbox_id: reaction.reference_inbox_id,
      reference: reaction.reference,
      schema: match reaction.schema.as_str() {
        "unicode" => ReactionSchema::Unicode,
        "shortcode" => ReactionSchema::Shortcode,
        "custom" => ReactionSchema::Custom,
        _ => ReactionSchema::Unknown,
      },
    }
  }
}

impl From<Reaction> for xmtp_content_types::reaction::Reaction {
  fn from(reaction: Reaction) -> Self {
    Self {
      action: reaction.action.into(),
      content: reaction.content,
      reference_inbox_id: reaction.reference_inbox_id,
      reference: reaction.reference,
      schema: reaction.schema.into(),
    }
  }
}

#[wasm_bindgen]
impl Reaction {
  #[wasm_bindgen(constructor)]
  pub fn new(
    reference: String,
    #[wasm_bindgen(js_name = "referenceInboxId")] reference_inbox_id: Option<String>,
    action: ReactionAction,
    content: String,
    schema: ReactionSchema,
  ) -> Self {
    Self {
      reference,
      reference_inbox_id,
      action,
      content,
      schema,
    }
  }
}

#[wasm_bindgen(js_name = "encodeReaction")]
pub fn encode_reaction(reaction: Reaction) -> Result<Uint8Array, JsError> {
  // Use ReactionCodec to encode the reaction
  let encoded =
    ReactionCodec::encode(reaction.into()).map_err(|e| JsError::new(&format!("{}", e)))?;

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
