use wasm_bindgen::prelude::wasm_bindgen;
use xmtp_cryptography::signature::ed25519_public_key_to_address;
use xmtp_id::associations::{AssociationState, MemberIdentifier};

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
pub struct WasmInstallation {
  pub id: String,
  pub client_timestamp_ns: Option<u64>,
}

#[wasm_bindgen(getter_with_clone)]
pub struct WasmInboxState {
  pub inbox_id: String,
  pub recovery_address: String,
  pub installations: Vec<WasmInstallation>,
  pub account_addresses: Vec<String>,
}

impl From<AssociationState> for WasmInboxState {
  fn from(state: AssociationState) -> Self {
    Self {
      inbox_id: state.inbox_id().to_string(),
      recovery_address: state.recovery_address().to_string(),
      installations: state
        .members()
        .into_iter()
        .filter_map(|m| match m.identifier {
          MemberIdentifier::Address(_) => None,
          MemberIdentifier::Installation(inst) => Some(WasmInstallation {
            id: ed25519_public_key_to_address(inst.as_slice()),
            client_timestamp_ns: m.client_timestamp_ns,
          }),
        })
        .collect(),
      account_addresses: state.account_addresses(),
    }
  }
}
