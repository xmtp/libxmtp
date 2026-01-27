use bindings_wasm_macros::wasm_bindgen_numbered_enum;
use serde::{Deserialize, Serialize};
use tsify::Tsify;

#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct DeletedMessage {
  pub deleted_by: DeletedBy,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub admin_inbox_id: Option<String>,
}

#[wasm_bindgen_numbered_enum]
pub enum DeletedBy {
  Sender = 0,
  Admin = 1,
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
