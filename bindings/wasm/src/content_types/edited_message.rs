use bindings_wasm_macros::wasm_bindgen_numbered_enum;
use serde::{Deserialize, Serialize};
use tsify::Tsify;

#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct EditedMessage {
  pub edited_by: EditedBy,
}

#[wasm_bindgen_numbered_enum]
pub enum EditedBy {
  Sender = 0,
}

impl From<xmtp_mls::messages::decoded_message::EditedBy> for EditedMessage {
  fn from(value: xmtp_mls::messages::decoded_message::EditedBy) -> Self {
    match value {
      xmtp_mls::messages::decoded_message::EditedBy::Sender => EditedMessage {
        edited_by: EditedBy::Sender,
      },
    }
  }
}
