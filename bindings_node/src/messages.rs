use napi::bindgen_prelude::Uint8Array;
use prost::Message;
use xmtp_mls::storage::group_message::{DeliveryStatus, GroupMessageKind, StoredGroupMessage};

use napi_derive::napi;
use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

use crate::encoded_content::NapiEncodedContent;

#[napi]
pub enum NapiGroupMessageKind {
  Application,
  MembershipChange,
}

impl From<GroupMessageKind> for NapiGroupMessageKind {
  fn from(kind: GroupMessageKind) -> Self {
    match kind {
      GroupMessageKind::Application => NapiGroupMessageKind::Application,
      GroupMessageKind::MembershipChange => NapiGroupMessageKind::MembershipChange,
    }
  }
}

#[napi]
pub enum NapiDeliveryStatus {
  Unpublished,
  Published,
  Failed,
}

impl From<DeliveryStatus> for NapiDeliveryStatus {
  fn from(status: DeliveryStatus) -> Self {
    match status {
      DeliveryStatus::Unpublished => NapiDeliveryStatus::Unpublished,
      DeliveryStatus::Published => NapiDeliveryStatus::Published,
      DeliveryStatus::Failed => NapiDeliveryStatus::Failed,
    }
  }
}

impl From<NapiDeliveryStatus> for DeliveryStatus {
  fn from(status: NapiDeliveryStatus) -> Self {
    match status {
      NapiDeliveryStatus::Unpublished => DeliveryStatus::Unpublished,
      NapiDeliveryStatus::Published => DeliveryStatus::Published,
      NapiDeliveryStatus::Failed => DeliveryStatus::Failed,
    }
  }
}

#[napi(object)]
pub struct NapiListMessagesOptions {
  pub sent_before_ns: Option<i64>,
  pub sent_after_ns: Option<i64>,
  pub limit: Option<i64>,
  pub delivery_status: Option<NapiDeliveryStatus>,
}

#[napi]
pub struct NapiMessage {
  pub id: String,
  pub sent_at_ns: i64,
  pub convo_id: String,
  pub sender_inbox_id: String,
  pub content: NapiEncodedContent,
  pub kind: NapiGroupMessageKind,
  pub delivery_status: NapiDeliveryStatus,
}

impl From<StoredGroupMessage> for NapiMessage {
  fn from(msg: StoredGroupMessage) -> Self {
    let id = hex::encode(msg.id.clone());
    let convo_id = hex::encode(msg.group_id.clone());
    let contents = msg.decrypted_message_bytes.clone();
    let content: NapiEncodedContent = match EncodedContent::decode(contents.as_slice()) {
      Ok(value) => value.into(),
      Err(e) => {
        println!("Error decoding content: {:?}", e);
        NapiEncodedContent {
          r#type: None,
          parameters: Default::default(),
          fallback: None,
          compression: None,
          content: Uint8Array::new(vec![]),
        }
      }
    };

    Self {
      id,
      sent_at_ns: msg.sent_at_ns,
      convo_id,
      sender_inbox_id: msg.sender_inbox_id,
      content,
      kind: msg.kind.into(),
      delivery_status: msg.delivery_status.into(),
    }
  }
}
