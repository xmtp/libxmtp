use crate::{client::Client, identity::Identifier};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tsify::Tsify;
use wasm_bindgen::{JsError, JsValue, prelude::wasm_bindgen};
use xmtp_api::{ApiClientWrapper, strategies};
use xmtp_api_d14n::MessageBackendBuilder;
use xmtp_api_d14n::TrackedStatsClient;
use xmtp_db::{EncryptedMessageStore, StorageOption, WasmDb};
use xmtp_id::associations::{AssociationState, MemberIdentifier, ident};
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_mls::client::inbox_addresses_with_verifier;
use xmtp_mls::verified_key_package_v2::{VerifiedKeyPackageV2, VerifiedLifetime};

#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi, large_number_types_as_bigints)]
#[serde(rename_all = "camelCase")]
pub struct Installation {
  #[serde(with = "serde_bytes")]
  #[tsify(type = "Uint8Array")]
  pub bytes: Vec<u8>,
  pub id: String,
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub client_timestamp_ns: Option<u64>,
}

#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct InboxState {
  pub inbox_id: String,
  pub recovery_identifier: Identifier,
  pub installations: Vec<Installation>,
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
            bytes: key.to_vec(),
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

#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct KeyPackageStatus {
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub lifetime: Option<Lifetime>,
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub validation_error: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi, large_number_types_as_bigints)]
#[serde(rename_all = "camelCase")]
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

#[wasm_bindgen(js_name = inboxStateFromInboxIds)]
pub async fn inbox_state_from_inbox_ids(
  #[wasm_bindgen(js_name = host)] v3_host: String,
  #[wasm_bindgen(js_name = gatewayHost)] gateway_host: Option<String>,
  #[wasm_bindgen(js_name = inboxIds)] inbox_ids: Vec<String>,
) -> Result<Vec<InboxState>, JsError> {
  let backend = MessageBackendBuilder::default()
    .v3_host(&v3_host)
    .maybe_gateway_host(gateway_host)
    .is_secure(true)
    .build()
    .map_err(|e| JsError::new(&e.to_string()))?;
  let backend = TrackedStatsClient::new(backend);
  let api = ApiClientWrapper::new(backend, strategies::exponential_cooldown());
  let scw_verifier = Arc::new(Box::new(api.clone()) as Box<dyn SmartContractSignatureVerifier>);

  let db = WasmDb::new(&StorageOption::Ephemeral).await?;
  let store = EncryptedMessageStore::new(db)
    .map_err(|e| JsError::new(&format!("Error creating unencrypted message store {e}")))?;

  let state = inbox_addresses_with_verifier(
    &api.clone(),
    &store.db(),
    inbox_ids.iter().map(String::as_str).collect(),
    &scw_verifier,
  )
  .await
  .map_err(|e| JsError::new(format!("{}", e).as_str()))?;
  Ok(state.into_iter().map(Into::into).collect())
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
  pub async fn inbox_state(
    &self,
    #[wasm_bindgen(js_name = refreshFromNetwork)] refresh_from_network: bool,
  ) -> Result<InboxState, JsError> {
    let state = self
      .inner_client()
      .inbox_state(refresh_from_network)
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;
    Ok(state.into())
  }

  #[wasm_bindgen(js_name = getLatestInboxState)]
  pub async fn get_latest_inbox_state(
    &self,
    #[wasm_bindgen(js_name = inboxId)] inbox_id: String,
  ) -> Result<InboxState, JsError> {
    let conn = self.inner_client().context.store().db();
    let state = self
      .inner_client()
      .identity_updates()
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
    #[wasm_bindgen(js_name = installationIds)] installation_ids: Vec<String>,
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
