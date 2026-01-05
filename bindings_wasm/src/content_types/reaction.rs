use bindings_wasm_macros::wasm_bindgen_numbered_enum;
use serde::{Deserialize, Serialize};
use tsify::Tsify;
use wasm_bindgen::JsError;
use wasm_bindgen::prelude::wasm_bindgen;
use xmtp_content_types::ContentCodec;
use xmtp_content_types::reaction::ReactionCodec;
use xmtp_proto::xmtp::mls::message_contents::content_types::ReactionV2;

use crate::encoded_content::{ContentTypeId, EncodedContent};

#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct Reaction {
  pub reference: String,
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

#[wasm_bindgen(js_name = "contentTypeReaction")]
pub fn content_type_reaction() -> ContentTypeId {
  ReactionCodec::content_type().into()
}

#[wasm_bindgen(js_name = "encodeReaction")]
pub fn encode_reaction(reaction: Reaction) -> Result<EncodedContent, JsError> {
  let reaction_v2: ReactionV2 = reaction.into();
  Ok(
    ReactionCodec::encode(reaction_v2)
      .map_err(|e| JsError::new(&format!("{}", e)))?
      .into(),
  )
}

#[wasm_bindgen_numbered_enum]
#[derive(Default)]
pub enum ReactionAction {
  Unknown = 0,
  #[default]
  Added = 1,
  Removed = 2,
}

impl From<ReactionAction> for i32 {
  fn from(action: ReactionAction) -> Self {
    action as i32
  }
}

#[wasm_bindgen_numbered_enum]
#[derive(Default)]
pub enum ReactionSchema {
  Unknown = 0,
  #[default]
  Unicode = 1,
  Shortcode = 2,
  Custom = 3,
}

impl From<ReactionSchema> for i32 {
  fn from(schema: ReactionSchema) -> Self {
    schema as i32
  }
}
