use wasm_bindgen::prelude::wasm_bindgen;
use xmtp_mls::storage::consent_record::{ConsentState, ConsentType, StoredConsentRecord};

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
      WasmConsentEntityType::GroupId => ConsentType::GroupId,
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

impl From<WasmConsent> for StoredConsentRecord {
  fn from(consent: WasmConsent) -> Self {
    Self {
      entity_type: consent.entity_type.into(),
      state: consent.state.into(),
      entity: consent.entity,
    }
  }
}
