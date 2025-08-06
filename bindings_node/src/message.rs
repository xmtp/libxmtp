use napi::bindgen_prelude::Uint8Array;
use prost::Message as ProstMessage;
use xmtp_db::group_message::{
  DeliveryStatus as XmtpDeliveryStatus, GroupMessageKind as XmtpGroupMessageKind, MsgQueryArgs,
  SortDirection as XmtpSortDirection, StoredGroupMessage, StoredGroupMessageWithReactions,
};

use napi_derive::napi;
use xmtp_proto::xmtp::mls::message_contents::EncodedContent as XmtpEncodedContent;

use crate::{content_types::ContentType, encoded_content::EncodedContent};

#[napi]
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

#[napi(object)]
#[derive(Default)]
pub struct ListMessagesOptions {
  pub sent_before_ns: Option<i64>,
  pub sent_after_ns: Option<i64>,
  pub limit: Option<i64>,
  pub delivery_status: Option<DeliveryStatus>,
  pub direction: Option<SortDirection>,
  pub content_types: Option<Vec<ContentType>>,
  pub kind: Option<GroupMessageKind>,
}

impl From<ListMessagesOptions> for MsgQueryArgs {
  fn from(opts: ListMessagesOptions) -> MsgQueryArgs {
    let delivery_status = opts.delivery_status.map(Into::into);
    let direction = opts.direction.map(Into::into);
    let content_types = opts
      .content_types
      .map(|types| types.into_iter().map(Into::into).collect());

    MsgQueryArgs {
      sent_before_ns: opts.sent_before_ns,
      sent_after_ns: opts.sent_after_ns,
      delivery_status,
      limit: opts.limit,
      direction,
      content_types,
      kind: opts.kind.map(Into::into),
    }
  }
}

#[napi(object)]
#[derive(Clone)]
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

#[napi(object)]
#[derive(Clone)]
pub struct MessageWithReactions {
  pub message: Message,
  pub reactions: Vec<Message>,
}

impl From<StoredGroupMessageWithReactions> for MessageWithReactions {
  fn from(msg_with_reactions: StoredGroupMessageWithReactions) -> Self {
    Self {
      message: msg_with_reactions.message.into(),
      reactions: msg_with_reactions
        .reactions
        .into_iter()
        .map(|reaction| reaction.into())
        .collect(),
    }
  }
}

#[napi(object)]
#[derive(Clone)]
pub struct MessageListItem {
  pub message: Message,
  pub reactions: Vec<Message>,
  pub replies: Vec<Message>,
  #[napi(js_name = "isRead")]
  pub is_read: bool,
}

impl From<xmtp_db::group_message::StoredGroupMessageListItem> for MessageListItem {
  fn from(item: xmtp_db::group_message::StoredGroupMessageListItem) -> Self {
    Self {
      message: item.message.into(),
      reactions: item.reactions.into_iter().map(|msg| msg.into()).collect(),
      replies: item.replies.into_iter().map(|msg| msg.into()).collect(),
      is_read: item.is_read,
    }
  }
}
