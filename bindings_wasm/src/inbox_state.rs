use crate::{client::Client, identity::PublicIdentifier};
use js_sys::Uint8Array;
use wasm_bindgen::{prelude::wasm_bindgen, JsError};
use xmtp_id::associations::{ident, AssociationState, MemberIdentifier};

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
  #[wasm_bindgen(js_name = recoveryIdentifier)]
  pub recovery_identifier: PublicIdentifier,
  pub installations: Vec<Installation>,
  #[wasm_bindgen(js_name = accountAddresses)]
  pub account_identifiers: Vec<PublicIdentifier>,
}

#[wasm_bindgen]
impl InboxState {
  #[wasm_bindgen(constructor)]
  pub fn new(
    inbox_id: String,
    recovery_identifier: PublicIdentifier,
    installations: Vec<Installation>,
    account_identifiers: Vec<PublicIdentifier>,
  ) -> Self {
    Self {
      inbox_id,
      recovery_identifier,
      installations,
      account_identifiers,
    }
  }
}

impl From<AssociationState> for InboxState {
  fn from(state: AssociationState) -> Self {
    let ident: PublicIdentifier = state.recovery_identifier().clone().into();
    Self {
      inbox_id: state.inbox_id().to_string(),
      recovery_identifier: ident
        .try_into()
        .expect("Recovery identifier should always be a root identifier"),
      installations: state
        .members()
        .into_iter()
        .filter_map(|m| match m.identifier {
          MemberIdentifier::Installation(ident::Installation(key)) => Some(Installation {
            bytes: Uint8Array::from(key.as_slice()),
            client_timestamp_ns: m.client_timestamp_ns,
            id: hex::encode(key),
          }),
          _ => None,
        })
        .collect(),
      account_identifiers: state.identifiers().into_iter().map(Into::into).collect(),
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
