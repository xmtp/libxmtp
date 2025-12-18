use serde::{Deserialize, Serialize};
use tsify::Tsify;
use xmtp_db::group_message::ContentType as XmtpContentType;

pub mod actions;
pub mod attachment;
pub mod decoded_message_content;
pub mod group_updated;
pub mod intent;
pub mod leave_request;
pub mod markdown;
pub mod multi_remote_attachment;
pub mod reaction;
pub mod read_receipt;
pub mod remote_attachment;
pub mod reply;
pub mod text;
pub mod transaction_reference;
pub mod wallet_send_calls;

#[derive(Clone, Copy, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub enum ContentType {
  Unknown,
  Text,
  Markdown,
  LeaveRequest,
  GroupMembershipChange,
  GroupUpdated,
  Reaction,
  ReadReceipt,
  Reply,
  Attachment,
  RemoteAttachment,
  TransactionReference,
}

impl From<ContentType> for XmtpContentType {
  fn from(value: ContentType) -> Self {
    match value {
      ContentType::Unknown => XmtpContentType::Unknown,
      ContentType::Text => XmtpContentType::Text,
      ContentType::Markdown => XmtpContentType::Markdown,
      ContentType::LeaveRequest => XmtpContentType::LeaveRequest,
      ContentType::GroupMembershipChange => XmtpContentType::GroupMembershipChange,
      ContentType::GroupUpdated => XmtpContentType::GroupUpdated,
      ContentType::Reaction => XmtpContentType::Reaction,
      ContentType::ReadReceipt => XmtpContentType::ReadReceipt,
      ContentType::Reply => XmtpContentType::Reply,
      ContentType::Attachment => XmtpContentType::Attachment,
      ContentType::RemoteAttachment => XmtpContentType::RemoteAttachment,
      ContentType::TransactionReference => XmtpContentType::TransactionReference,
    }
  }
}
