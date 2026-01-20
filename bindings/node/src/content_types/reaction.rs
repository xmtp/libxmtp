use crate::ErrorWrapper;
use crate::messages::encoded_content::{ContentTypeId, EncodedContent};
use napi::bindgen_prelude::Result;
use napi_derive::napi;
use xmtp_content_types::ContentCodec;
use xmtp_content_types::reaction::ReactionCodec;
use xmtp_proto::xmtp::mls::message_contents::content_types::ReactionV2;

#[derive(Clone)]
#[napi(object)]
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
    let action = reaction.action().into();
    let schema = reaction.schema().into();
    Reaction {
      reference: reaction.reference,
      reference_inbox_id: reaction.reference_inbox_id,
      action,
      content: reaction.content,
      schema,
    }
  }
}

#[napi]
pub fn content_type_reaction() -> ContentTypeId {
  ReactionCodec::content_type().into()
}

#[napi]
pub fn encode_reaction(reaction: Reaction) -> Result<EncodedContent> {
  let reaction_v2: ReactionV2 = reaction.into();
  Ok(
    ReactionCodec::encode(reaction_v2)
      .map_err(ErrorWrapper::from)?
      .into(),
  )
}

#[napi]
#[derive(Clone, Default, PartialEq, Debug)]
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

#[napi]
#[derive(Clone, Default, PartialEq)]
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

impl From<xmtp_proto::xmtp::mls::message_contents::content_types::ReactionAction>
  for ReactionAction
{
  fn from(action: xmtp_proto::xmtp::mls::message_contents::content_types::ReactionAction) -> Self {
    match action {
      xmtp_proto::xmtp::mls::message_contents::content_types::ReactionAction::Added => {
        ReactionAction::Added
      }
      xmtp_proto::xmtp::mls::message_contents::content_types::ReactionAction::Removed => {
        ReactionAction::Removed
      }
      _ => ReactionAction::Unknown,
    }
  }
}

impl From<xmtp_proto::xmtp::mls::message_contents::content_types::ReactionSchema>
  for ReactionSchema
{
  fn from(schema: xmtp_proto::xmtp::mls::message_contents::content_types::ReactionSchema) -> Self {
    match schema {
      xmtp_proto::xmtp::mls::message_contents::content_types::ReactionSchema::Unicode => {
        ReactionSchema::Unicode
      }
      xmtp_proto::xmtp::mls::message_contents::content_types::ReactionSchema::Shortcode => {
        ReactionSchema::Shortcode
      }
      xmtp_proto::xmtp::mls::message_contents::content_types::ReactionSchema::Custom => {
        ReactionSchema::Custom
      }
      _ => ReactionSchema::Unknown,
    }
  }
}
