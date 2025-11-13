use napi_derive::napi;
use xmtp_mls::messages::decoded_message::{DecodedMessage as XmtpDecodedMessage, MessageBody};

use crate::content_types::attachment::Attachment;
use crate::content_types::group_updated::GroupUpdated;
use crate::content_types::multi_remote_attachment::MultiRemoteAttachmentPayload;
use crate::content_types::reaction::Reaction;
use crate::content_types::read_receipt::ReadReceipt;
use crate::content_types::remote_attachment::RemoteAttachment;
use crate::content_types::reply::EnrichedReply;
use crate::content_types::text::TextContent;
use crate::content_types::transaction_reference::TransactionReference;
use crate::content_types::wallet_send_calls::WalletSendCalls;
use crate::encoded_content::{ContentTypeId, EncodedContent};
use crate::message::{DeliveryStatus, GroupMessageKind};

// Because we don't have a way to create an enum for body types, we instead include all possible content types
// on the DecodedMessage struct. The wrappers will clean this up and use union types.

#[derive(Clone)]
#[napi]
pub struct DecodedMessage {
  inner: Box<XmtpDecodedMessage>,
  pub id: Vec<u8>,
  pub sent_at_ns: i64,
  pub kind: GroupMessageKind,
  pub sender_installation_id: Vec<u8>,
  pub sender_inbox_id: String,
  content_type: ContentTypeId,
  pub conversation_id: Vec<u8>,
  pub fallback_text: Option<String>,
  pub delivery_status: DeliveryStatus,
  pub num_replies: i64,
}

#[napi]
impl DecodedMessage {
  // Lazy getter for reactions
  #[napi(getter)]
  pub fn reactions(&self) -> Vec<DecodedMessage> {
    self
      .inner
      .reactions
      .iter()
      .map(|r| r.clone().into())
      .collect()
  }

  #[napi(getter)]
  pub fn content_type(&self) -> ContentTypeId {
    self.content_type.clone()
  }

  // Lazy getter methods for content fields
  #[napi(getter)]
  pub fn text_content(&self) -> Option<TextContent> {
    match &self.inner.content {
      MessageBody::Text(t) => Some(t.clone().into()),
      _ => None,
    }
  }

  #[napi(getter)]
  pub fn reply_content(&self) -> Option<EnrichedReply> {
    match &self.inner.content {
      MessageBody::Reply(r) => Some(r.clone().into()),
      _ => None,
    }
  }

  #[napi(getter)]
  pub fn reaction_content(&self) -> Option<Reaction> {
    match &self.inner.content {
      MessageBody::Reaction(r) => Some(r.clone().into()),
      _ => None,
    }
  }

  #[napi(getter)]
  pub fn attachment_content(&self) -> Option<Attachment> {
    match &self.inner.content {
      MessageBody::Attachment(a) => Some(a.clone().into()),
      _ => None,
    }
  }

  #[napi(getter)]
  pub fn remote_attachment_content(&self) -> Option<RemoteAttachment> {
    match &self.inner.content {
      MessageBody::RemoteAttachment(ra) => Some(ra.clone().into()),
      _ => None,
    }
  }

  #[napi(getter)]
  pub fn multi_remote_attachment_content(&self) -> Option<MultiRemoteAttachmentPayload> {
    match &self.inner.content {
      MessageBody::MultiRemoteAttachment(mra) => Some(mra.clone().into()),
      _ => None,
    }
  }

  #[napi(getter)]
  pub fn transaction_reference_content(&self) -> Option<TransactionReference> {
    match &self.inner.content {
      MessageBody::TransactionReference(tr) => Some(tr.clone().into()),
      _ => None,
    }
  }

  #[napi(getter)]
  pub fn group_updated_content(&self) -> Option<GroupUpdated> {
    match &self.inner.content {
      MessageBody::GroupUpdated(gu) => Some(gu.clone().into()),
      _ => None,
    }
  }

  #[napi(getter)]
  pub fn read_receipt_content(&self) -> Option<ReadReceipt> {
    match &self.inner.content {
      MessageBody::ReadReceipt(rr) => Some(rr.clone().into()),
      _ => None,
    }
  }

  #[napi(getter)]
  pub fn wallet_send_calls_content(&self) -> Option<WalletSendCalls> {
    match &self.inner.content {
      MessageBody::WalletSendCalls(wsc) => Some(wsc.clone().into()),
      _ => None,
    }
  }

  #[napi(getter)]
  pub fn custom_content(&self) -> Option<EncodedContent> {
    match &self.inner.content {
      MessageBody::Custom(c) => Some(c.clone().into()),
      _ => None,
    }
  }
}

impl From<XmtpDecodedMessage> for DecodedMessage {
  fn from(msg: XmtpDecodedMessage) -> Self {
    Self {
      id: msg.metadata.id.clone(),
      sent_at_ns: msg.metadata.sent_at_ns,
      kind: msg.metadata.kind.into(),
      sender_installation_id: msg.metadata.sender_installation_id.clone(),
      sender_inbox_id: msg.metadata.sender_inbox_id.clone(),
      content_type: msg.metadata.content_type.clone().into(),
      conversation_id: msg.metadata.group_id.clone(),
      fallback_text: msg.fallback_text.clone(),
      delivery_status: msg.metadata.delivery_status.into(),
      num_replies: msg.num_replies as i64,
      inner: Box::new(msg),
    }
  }
}
