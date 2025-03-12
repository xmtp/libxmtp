use serde_repr::{Deserialize_repr, Serialize_repr};
use tsify_next::Tsify;
use xmtp_mls::storage::group_message::ContentType as XmtpContentType;

pub mod multi_remote_attachment;
pub mod reaction;

#[derive(Tsify, Clone, Serialize_repr, Deserialize_repr)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[repr(u16)]
pub enum ContentType {
  Unknown = 0,
  Text = 1,
  GroupMembershipChange = 2,
  GroupUpdated = 3,
  Reaction = 4,
  ReadReceipt = 5,
  Reply = 6,
  Attachment = 7,
  RemoteAttachment = 8,
  TransactionReference = 9,
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
