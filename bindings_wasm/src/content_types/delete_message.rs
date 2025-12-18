use crate::encoded_content::EncodedContent;
use js_sys::Uint8Array;
use prost::Message;
use wasm_bindgen::{JsError, prelude::wasm_bindgen};
use xmtp_content_types::{ContentCodec, delete_message::DeleteMessageCodec};

#[wasm_bindgen]
#[derive(Clone)]
pub struct DeleteMessage {
  #[wasm_bindgen(getter_with_clone, js_name = "messageId")]
  pub message_id: String,
}

#[wasm_bindgen]
impl DeleteMessage {
  #[wasm_bindgen(constructor)]
  pub fn new(message_id: String) -> Self {
    Self { message_id }
  }
}

impl From<xmtp_proto::xmtp::mls::message_contents::content_types::DeleteMessage> for DeleteMessage {
  fn from(dm: xmtp_proto::xmtp::mls::message_contents::content_types::DeleteMessage) -> Self {
    Self {
      message_id: dm.message_id,
    }
  }
}

impl From<DeleteMessage> for xmtp_proto::xmtp::mls::message_contents::content_types::DeleteMessage {
  fn from(dm: DeleteMessage) -> Self {
    Self {
      message_id: dm.message_id,
    }
  }
}

#[wasm_bindgen(js_name = "encodeDeleteMessage")]
pub fn encode_delete_message(
  #[wasm_bindgen(js_name = "deleteMessage")] delete_message: DeleteMessage,
) -> Result<Uint8Array, JsError> {
  let encoded = DeleteMessageCodec::encode(delete_message.into())
    .map_err(|e| JsError::new(&format!("{}", e)))?;

  let mut buf = Vec::new();
  encoded
    .encode(&mut buf)
    .map_err(|e| JsError::new(&format!("{}", e)))?;

  Ok(Uint8Array::from(buf.as_slice()))
}

#[wasm_bindgen(js_name = "decodeDeleteMessage")]
pub fn decode_delete_message(encoded_content: EncodedContent) -> Result<DeleteMessage, JsError> {
  DeleteMessageCodec::decode(encoded_content.into())
    .map(Into::into)
    .map_err(|e| JsError::new(&format!("{}", e)))
}

#[wasm_bindgen]
#[derive(Clone)]
pub struct DeletedMessage {
  #[wasm_bindgen(getter_with_clone, js_name = "deletedBy")]
  pub deleted_by: DeletedBy,
  #[wasm_bindgen(getter_with_clone, js_name = "adminInboxId")]
  pub admin_inbox_id: Option<String>,
}

#[wasm_bindgen]
#[derive(Clone)]
pub enum DeletedBy {
  Sender,
  Admin,
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
