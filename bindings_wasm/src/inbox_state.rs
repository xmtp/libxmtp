use crate::{client::Client, identity::Identifier};
use js_sys::Uint8Array;
use std::collections::HashMap;
use wasm_bindgen::{prelude::wasm_bindgen, JsError, JsValue};
use xmtp_id::associations::{ident, AssociationState, MemberIdentifier};
use xmtp_mls::verified_key_package_v2::{VerifiedKeyPackageV2, VerifiedLifetime};

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
  pub recovery_identifier: Identifier,
  pub installations: Vec<Installation>,
  #[wasm_bindgen(js_name = accountIdentifiers)]
  pub account_identifiers: Vec<Identifier>,
}

#[wasm_bindgen]
impl InboxState {
  #[wasm_bindgen(constructor)]
  pub fn new(
    inbox_id: String,
    recovery_identifier: Identifier,
    installations: Vec<Installation>,
    account_identifiers: Vec<Identifier>,
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
    let ident: Identifier = state.recovery_identifier().clone().into();
    Self {
      inbox_id: state.inbox_id().to_string(),
      recovery_identifier: ident,
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

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone, serde::Serialize)]
pub struct KeyPackageStatus {
  #[wasm_bindgen(js_name = lifetime)]
  pub lifetime: Option<Lifetime>,
  #[wasm_bindgen(js_name = validationError)]
  #[serde(rename = "validationError")]
  pub validation_error: Option<String>,
}

#[wasm_bindgen]
#[derive(Clone, serde::Serialize)]
pub struct Lifetime {
  pub not_before: u64,
  pub not_after: u64,
}

impl From<VerifiedLifetime> for Lifetime {
  fn from(lifetime: VerifiedLifetime) -> Self {
    Self {
      not_before: lifetime.not_before,
      not_after: lifetime.not_after,
    }
  }
}

impl From<VerifiedKeyPackageV2> for KeyPackageStatus {
  fn from(key_package: VerifiedKeyPackageV2) -> Self {
    Self {
      lifetime: key_package.life_time().map(Into::into),
      validation_error: None,
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

  /**
   * Get key package statuses for a list of installation IDs.
   *
   * Returns a JavaScript object mapping installation ID strings to KeyPackageStatus objects.
   */
  #[wasm_bindgen(js_name = getKeyPackageStatusesForInstallationIds)]
  pub async fn get_key_package_statuses_for_installation_ids(
    &self,
    installation_ids: Vec<String>,
  ) -> Result<JsValue, JsError> {
    // Convert String to Vec<u8>
    let installation_ids = installation_ids
      .into_iter()
      .map(hex::decode)
      .collect::<std::result::Result<Vec<Vec<u8>>, _>>()
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;

    let key_package_results = self
      .inner_client()
      .get_key_packages_for_installation_ids(installation_ids)
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;

    // Create a HashMap to store results
    let mut result_map: HashMap<String, KeyPackageStatus> = HashMap::new();

    for (installation_id, key_package_result) in key_package_results {
      let status = match key_package_result {
        Ok(key_package) => KeyPackageStatus::from(key_package),
        Err(e) => KeyPackageStatus {
          lifetime: None,
          validation_error: Some(e.to_string()),
        },
      };

      // Convert installation_id to hex string for JavaScript
      let id_key = hex::encode(&installation_id);
      result_map.insert(id_key, status);
    }

    // Convert HashMap to JsValue
    Ok(crate::to_value(&result_map)?)
  }
}
