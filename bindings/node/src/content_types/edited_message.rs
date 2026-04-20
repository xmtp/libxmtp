use napi_derive::napi;

#[napi(string_enum)]
#[derive(Clone, PartialEq)]
pub enum EditedBy {
  Sender,
}

#[napi(object)]
#[derive(Clone)]
pub struct EditedMessage {
  pub edited_by: EditedBy,
}

impl From<xmtp_mls::messages::decoded_message::EditedBy> for EditedBy {
  fn from(value: xmtp_mls::messages::decoded_message::EditedBy) -> Self {
    match value {
      xmtp_mls::messages::decoded_message::EditedBy::Sender => EditedBy::Sender,
    }
  }
}

impl From<xmtp_mls::messages::decoded_message::EditedBy> for EditedMessage {
  fn from(value: xmtp_mls::messages::decoded_message::EditedBy) -> Self {
    EditedMessage {
      edited_by: value.into(),
    }
  }
}
