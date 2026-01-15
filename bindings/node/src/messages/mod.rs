use crate::{content_types::ContentType, messages::encoded_content::EncodedContent};
use napi::bindgen_prelude::BigInt;
use napi_derive::napi;
use prost::Message as ProstMessage;
use xmtp_db::group_message::{
  DeliveryStatus as XmtpDeliveryStatus, GroupMessageKind as XmtpGroupMessageKind, MsgQueryArgs,
  SortBy as XmtpMessageSortBy, SortDirection as XmtpSortDirection, StoredGroupMessage,
};
use xmtp_proto::xmtp::mls::message_contents::EncodedContent as XmtpEncodedContent;

pub mod decoded_message;
pub mod encoded_content;

#[napi]
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

impl From<GroupMessageKind> for XmtpGroupMessageKind {
  fn from(kind: GroupMessageKind) -> Self {
    match kind {
      GroupMessageKind::Application => XmtpGroupMessageKind::Application,
      GroupMessageKind::MembershipChange => XmtpGroupMessageKind::MembershipChange,
    }
  }
}

#[napi]
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

#[napi]
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

#[napi]
pub enum MessageSortBy {
  SentAt,
  InsertedAt,
}

impl From<MessageSortBy> for XmtpMessageSortBy {
  fn from(sort_by: MessageSortBy) -> Self {
    match sort_by {
      MessageSortBy::SentAt => XmtpMessageSortBy::SentAt,
      MessageSortBy::InsertedAt => XmtpMessageSortBy::InsertedAt,
    }
  }
}

#[napi(object)]
#[derive(Default)]
pub struct ListMessagesOptions {
  pub sent_before_ns: Option<BigInt>,
  pub sent_after_ns: Option<BigInt>,
  pub limit: Option<i64>,
  pub delivery_status: Option<DeliveryStatus>,
  pub direction: Option<SortDirection>,
  pub content_types: Option<Vec<ContentType>>,
  pub exclude_content_types: Option<Vec<ContentType>>,
  pub kind: Option<GroupMessageKind>,
  pub exclude_sender_inbox_ids: Option<Vec<String>>,
  pub sort_by: Option<MessageSortBy>,
  pub inserted_after_ns: Option<BigInt>,
  pub inserted_before_ns: Option<BigInt>,
}

impl From<ListMessagesOptions> for MsgQueryArgs {
  fn from(opts: ListMessagesOptions) -> MsgQueryArgs {
    let delivery_status = opts.delivery_status.map(Into::into);
    let direction = opts.direction.map(Into::into);
    let content_types = opts
      .content_types
      .map(|types| types.into_iter().map(Into::into).collect());
    let exclude_content_types = opts
      .exclude_content_types
      .map(|types| types.into_iter().map(Into::into).collect());

    MsgQueryArgs {
      sent_before_ns: opts.sent_before_ns.map(|v| v.get_i64().0),
      sent_after_ns: opts.sent_after_ns.map(|v| v.get_i64().0),
      delivery_status,
      limit: opts.limit,
      direction,
      content_types,
      exclude_content_types,
      kind: opts.kind.map(Into::into),
      exclude_sender_inbox_ids: opts.exclude_sender_inbox_ids,
      sort_by: opts.sort_by.map(Into::into),
      inserted_after_ns: opts.inserted_after_ns.map(|v| v.get_i64().0),
      inserted_before_ns: opts.inserted_before_ns.map(|v| v.get_i64().0),
      exclude_disappearing: false,
    }
  }
}

#[napi(object)]
#[derive(Clone)]
pub struct Message {
  pub id: String,
  pub sent_at_ns: BigInt,
  pub convo_id: String,
  pub sender_inbox_id: String,
  pub content: EncodedContent,
  pub kind: GroupMessageKind,
  pub delivery_status: DeliveryStatus,
  pub inserted_at_ns: BigInt,
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
          content: vec![].into(),
        }
      }
    };

    Self {
      id,
      sent_at_ns: BigInt::from(msg.sent_at_ns),
      convo_id,
      sender_inbox_id: msg.sender_inbox_id,
      content,
      kind: msg.kind.into(),
      delivery_status: msg.delivery_status.into(),
      inserted_at_ns: BigInt::from(msg.inserted_at_ns),
    }
  }
}
