use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen]
#[derive(Clone, Serialize, Deserialize)]
pub struct DeletedMessage {
  #[wasm_bindgen(getter_with_clone, js_name = "deletedBy")]
  pub deleted_by: DeletedBy,
  #[wasm_bindgen(getter_with_clone, js_name = "adminInboxId")]
  pub admin_inbox_id: Option<String>,
}

#[wasm_bindgen]
#[derive(Clone, Copy, Serialize, Deserialize)]
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
