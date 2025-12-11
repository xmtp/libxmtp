use super::decoded_message_content::DecodedMessageContent;
use crate::encoded_content::EncodedContent;
use crate::enriched_message::DecodedMessage;
use js_sys::Uint8Array;
use prost::Message;
use wasm_bindgen::{JsError, prelude::wasm_bindgen};
use xmtp_content_types::ContentCodec;
use xmtp_content_types::reply::ReplyCodec;

#[wasm_bindgen]
#[derive(Clone)]
pub struct EnrichedReply {
  in_reply_to_msg: Option<DecodedMessage>,
  content: DecodedMessageContent,
  reference_id: String,
}

#[wasm_bindgen]
impl EnrichedReply {
  #[wasm_bindgen(getter, js_name = "referenceId")]
  pub fn reference_id(&self) -> String {
    self.reference_id.clone()
  }

  #[wasm_bindgen(getter)]
  pub fn content(&self) -> DecodedMessageContent {
    self.content.clone()
  }

  #[wasm_bindgen(getter, js_name = "inReplyTo")]
  pub fn in_reply_to(&self) -> Option<DecodedMessage> {
    self.in_reply_to_msg.clone()
  }
}

impl From<xmtp_mls::messages::decoded_message::Reply> for EnrichedReply {
  fn from(reply: xmtp_mls::messages::decoded_message::Reply) -> Self {
    Self {
      in_reply_to_msg: reply.in_reply_to.map(|m| (*m).into()),
      content: reply.content.as_ref().clone().into(),
      reference_id: reply.reference_id,
    }
  }
}

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
pub struct Reply {
  pub content: EncodedContent,
  pub reference: String,
  #[wasm_bindgen(js_name = "referenceInboxId")]
  pub reference_inbox_id: Option<String>,
}

#[wasm_bindgen]
impl Reply {
  #[wasm_bindgen(constructor)]
  pub fn new(
    content: EncodedContent,
    reference: String,
    #[wasm_bindgen(js_name = "referenceInboxId")] reference_inbox_id: Option<String>,
  ) -> Self {
    Self {
      content,
      reference,
      reference_inbox_id,
    }
  }
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

#[wasm_bindgen(js_name = "encodeReply")]
pub fn encode_reply(reply: Reply) -> Result<Uint8Array, JsError> {
  // Convert Reply to xmtp_content_types::reply::Reply
  let encoded = ReplyCodec::encode(reply.into()).map_err(|e| JsError::new(&format!("{}", e)))?;

  // Encode the EncodedContent to bytes
  let mut buf = Vec::new();
  encoded
    .encode(&mut buf)
    .map_err(|e| JsError::new(&format!("{}", e)))?;

  Ok(Uint8Array::from(buf.as_slice()))
}

#[wasm_bindgen(js_name = "decodeReply")]
pub fn decode_reply(encoded_content: EncodedContent) -> Result<Reply, JsError> {
  // Use ReplyCodec to decode and convert to Reply
  ReplyCodec::decode(encoded_content.into())
    .map(Into::into)
    .map_err(|e| JsError::new(&format!("{}", e)))
}
