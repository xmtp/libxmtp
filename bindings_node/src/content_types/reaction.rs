use napi::bindgen_prelude::{Result, Uint8Array};
use napi_derive::napi;
use prost::Message;
use xmtp_content_types::ContentCodec;
use xmtp_content_types::reaction::ReactionCodec;
use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

use crate::ErrorWrapper;

#[napi]
#[derive(Clone, Default, PartialEq, Debug)]
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

#[napi]
#[derive(Clone, Default, PartialEq)]
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

#[derive(Clone)]
#[napi(object)]
pub struct Reaction {
  pub action: ReactionAction,
  pub content: String,
  pub reference_inbox_id: Option<String>,
  pub reference: String,
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

#[napi]
pub fn encode_reaction(reaction: Reaction) -> Result<Uint8Array> {
  // Use ReactionCodec to encode the reaction
  let encoded = ReactionCodec::encode(reaction.into()).map_err(ErrorWrapper::from)?;

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
