use wasm_bindgen::prelude::wasm_bindgen;
use xmtp_db::group_message::ContentType as XmtpContentType;

pub mod attachment;
pub mod decoded_message_content;
pub mod group_updated;
pub mod multi_remote_attachment;
pub mod reaction;
pub mod read_receipt;
pub mod remote_attachment;
pub mod reply;
pub mod text;
pub mod transaction_reference;
pub mod wallet_send_calls;

#[wasm_bindgen]
#[derive(Clone)]
pub enum ContentType {
  Unknown,
  Text,
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
