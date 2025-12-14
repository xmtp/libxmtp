use crate::encoded_content::{ContentTypeId, EncodedContent};
use wasm_bindgen::{JsError, prelude::wasm_bindgen};
use xmtp_content_types::ContentCodec;
use xmtp_content_types::reaction::ReactionCodec as XmtpReactionCodec;
use xmtp_proto::xmtp::mls::message_contents::content_types::ReactionV2;

#[wasm_bindgen(getter_with_clone)]
pub struct Reaction {
  pub reference: String,
  #[wasm_bindgen(js_name = "referenceInboxId")]
  pub reference_inbox_id: String,
  pub action: ReactionAction,
  pub content: String,
  pub schema: ReactionSchema,
}

#[wasm_bindgen]
impl Reaction {
  #[wasm_bindgen(constructor)]
  pub fn new(
    reference: String,
    #[wasm_bindgen(js_name = "referenceInboxId")] reference_inbox_id: String,
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

#[wasm_bindgen]
pub struct ReactionCodec;

#[wasm_bindgen]
impl ReactionCodec {
  #[wasm_bindgen(js_name = "contentType")]
  pub fn content_type() -> ContentTypeId {
    XmtpReactionCodec::content_type().into()
  }

  #[wasm_bindgen]
  pub fn encode(reaction: Reaction) -> Result<EncodedContent, JsError> {
    let encoded_content =
      XmtpReactionCodec::encode(reaction.into()).map_err(|e| JsError::new(&format!("{}", e)))?;
    Ok(encoded_content.into())
  }

  #[wasm_bindgen]
  pub fn decode(encoded_content: EncodedContent) -> Result<Reaction, JsError> {
    XmtpReactionCodec::decode(encoded_content.into())
      .map(Into::into)
      .map_err(|e| JsError::new(&format!("{}", e)))
  }

  #[wasm_bindgen(js_name = "shouldPush")]
  pub fn should_push() -> bool {
    XmtpReactionCodec::should_push()
  }
}

#[wasm_bindgen]
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub enum ReactionAction {
  Unknown,
  #[default]
  Added,
  Removed,
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

#[wasm_bindgen]
#[derive(Copy, Clone, Default, PartialEq)]
pub enum ReactionSchema {
  Unknown,
  #[default]
  Unicode,
  Shortcode,
  Custom,
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

#[wasm_bindgen(getter_with_clone)]
#[derive(Debug, Clone)]
pub struct ReactionPayload {
  pub reference: String,
  #[wasm_bindgen(js_name = "referenceInboxId")]
  pub reference_inbox_id: String,
  pub action: ReactionActionPayload,
  pub content: String,
  pub schema: ReactionSchemaPayload,
}

impl From<ReactionV2> for ReactionPayload {
  fn from(reaction: ReactionV2) -> Self {
    ReactionPayload {
      reference: reaction.reference,
      reference_inbox_id: reaction.reference_inbox_id,
      action: match reaction.action {
        1 => ReactionActionPayload::Added,
        2 => ReactionActionPayload::Removed,
        _ => ReactionActionPayload::Unknown,
      },
      content: reaction.content,
      schema: match reaction.schema {
        1 => ReactionSchemaPayload::Unicode,
        2 => ReactionSchemaPayload::Shortcode,
        3 => ReactionSchemaPayload::Custom,
        _ => ReactionSchemaPayload::Unknown,
      },
    }
  }
}

#[wasm_bindgen]
#[derive(Debug, Clone)]
pub enum ReactionActionPayload {
  Added,
  Removed,
  Unknown,
}

#[wasm_bindgen]
#[derive(Debug, Clone)]
pub enum ReactionSchemaPayload {
  Unicode,
  Shortcode,
  Custom,
  Unknown,
}
