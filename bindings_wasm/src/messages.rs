use bindings_wasm_macros::wasm_bindgen_numbered_enum;
use prost::Message as ProstMessage;
use serde::{Deserialize, Serialize};
use tsify::Tsify;
use xmtp_db::group_message::{
  DeliveryStatus as XmtpDeliveryStatus, GroupMessageKind as XmtpGroupMessageKind, MsgQueryArgs,
  SortBy as XmtpMessageSortBy, SortDirection as XmtpSortDirection, StoredGroupMessage,
};
use xmtp_proto::xmtp::mls::message_contents::EncodedContent as XmtpEncodedContent;

use crate::content_types::ContentType;
use crate::encoded_content::EncodedContent;

#[wasm_bindgen_numbered_enum]
pub enum GroupMessageKind {
  Application = 0,
  MembershipChange = 1,
}

impl From<XmtpGroupMessageKind> for GroupMessageKind {
  fn from(kind: XmtpGroupMessageKind) -> Self {
    match kind {
      XmtpGroupMessageKind::Application => GroupMessageKind::Application,
      XmtpGroupMessageKind::MembershipChange => GroupMessageKind::MembershipChange,
    }
  }
}

impl From<GroupMessageKind> for XmtpGroupMessageKind {
  fn from(kind: GroupMessageKind) -> Self {
    match kind {
      GroupMessageKind::Application => XmtpGroupMessageKind::Application,
      GroupMessageKind::MembershipChange => XmtpGroupMessageKind::MembershipChange,
    }
  }
}

#[wasm_bindgen_numbered_enum]
pub enum DeliveryStatus {
  Unpublished = 0,
  Published = 1,
  Failed = 2,
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

#[wasm_bindgen_numbered_enum]
pub enum SortDirection {
  Ascending = 0,
  Descending = 1,
}

impl From<SortDirection> for XmtpSortDirection {
  fn from(direction: SortDirection) -> Self {
    match direction {
      SortDirection::Ascending => XmtpSortDirection::Ascending,
      SortDirection::Descending => XmtpSortDirection::Descending,
    }
  }
}

#[wasm_bindgen_numbered_enum]
pub enum MessageSortBy {
  SentAt = 0,
  InsertedAt = 1,
}

impl From<MessageSortBy> for XmtpMessageSortBy {
  fn from(sort_by: MessageSortBy) -> Self {
    match sort_by {
      MessageSortBy::SentAt => XmtpMessageSortBy::SentAt,
      MessageSortBy::InsertedAt => XmtpMessageSortBy::InsertedAt,
    }
  }
}

#[derive(Clone, Default, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi, large_number_types_as_bigints)]
#[serde(rename_all = "camelCase")]
pub struct ListMessagesOptions {
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub content_types: Option<Vec<ContentType>>,
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub exclude_content_types: Option<Vec<ContentType>>,
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub sent_before_ns: Option<i64>,
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub sent_after_ns: Option<i64>,
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub limit: Option<i64>,
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub delivery_status: Option<DeliveryStatus>,
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub direction: Option<SortDirection>,
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub kind: Option<GroupMessageKind>,
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub exclude_sender_inbox_ids: Option<Vec<String>>,
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub sort_by: Option<MessageSortBy>,
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub inserted_after_ns: Option<i64>,
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub inserted_before_ns: Option<i64>,
}

impl From<ListMessagesOptions> for MsgQueryArgs {
  fn from(opts: ListMessagesOptions) -> MsgQueryArgs {
    MsgQueryArgs {
      sent_before_ns: opts.sent_before_ns,
      sent_after_ns: opts.sent_after_ns,
      delivery_status: opts.delivery_status.map(Into::into),
      limit: opts.limit,
      direction: opts.direction.map(Into::into),
      kind: opts.kind.map(Into::into),
      exclude_content_types: opts
        .exclude_content_types
        .map(|t| t.into_iter().map(Into::into).collect()),
      content_types: opts
        .content_types
        .map(|t| t.into_iter().map(Into::into).collect()),
      exclude_sender_inbox_ids: opts.exclude_sender_inbox_ids,
      sort_by: opts.sort_by.map(Into::into),
      inserted_after_ns: opts.inserted_after_ns,
      inserted_before_ns: opts.inserted_before_ns,
      exclude_disappearing: false,
    }
  }
}

#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi, large_number_types_as_bigints)]
#[serde(rename_all = "camelCase")]
pub struct Message {
  pub id: String,
  pub sent_at_ns: i64,
  pub convo_id: String,
  pub sender_inbox_id: String,
  pub content: EncodedContent,
  pub kind: GroupMessageKind,
  pub delivery_status: DeliveryStatus,
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
          content: Vec::new(),
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
