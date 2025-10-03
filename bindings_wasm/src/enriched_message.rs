use wasm_bindgen::prelude::*;
use xmtp_mls::messages::decoded_message::DecodedMessage as XmtpDecodedMessage;

use crate::content_types::decoded_message_content::DecodedMessageContent;
use crate::encoded_content::ContentTypeId;
use crate::messages::{DeliveryStatus, GroupMessageKind};

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
pub struct DecodedMessage {
  pub id: Vec<u8>,
  pub sent_at_ns: i64,
  pub kind: GroupMessageKind,
  pub sender_installation_id: Vec<u8>,
  pub sender_inbox_id: String,
  pub content_type: ContentTypeId,
  pub conversation_id: Vec<u8>,
  pub content: DecodedMessageContent,
  pub fallback_text: Option<String>,
  pub reactions: Vec<DecodedMessage>,
  pub delivery_status: DeliveryStatus,
  pub num_replies: i64,
}

impl From<XmtpDecodedMessage> for DecodedMessage {
  fn from(msg: XmtpDecodedMessage) -> Self {
    Self {
      id: msg.metadata.id,
      sent_at_ns: msg.metadata.sent_at_ns,
      kind: msg.metadata.kind.into(),
      sender_installation_id: msg.metadata.sender_installation_id,
      sender_inbox_id: msg.metadata.sender_inbox_id,
      content_type: msg.metadata.content_type.into(),
      conversation_id: msg.metadata.group_id,
      content: msg.content.into(),
      fallback_text: msg.fallback_text,
      reactions: msg.reactions.into_iter().map(|r| r.into()).collect(),
      delivery_status: msg.metadata.delivery_status.into(),
      num_replies: msg.num_replies as i64,
    }
  }
}
