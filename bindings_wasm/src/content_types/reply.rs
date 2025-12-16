use super::decoded_message_content::DecodedMessageContent;
use crate::encoded_content::{ContentTypeId, EncodedContent};
use crate::enriched_message::DecodedMessage;
use serde::{Deserialize, Serialize};
use tsify::Tsify;
use wasm_bindgen::prelude::wasm_bindgen;
use xmtp_content_types::ContentCodec;
use xmtp_content_types::reply::ReplyCodec;

#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct EnrichedReply {
  pub reference_id: String,
  pub content: Box<DecodedMessageContent>,
  #[serde(skip_serializing_if = "Option::is_none")]
  #[tsify(optional)]
  pub in_reply_to: Option<Box<DecodedMessage>>,
}

impl From<xmtp_mls::messages::decoded_message::Reply> for EnrichedReply {
  fn from(reply: xmtp_mls::messages::decoded_message::Reply) -> Self {
    // Note: We need to handle the TryFrom for DecodedMessageContent
    // For now, we unwrap - a more robust solution would propagate the error
    let content = reply
      .content
      .as_ref()
      .clone()
      .try_into()
      .expect("Failed to convert message content");
    let in_reply_to = reply.in_reply_to.map(|m| {
      let msg: DecodedMessage = (*m)
        .try_into()
        .expect("Failed to convert in_reply_to message");
      Box::new(msg)
    });

    Self {
      reference_id: reply.reference_id,
      content: Box::new(content),
      in_reply_to,
    }
  }
}

#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct Reply {
  pub content: EncodedContent,
  pub reference: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  #[tsify(optional)]
  pub reference_inbox_id: Option<String>,
}

impl From<xmtp_content_types::reply::Reply> for Reply {
  fn from(reply: xmtp_content_types::reply::Reply) -> Self {
    Self {
      content: reply.content.into(),
      reference: reply.reference,
      reference_inbox_id: reply.reference_inbox_id,
    }
  }
}

impl From<Reply> for xmtp_content_types::reply::Reply {
  fn from(reply: Reply) -> Self {
    Self {
      content: reply.content.into(),
      reference: reply.reference,
      reference_inbox_id: reply.reference_inbox_id,
    }
  }
}

#[wasm_bindgen(js_name = "replyContentType")]
pub fn reply_content_type() -> ContentTypeId {
  ReplyCodec::content_type().into()
}
