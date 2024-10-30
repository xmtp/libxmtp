use napi::bindgen_prelude::Result;
use napi_derive::napi;
use xmtp_mls::storage::consent_record::{ConsentState, ConsentType, StoredConsentRecord};

use crate::{mls_client::NapiClient, ErrorWrapper};

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
      NapiConsentEntityType::GroupId => ConsentType::ConversationId,
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

#[napi]
impl NapiClient {
  #[napi]
  pub async fn set_consent_states(&self, records: Vec<NapiConsent>) -> Result<()> {
    let stored_records: Vec<StoredConsentRecord> =
      records.into_iter().map(StoredConsentRecord::from).collect();

    self
      .inner_client()
      .set_consent_states(&stored_records)
      .await
      .map_err(ErrorWrapper::from)?;
    Ok(())
  }

  #[napi]
  pub async fn get_consent_state(
    &self,
    entity_type: NapiConsentEntityType,
    entity: String,
  ) -> Result<NapiConsentState> {
    let result = self
      .inner_client()
      .get_consent_state(entity_type.into(), entity)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(result.into())
  }
}
