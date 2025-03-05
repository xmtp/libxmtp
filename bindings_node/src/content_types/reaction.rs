use napi::bindgen_prelude::{Result, Uint8Array};
use napi_derive::napi;
use prost::Message;
use xmtp_content_types::reaction::ReactionCodec;
use xmtp_content_types::ContentCodec;
use xmtp_proto::xmtp::mls::message_contents::content_types::ReactionV2;
use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

use crate::ErrorWrapper;

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

#[napi]
pub fn encode_reaction(reaction: Reaction) -> Result<Uint8Array> {
  // Convert Reaction to Reaction
  let reaction: ReactionV2 = reaction.into();

  // Use ReactionCodec to encode the reaction
  let encoded = ReactionCodec::encode(reaction).map_err(ErrorWrapper::from)?;

  // Encode the EncodedContent to bytes
  let mut buf = Vec::new();
  encoded.encode(&mut buf).map_err(ErrorWrapper::from)?;

  Ok(Uint8Array::from(buf.as_slice()))
}

#[napi]
pub fn decode_reaction(bytes: Uint8Array) -> Result<Reaction> {
  // Decode bytes into EncodedContent
  let encoded_content =
    EncodedContent::decode(bytes.to_vec().as_slice()).map_err(ErrorWrapper::from)?;

  // Use ReactionCodec to decode into Reaction and convert to Reaction
  ReactionCodec::decode(encoded_content)
    .map(Into::into)
    .map_err(|e| napi::Error::from_reason(e.to_string()))
}

#[napi]
#[derive(Default, PartialEq, Debug)]
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
#[derive(Default, PartialEq)]
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
