use wasm_bindgen::prelude::*;
use xmtp_mls::messages::decoded_message::DecodedMessage as XmtpDecodedMessage;

use crate::content_types::decoded_message_content::DecodedMessageContent;
use crate::encoded_content::ContentTypeId;
use crate::messages::{DeliveryStatus, GroupMessageKind};

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
pub struct DecodedMessage {
  pub id: Vec<u8>,
  #[wasm_bindgen(js_name = sentAtNs)]
  pub sent_at_ns: i64,
  pub kind: GroupMessageKind,
  #[wasm_bindgen(js_name = senderInstallationId)]
  pub sender_installation_id: Vec<u8>,
  #[wasm_bindgen(js_name = senderInboxId)]
  pub sender_inbox_id: String,
  #[wasm_bindgen(js_name = contentType)]
  pub content_type: ContentTypeId,
  #[wasm_bindgen(js_name = conversationId)]
  pub conversation_id: Vec<u8>,
  pub content: DecodedMessageContent,
  #[wasm_bindgen(js_name = fallbackText)]
  pub fallback_text: Option<String>,
  pub reactions: Vec<DecodedMessage>,
  #[wasm_bindgen(js_name = deliveryStatus)]
  pub delivery_status: DeliveryStatus,
  #[wasm_bindgen(js_name = numReplies)]
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
