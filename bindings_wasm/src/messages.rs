use js_sys::Uint8Array;
use prost::Message as ProstMessage;
use wasm_bindgen::prelude::wasm_bindgen;
use xmtp_mls::storage::group_message::{
  DeliveryStatus as XmtpDeliveryStatus, GroupMessageKind as XmtpGroupMessageKind, MsgQueryArgs,
  SortDirection as XmtpSortDirection, StoredGroupMessage,
};
use xmtp_proto::xmtp::mls::message_contents::EncodedContent as XmtpEncodedContent;

use crate::encoded_content::EncodedContent;

#[wasm_bindgen]
#[derive(Clone)]
pub enum GroupMessageKind {
  Application,
  MembershipChange,
}

impl From<XmtpGroupMessageKind> for GroupMessageKind {
  fn from(kind: XmtpGroupMessageKind) -> Self {
    match kind {
      XmtpGroupMessageKind::Application => GroupMessageKind::Application,
      XmtpGroupMessageKind::MembershipChange => GroupMessageKind::MembershipChange,
    }
  }
}

#[wasm_bindgen]
#[derive(Clone)]
pub enum DeliveryStatus {
  Unpublished,
  Published,
  Failed,
}

impl From<XmtpDeliveryStatus> for DeliveryStatus {
  fn from(status: XmtpDeliveryStatus) -> Self {
    match status {
      XmtpDeliveryStatus::Unpublished => DeliveryStatus::Unpublished,
      XmtpDeliveryStatus::Published => DeliveryStatus::Published,
      XmtpDeliveryStatus::Failed => DeliveryStatus::Failed,
    }
  }
}

impl From<DeliveryStatus> for XmtpDeliveryStatus {
  fn from(status: DeliveryStatus) -> Self {
    match status {
      DeliveryStatus::Unpublished => XmtpDeliveryStatus::Unpublished,
      DeliveryStatus::Published => XmtpDeliveryStatus::Published,
      DeliveryStatus::Failed => XmtpDeliveryStatus::Failed,
    }
  }
}

#[wasm_bindgen]
#[derive(Clone)]
pub enum SortDirection {
  Ascending,
  Descending,
}

impl From<SortDirection> for XmtpSortDirection {
  fn from(direction: SortDirection) -> Self {
    match direction {
      SortDirection::Ascending => XmtpSortDirection::Ascending,
      SortDirection::Descending => XmtpSortDirection::Descending,
    }
  }
}

#[wasm_bindgen(getter_with_clone)]
#[derive(Default)]
pub struct ListMessagesOptions {
  #[wasm_bindgen(js_name = sentBeforeNs)]
  pub sent_before_ns: Option<i64>,
  #[wasm_bindgen(js_name = sentAfterNs)]
  pub sent_after_ns: Option<i64>,
  pub limit: Option<i64>,
  #[wasm_bindgen(js_name = deliveryStatus)]
  pub delivery_status: Option<DeliveryStatus>,
  pub direction: Option<SortDirection>,
}

impl From<ListMessagesOptions> for MsgQueryArgs {
  fn from(opts: ListMessagesOptions) -> MsgQueryArgs {
    let delivery_status = opts.delivery_status.map(Into::into);
    let direction = opts.direction.map(Into::into);

    MsgQueryArgs {
      sent_before_ns: opts.sent_before_ns,
      sent_after_ns: opts.sent_after_ns,
      delivery_status,
      limit: opts.limit,
      direction,
      ..Default::default()
    }
  }
}

#[wasm_bindgen]
impl ListMessagesOptions {
  #[wasm_bindgen(constructor)]
  pub fn new(
    sent_before_ns: Option<i64>,
    sent_after_ns: Option<i64>,
    limit: Option<i64>,
    delivery_status: Option<DeliveryStatus>,
    direction: Option<SortDirection>,
  ) -> Self {
    Self {
      sent_before_ns,
      sent_after_ns,
      limit,
      delivery_status,
      direction,
    }
  }
}

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
pub struct Message {
  pub id: String,
  #[wasm_bindgen(js_name = sentAtNs)]
  pub sent_at_ns: i64,
  #[wasm_bindgen(js_name = convoId)]
  pub convo_id: String,
  #[wasm_bindgen(js_name = senderInboxId)]
  pub sender_inbox_id: String,
  pub content: EncodedContent,
  pub kind: GroupMessageKind,
  #[wasm_bindgen(js_name = deliveryStatus)]
  pub delivery_status: DeliveryStatus,
}

#[wasm_bindgen]
impl Message {
  #[wasm_bindgen(constructor)]
  pub fn new(
    id: String,
    sent_at_ns: i64,
    convo_id: String,
    sender_inbox_id: String,
    content: EncodedContent,
    kind: GroupMessageKind,
    delivery_status: DeliveryStatus,
  ) -> Self {
    Self {
      id,
      sent_at_ns,
      convo_id,
      sender_inbox_id,
      content,
      kind,
      delivery_status,
    }
  }
}

impl From<StoredGroupMessage> for Message {
  fn from(msg: StoredGroupMessage) -> Self {
    let id = hex::encode(msg.id.clone());
    let convo_id = hex::encode(msg.group_id.clone());
    let contents = msg.decrypted_message_bytes.clone();
    let content: EncodedContent = match XmtpEncodedContent::decode(contents.as_slice()) {
      Ok(value) => value.into(),
      Err(e) => {
        println!("Error decoding content: {:?}", e);
        EncodedContent {
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
