use wasm_bindgen::{prelude::wasm_bindgen, JsError};
use xmtp_mls::storage::consent_record::{ConsentState, ConsentType, StoredConsentRecord};

use crate::{groups::WasmGroup, mls_client::WasmClient};

#[wasm_bindgen]
#[derive(Clone, serde::Serialize)]
pub enum WasmConsentState {
  Unknown,
  Allowed,
  Denied,
}

impl From<ConsentState> for WasmConsentState {
  fn from(state: ConsentState) -> Self {
    match state {
      ConsentState::Unknown => WasmConsentState::Unknown,
      ConsentState::Allowed => WasmConsentState::Allowed,
      ConsentState::Denied => WasmConsentState::Denied,
    }
  }
}

impl From<WasmConsentState> for ConsentState {
  fn from(state: WasmConsentState) -> Self {
    match state {
      WasmConsentState::Unknown => ConsentState::Unknown,
      WasmConsentState::Allowed => ConsentState::Allowed,
      WasmConsentState::Denied => ConsentState::Denied,
    }
  }
}

#[wasm_bindgen]
#[derive(Clone)]
pub enum WasmConsentEntityType {
  GroupId,
  InboxId,
  Address,
}

impl From<WasmConsentEntityType> for ConsentType {
  fn from(entity_type: WasmConsentEntityType) -> Self {
    match entity_type {
      WasmConsentEntityType::GroupId => ConsentType::ConversationId,
      WasmConsentEntityType::InboxId => ConsentType::InboxId,
      WasmConsentEntityType::Address => ConsentType::Address,
    }
  }
}

#[wasm_bindgen(getter_with_clone)]
pub struct WasmConsent {
  pub entity_type: WasmConsentEntityType,
  pub state: WasmConsentState,
  pub entity: String,
}

#[wasm_bindgen]
impl WasmConsent {
  #[wasm_bindgen(constructor)]
  pub fn new(entity_type: WasmConsentEntityType, state: WasmConsentState, entity: String) -> Self {
    Self {
      entity_type,
      state,
      entity,
    }
  }
}

impl From<WasmConsent> for StoredConsentRecord {
  fn from(consent: WasmConsent) -> Self {
    Self {
      entity_type: consent.entity_type.into(),
      state: consent.state.into(),
      entity: consent.entity,
    }
  }
}

#[wasm_bindgen]
impl WasmClient {
  #[wasm_bindgen(js_name = setConsentStates)]
  pub async fn set_consent_states(&self, records: Vec<WasmConsent>) -> Result<(), JsError> {
    let stored_records: Vec<StoredConsentRecord> =
      records.into_iter().map(StoredConsentRecord::from).collect();

    self
      .inner_client()
      .set_consent_states(&stored_records)
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;
    Ok(())
  }

  #[wasm_bindgen(js_name = getConsentState)]
  pub async fn get_consent_state(
    &self,
    entity_type: WasmConsentEntityType,
    entity: String,
  ) -> Result<WasmConsentState, JsError> {
    let result = self
      .inner_client()
      .get_consent_state(entity_type.into(), entity)
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;

    Ok(result.into())
  }
}

#[wasm_bindgen]
impl WasmGroup {
  #[wasm_bindgen]
  pub fn consent_state(&self) -> Result<WasmConsentState, JsError> {
    let group = self.to_mls_group();
    let state = group
      .consent_state()
      .map_err(|e| JsError::new(&format!("{e}")))?;

    Ok(state.into())
  }

  #[wasm_bindgen]
  pub fn update_consent_state(&self, state: WasmConsentState) -> Result<(), JsError> {
    let group = self.to_mls_group();

    group
      .update_consent_state(state.into())
      .map_err(|e| JsError::new(&format!("{e}")))?;

    Ok(())
  }
}
