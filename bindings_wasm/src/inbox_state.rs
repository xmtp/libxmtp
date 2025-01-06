use js_sys::Uint8Array;
use wasm_bindgen::{prelude::wasm_bindgen, JsError};
use xmtp_cryptography::signature::ed25519_public_key_to_address;
use xmtp_id::associations::{AssociationState, MemberIdentifier};

use crate::client::Client;

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
pub struct Installation {
  pub bytes: Uint8Array,
  pub id: String,
  #[wasm_bindgen(js_name = clientTimestampNs)]
  pub client_timestamp_ns: Option<u64>,
}

#[wasm_bindgen]
impl Installation {
  #[wasm_bindgen(constructor)]
  pub fn new(bytes: Uint8Array, id: String, client_timestamp_ns: Option<u64>) -> Self {
    Self {
      bytes,
      client_timestamp_ns,
      id,
    }
  }
}

#[wasm_bindgen(getter_with_clone)]
pub struct InboxState {
  #[wasm_bindgen(js_name = inboxId)]
  pub inbox_id: String,
  #[wasm_bindgen(js_name = recoveryAddress)]
  pub recovery_address: String,
  pub installations: Vec<Installation>,
  #[wasm_bindgen(js_name = accountAddresses)]
  pub account_addresses: Vec<String>,
}

#[wasm_bindgen]
impl InboxState {
  #[wasm_bindgen(constructor)]
  pub fn new(
    inbox_id: String,
    recovery_address: String,
    installations: Vec<Installation>,
    account_addresses: Vec<String>,
  ) -> Self {
    Self {
      inbox_id,
      recovery_address,
      installations,
      account_addresses,
    }
  }
}

impl From<AssociationState> for InboxState {
  fn from(state: AssociationState) -> Self {
    Self {
      inbox_id: state.inbox_id().to_string(),
      recovery_address: state.recovery_address().to_string(),
      installations: state
        .members()
        .into_iter()
        .filter_map(|m| match m.identifier {
          MemberIdentifier::Address(_) => None,
          MemberIdentifier::Installation(inst) => Some(Installation {
            bytes: Uint8Array::from(inst.as_slice()),
            client_timestamp_ns: m.client_timestamp_ns,
            id: ed25519_public_key_to_address(inst.as_slice()),
          }),
        })
        .collect(),
      account_addresses: state.account_addresses(),
    }
  }
}

#[wasm_bindgen]
impl Client {
  /**
   * Get the client's inbox state.
   *
   * If `refresh_from_network` is true, the client will go to the network first to refresh the state.
   * Otherwise, the state will be read from the local database.
   */
  #[wasm_bindgen(js_name = inboxState)]
  pub async fn inbox_state(&self, refresh_from_network: bool) -> Result<InboxState, JsError> {
    let state = self
      .inner_client()
      .inbox_state(refresh_from_network)
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;
    Ok(state.into())
  }

  #[wasm_bindgen(js_name = getLatestInboxState)]
  pub async fn get_latest_inbox_state(&self, inbox_id: String) -> Result<InboxState, JsError> {
    let conn = self
      .inner_client()
      .store()
      .conn()
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;
    let state = self
      .inner_client()
      .get_latest_association_state(&conn, &inbox_id)
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;
    Ok(state.into())
  }
}
