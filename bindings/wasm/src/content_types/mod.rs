use bindings_wasm_macros::wasm_bindgen_numbered_enum;
use xmtp_db::group_message::ContentType as XmtpContentType;

pub mod actions;
pub mod attachment;
pub mod decoded_message_content;
pub mod deleted_message;
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

#[wasm_bindgen_numbered_enum]
pub enum ContentType {
  Actions = 0,
  Attachment = 1,
  Custom = 2,
  GroupMembershipChange = 3,
  GroupUpdated = 4,
  Intent = 5,
  LeaveRequest = 6,
  Markdown = 7,
  MultiRemoteAttachment = 8,
  Reaction = 9,
  ReadReceipt = 10,
  RemoteAttachment = 11,
  Reply = 12,
  Text = 13,
  TransactionReference = 14,
  WalletSendCalls = 15,
}

impl From<ContentType> for XmtpContentType {
  fn from(value: ContentType) -> Self {
    match value {
      ContentType::Actions => XmtpContentType::Actions,
      ContentType::Attachment => XmtpContentType::Attachment,
      ContentType::Custom => XmtpContentType::Unknown,
      ContentType::GroupMembershipChange => XmtpContentType::GroupMembershipChange,
      ContentType::GroupUpdated => XmtpContentType::GroupUpdated,
      ContentType::Intent => XmtpContentType::Intent,
      ContentType::LeaveRequest => XmtpContentType::LeaveRequest,
      ContentType::Markdown => XmtpContentType::Markdown,
      ContentType::MultiRemoteAttachment => XmtpContentType::MultiRemoteAttachment,
      ContentType::Text => XmtpContentType::Text,
      ContentType::Reaction => XmtpContentType::Reaction,
      ContentType::ReadReceipt => XmtpContentType::ReadReceipt,
      ContentType::Reply => XmtpContentType::Reply,
      ContentType::RemoteAttachment => XmtpContentType::RemoteAttachment,
      ContentType::TransactionReference => XmtpContentType::TransactionReference,
      ContentType::WalletSendCalls => XmtpContentType::WalletSendCalls,
    }
  }
}
