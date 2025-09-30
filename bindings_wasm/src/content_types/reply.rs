use super::decoded_message_content::DecodedMessageContent;
use crate::enriched_message::DecodedMessage;
use wasm_bindgen::prelude::wasm_bindgen;

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
