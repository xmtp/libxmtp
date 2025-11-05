use napi_derive::napi;

use crate::enriched_message::DecodedMessage as NodeDecodedMessage;

use super::decoded_message_body::DecodedMessageBody;
use xmtp_mls::messages::decoded_message::DecodedMessage as RustDecodedMessage;

#[derive(Clone)]
#[napi]
pub struct EnrichedReply {
  in_reply_to: Option<Box<RustDecodedMessage>>,
  content: DecodedMessageBody,
  reference_id: String,
}

#[napi]
impl EnrichedReply {
  #[napi(getter)]
  pub fn reference_id(&self) -> String {
    self.reference_id.clone()
  }

  #[napi(getter)]
  pub fn content(&self) -> DecodedMessageBody {
    self.content.clone()
  }

  #[napi(getter)]
  pub fn in_reply_to(&self) -> Option<NodeDecodedMessage> {
    self.in_reply_to.clone().map(|m| (*m).into())
  }
}

impl From<xmtp_mls::messages::decoded_message::Reply> for EnrichedReply {
  fn from(reply: xmtp_mls::messages::decoded_message::Reply) -> Self {
    Self {
      in_reply_to: reply.in_reply_to,
      content: reply.content.as_ref().clone().into(),
      reference_id: reply.reference_id,
    }
  }
}
