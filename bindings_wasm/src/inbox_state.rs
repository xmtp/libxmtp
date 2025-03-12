use crate::{client::Client, identity::Identifier};
use serde::{Deserialize, Serialize};
use tsify_next::Tsify;
use wasm_bindgen::{prelude::wasm_bindgen, JsError};
use xmtp_id::associations::{ident, AssociationState, MemberIdentifier};

#[derive(Tsify, Clone, Serialize, Deserialize)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct Installation {
  #[serde(with = "serde_bytes")]
  pub bytes: Vec<u8>,
  pub id: String,
  #[serde(rename = "clientTimestampNs")]
  pub client_timestamp_ns: Option<u64>,
}

#[derive(Tsify, Clone, Serialize, Deserialize)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct InboxState {
  #[serde(rename = "inboxId")]
  pub inbox_id: String,
  #[serde(rename = "recoveryIdentifier")]
  pub recovery_identifier: Identifier,
  pub installations: Vec<Installation>,
  #[serde(rename = "accountIdentifiers")]
  pub account_identifiers: Vec<Identifier>,
}

impl From<AssociationState> for InboxState {
  fn from(state: AssociationState) -> Self {
    let ident: Identifier = state.recovery_identifier().clone().into();
    Self {
      inbox_id: state.inbox_id().to_string(),
      recovery_identifier: ident,
      installations: state
        .members()
        .into_iter()
        .filter_map(|m| match m.identifier {
          MemberIdentifier::Installation(ident::Installation(key)) => Some(Installation {
            bytes: key.clone(),
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
