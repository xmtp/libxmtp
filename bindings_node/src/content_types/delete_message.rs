use napi_derive::napi;
use xmtp_content_types::{ContentCodec, delete_message::DeleteMessageCodec};

use crate::encoded_content::ContentTypeId;

#[napi(object)]
#[derive(Clone)]
pub struct DeleteMessage {
  pub message_id: String,
}

impl From<xmtp_proto::xmtp::mls::message_contents::content_types::DeleteMessage> for DeleteMessage {
  fn from(dm: xmtp_proto::xmtp::mls::message_contents::content_types::DeleteMessage) -> Self {
    Self {
      message_id: dm.message_id,
    }
  }
}

#[napi]
pub fn delete_message_content_type() -> ContentTypeId {
  DeleteMessageCodec::content_type().into()
}

#[napi(string_enum)]
#[derive(Clone, PartialEq)]
pub enum DeletedBy {
  Sender,
  Admin,
}

#[napi(object)]
#[derive(Clone)]
pub struct DeletedMessage {
  pub deleted_by: DeletedBy,
  pub admin_inbox_id: Option<String>,
}

impl From<xmtp_mls::messages::decoded_message::DeletedBy> for DeletedBy {
  fn from(value: xmtp_mls::messages::decoded_message::DeletedBy) -> Self {
    match value {
      xmtp_mls::messages::decoded_message::DeletedBy::Sender => DeletedBy::Sender,
      xmtp_mls::messages::decoded_message::DeletedBy::Admin(_) => DeletedBy::Admin,
    }
  }
}

impl From<xmtp_mls::messages::decoded_message::DeletedBy> for DeletedMessage {
  fn from(value: xmtp_mls::messages::decoded_message::DeletedBy) -> Self {
    match value {
      xmtp_mls::messages::decoded_message::DeletedBy::Sender => DeletedMessage {
        deleted_by: DeletedBy::Sender,
        admin_inbox_id: None,
      },
      xmtp_mls::messages::decoded_message::DeletedBy::Admin(inbox_id) => DeletedMessage {
        deleted_by: DeletedBy::Admin,
        admin_inbox_id: Some(inbox_id),
      },
    }
  }
}
