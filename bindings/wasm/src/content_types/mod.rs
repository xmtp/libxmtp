use bindings_wasm_macros::wasm_bindgen_numbered_enum;
use thiserror::Error;
use wasm_bindgen::JsError;
use xmtp_common::ErrorCode;
use xmtp_db::group_message::ContentType as XmtpContentType;

use crate::ErrorWrapper;

/// Error type for content type conversion failures in WASM bindings.
///
/// Provides structured error codes via `ErrorCode` derive, ensuring
/// all content type errors are prefixed with `[ContentTypeError::Variant]`
/// when surfaced to JavaScript.
#[derive(Debug, Error, ErrorCode)]
pub enum ContentTypeError {
  #[error("{0}")]
  InvalidData(String),
  #[error("{0}")]
  TimestampOutOfRange(String),
  #[error("{0}")]
  Codec(String),
  #[error("{0}")]
  Crypto(String),
}

impl ContentTypeError {
  /// Converts this error into a `JsError` with the `[ErrorCode] message` format.
  pub fn into_js(self) -> JsError {
    ErrorWrapper(self).into()
  }
}

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
