use wasm_bindgen::prelude::wasm_bindgen;
use xmtp_mls::storage::group_message::ContentType as XmtpContentType;

pub mod multi_remote_attachment;
pub mod reaction;

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
