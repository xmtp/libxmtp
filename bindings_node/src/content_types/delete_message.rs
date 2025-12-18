use napi_derive::napi;

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
