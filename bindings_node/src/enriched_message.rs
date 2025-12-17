use napi::Error;
use napi::bindgen_prelude::Result;
use napi_derive::napi;
use xmtp_mls::messages::decoded_message::DecodedMessage as XmtpDecodedMessage;

use crate::content_types::decoded_message_content::DecodedMessageContent;
use crate::encoded_content::ContentTypeId;
use crate::message::{DeliveryStatus, GroupMessageKind};

#[derive(Clone)]
#[napi]
pub struct DecodedMessage {
  inner: Box<XmtpDecodedMessage>,
  pub id: String,
  pub sent_at_ns: i64,
  pub kind: GroupMessageKind,
  pub sender_installation_id: String,
  pub sender_inbox_id: String,
  content_type: ContentTypeId,
  pub conversation_id: String,
  pub fallback: Option<String>,
  pub delivery_status: DeliveryStatus,
  pub num_replies: i64,
  pub expires_at_ns: Option<i64>,
}

#[napi]
impl DecodedMessage {
  #[napi(getter)]
  pub fn reactions(&self) -> Result<Vec<DecodedMessage>> {
    self
      .inner
      .reactions
      .iter()
      .map(|r| r.clone().try_into())
      .collect()
  }

  #[napi(getter)]
  pub fn content_type(&self) -> ContentTypeId {
    self.content_type.clone()
  }

  #[napi(getter)]
  pub fn content(&self) -> Result<DecodedMessageContent> {
    self.inner.content.clone().try_into()
  }
}

impl TryFrom<XmtpDecodedMessage> for DecodedMessage {
  type Error = Error;

  fn try_from(msg: XmtpDecodedMessage) -> Result<Self> {
    Ok(Self {
      id: hex::encode(&msg.metadata.id),
      sent_at_ns: msg.metadata.sent_at_ns,
      kind: msg.metadata.kind.into(),
      sender_installation_id: hex::encode(&msg.metadata.sender_installation_id),
      sender_inbox_id: msg.metadata.sender_inbox_id.clone(),
      content_type: msg.metadata.content_type.clone().into(),
      conversation_id: hex::encode(&msg.metadata.group_id),
      fallback: msg.fallback_text.clone(),
      delivery_status: msg.metadata.delivery_status.into(),
      num_replies: msg.num_replies as i64,
      expires_at_ns: msg.metadata.expires_at_ns,
      inner: Box::new(msg),
    })
  }
}
