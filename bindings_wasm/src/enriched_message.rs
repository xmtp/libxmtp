use serde::{Deserialize, Serialize};
use tsify::Tsify;
use wasm_bindgen::JsError;
use xmtp_mls::messages::decoded_message::DecodedMessage as XmtpDecodedMessage;

use crate::content_types::decoded_message_content::DecodedMessageContent;
use crate::encoded_content::ContentTypeId;
use crate::messages::{DeliveryStatus, GroupMessageKind};

#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(
  into_wasm_abi,
  from_wasm_abi,
  large_number_types_as_bigints,
  hashmap_as_object
)]
#[serde(rename_all = "camelCase")]
pub struct DecodedMessage {
  pub id: String,
  pub sent_at_ns: i64,
  pub kind: GroupMessageKind,
  pub sender_installation_id: String,
  pub sender_inbox_id: String,
  pub content_type: ContentTypeId,
  pub conversation_id: String,
  pub content: DecodedMessageContent,
  #[serde(skip_serializing_if = "Option::is_none")]
  #[tsify(optional)]
  pub fallback: Option<String>,
  pub reactions: Vec<DecodedMessage>,
  pub delivery_status: DeliveryStatus,
  pub num_replies: i64,
  pub expires_at_ns: Option<i64>,
}

impl TryFrom<XmtpDecodedMessage> for DecodedMessage {
  type Error = JsError;

  fn try_from(msg: XmtpDecodedMessage) -> Result<Self, Self::Error> {
    let content = msg.content.try_into()?;
    let reactions: Result<Vec<_>, _> = msg.reactions.into_iter().map(|r| r.try_into()).collect();

    Ok(Self {
      id: hex::encode(msg.metadata.id),
      sent_at_ns: msg.metadata.sent_at_ns,
      kind: msg.metadata.kind.into(),
      sender_installation_id: hex::encode(msg.metadata.sender_installation_id),
      sender_inbox_id: msg.metadata.sender_inbox_id,
      content_type: msg.metadata.content_type.into(),
      conversation_id: hex::encode(msg.metadata.group_id),
      content,
      fallback: msg.fallback_text,
      reactions: reactions?,
      delivery_status: msg.metadata.delivery_status.into(),
      num_replies: msg.num_replies as i64,
      expires_at_ns: msg.metadata.expires_at_ns,
    })
  }
}
