use napi_derive::napi;
use xmtp_db::group_message::ContentType as XmtpContentType;

pub mod multi_remote_attachment;
pub mod reaction;

#[napi]
pub enum ContentType {
  Unknown,
  Text,
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
