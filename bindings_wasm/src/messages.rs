use js_sys::Uint8Array;
use prost::Message;
use wasm_bindgen::prelude::wasm_bindgen;
use xmtp_mls::storage::group_message::{DeliveryStatus, GroupMessageKind, StoredGroupMessage};
use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

use crate::encoded_content::WasmEncodedContent;

#[wasm_bindgen]
#[derive(Clone)]
pub enum WasmGroupMessageKind {
  Application,
  MembershipChange,
}

impl From<GroupMessageKind> for WasmGroupMessageKind {
  fn from(kind: GroupMessageKind) -> Self {
    match kind {
      GroupMessageKind::Application => WasmGroupMessageKind::Application,
      GroupMessageKind::MembershipChange => WasmGroupMessageKind::MembershipChange,
    }
  }
}

#[wasm_bindgen]
#[derive(Clone)]
pub enum WasmDeliveryStatus {
  Unpublished,
  Published,
  Failed,
}

impl From<DeliveryStatus> for WasmDeliveryStatus {
  fn from(status: DeliveryStatus) -> Self {
    match status {
      DeliveryStatus::Unpublished => WasmDeliveryStatus::Unpublished,
      DeliveryStatus::Published => WasmDeliveryStatus::Published,
      DeliveryStatus::Failed => WasmDeliveryStatus::Failed,
    }
  }
}

impl From<WasmDeliveryStatus> for DeliveryStatus {
  fn from(status: WasmDeliveryStatus) -> Self {
    match status {
      WasmDeliveryStatus::Unpublished => DeliveryStatus::Unpublished,
      WasmDeliveryStatus::Published => DeliveryStatus::Published,
      WasmDeliveryStatus::Failed => DeliveryStatus::Failed,
    }
  }
}

#[wasm_bindgen(getter_with_clone)]
pub struct WasmListMessagesOptions {
  pub sent_before_ns: Option<i64>,
  pub sent_after_ns: Option<i64>,
  pub limit: Option<i64>,
  pub delivery_status: Option<WasmDeliveryStatus>,
}

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
pub struct WasmMessage {
  pub id: String,
  pub sent_at_ns: i64,
  pub convo_id: String,
  pub sender_inbox_id: String,
  pub content: WasmEncodedContent,
  pub kind: WasmGroupMessageKind,
  pub delivery_status: WasmDeliveryStatus,
}

impl From<StoredGroupMessage> for WasmMessage {
  fn from(msg: StoredGroupMessage) -> Self {
    let id = hex::encode(msg.id.clone());
    let convo_id = hex::encode(msg.group_id.clone());
    let contents = msg.decrypted_message_bytes.clone();
    let content: WasmEncodedContent = match EncodedContent::decode(contents.as_slice()) {
      Ok(value) => value.into(),
      Err(e) => {
        println!("Error decoding content: {:?}", e);
        WasmEncodedContent {
          r#type: None,
          parameters: Default::default(),
          fallback: None,
          compression: None,
          content: Uint8Array::new_with_length(0),
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
