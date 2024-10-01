use napi_derive::napi;
use xmtp_mls::storage::consent_record::{ConsentState, ConsentType, StoredConsentRecord};

#[napi]
pub enum NapiConsentState {
  Unknown,
  Allowed,
  Denied,
}

impl From<ConsentState> for NapiConsentState {
  fn from(state: ConsentState) -> Self {
    match state {
      ConsentState::Unknown => NapiConsentState::Unknown,
      ConsentState::Allowed => NapiConsentState::Allowed,
      ConsentState::Denied => NapiConsentState::Denied,
    }
  }
}

impl From<NapiConsentState> for ConsentState {
  fn from(state: NapiConsentState) -> Self {
    match state {
      NapiConsentState::Unknown => ConsentState::Unknown,
      NapiConsentState::Allowed => ConsentState::Allowed,
      NapiConsentState::Denied => ConsentState::Denied,
    }
  }
}

#[napi]
pub enum NapiConsentEntityType {
  GroupId,
  InboxId,
  Address,
}

impl From<NapiConsentEntityType> for ConsentType {
  fn from(entity_type: NapiConsentEntityType) -> Self {
    match entity_type {
      NapiConsentEntityType::GroupId => ConsentType::GroupId,
      NapiConsentEntityType::InboxId => ConsentType::InboxId,
      NapiConsentEntityType::Address => ConsentType::Address,
    }
  }
}

#[napi(object)]
pub struct NapiConsent {
  pub entity_type: NapiConsentEntityType,
  pub state: NapiConsentState,
  pub entity: String,
}

impl From<NapiConsent> for StoredConsentRecord {
  fn from(consent: NapiConsent) -> Self {
    Self {
      entity_type: consent.entity_type.into(),
      state: consent.state.into(),
      entity: consent.entity,
    }
  }
}
