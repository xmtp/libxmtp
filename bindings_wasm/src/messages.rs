use prost::Message as ProstMessage;
use serde::{Deserialize, Serialize};
use tsify_next::Tsify;
use xmtp_mls::storage::group_message::{
  DeliveryStatus as XmtpDeliveryStatus, GroupMessageKind as XmtpGroupMessageKind, MsgQueryArgs,
  SortDirection as XmtpSortDirection, StoredGroupMessage, StoredGroupMessageWithReactions,
};
use xmtp_proto::xmtp::mls::message_contents::EncodedContent as XmtpEncodedContent;

use crate::{content_types::ContentType, encoded_content::EncodedContent};

#[derive(Tsify, Clone, Serialize, Deserialize)]
#[tsify(into_wasm_abi, from_wasm_abi)]
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

#[derive(Tsify, Clone, Serialize, Deserialize)]
#[tsify(into_wasm_abi, from_wasm_abi)]
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

#[derive(Tsify, Clone, Copy, Serialize, Deserialize)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[repr(u16)]
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

#[derive(Tsify, Default, Clone, Serialize, Deserialize)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct ListMessagesOptions {
  #[serde(rename = "contentTypes")]
  #[tsify(optional)]
  pub content_types: Option<Vec<ContentType>>,
  #[serde(rename = "sentBeforeNs")]
  #[tsify(optional)]
  pub sent_before_ns: Option<i64>,
  #[serde(rename = "sentAfterNs")]
  #[tsify(optional)]
  pub sent_after_ns: Option<i64>,
  #[tsify(optional)]
  pub limit: Option<i64>,
  #[serde(rename = "deliveryStatus")]
  #[tsify(optional)]
  pub delivery_status: Option<DeliveryStatus>,
  #[tsify(optional)]
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

#[derive(Tsify, Clone, Serialize, Deserialize)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct Message {
  pub id: String,
  #[serde(rename = "sentAtNs")]
  pub sent_at_ns: i64,
  #[serde(rename = "convoId")]
  pub convo_id: String,
  #[serde(rename = "senderInboxId")]
  pub sender_inbox_id: String,
  pub content: EncodedContent,
  pub kind: GroupMessageKind,
  #[serde(rename = "deliveryStatus")]
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
          content: vec![],
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

#[derive(Tsify, Clone, Serialize, Deserialize)]
#[tsify(into_wasm_abi, from_wasm_abi)]
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
