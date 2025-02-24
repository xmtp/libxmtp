use napi::bindgen_prelude::Result;
use napi_derive::napi;
use xmtp_mls::storage::consent_record::{
  ConsentState as XmtpConsentState, ConsentType as XmtpConsentType, StoredConsentRecord,
};

use crate::{client::Client, ErrorWrapper};

#[napi]
#[derive(serde::Serialize, serde::Deserialize)]
pub enum ConsentState {
  Unknown,
  Allowed,
  Denied,
}

impl From<XmtpConsentState> for ConsentState {
  fn from(state: XmtpConsentState) -> Self {
    match state {
      XmtpConsentState::Unknown => ConsentState::Unknown,
      XmtpConsentState::Allowed => ConsentState::Allowed,
      XmtpConsentState::Denied => ConsentState::Denied,
    }
  }
}

impl From<ConsentState> for XmtpConsentState {
  fn from(state: ConsentState) -> Self {
    match state {
      ConsentState::Unknown => XmtpConsentState::Unknown,
      ConsentState::Allowed => XmtpConsentState::Allowed,
      ConsentState::Denied => XmtpConsentState::Denied,
    }
  }
}

#[napi]
#[derive(serde::Serialize, serde::Deserialize)]
pub enum ConsentEntityType {
  GroupId,
  InboxId,
  Address,
}

impl From<ConsentEntityType> for XmtpConsentType {
  fn from(entity_type: ConsentEntityType) -> Self {
    match entity_type {
      ConsentEntityType::GroupId => XmtpConsentType::ConversationId,
      ConsentEntityType::InboxId => XmtpConsentType::InboxId,
      ConsentEntityType::Address => XmtpConsentType::Address,
    }
  }
}

impl From<XmtpConsentType> for ConsentEntityType {
  fn from(entity_type: XmtpConsentType) -> Self {
    match entity_type {
      XmtpConsentType::ConversationId => ConsentEntityType::GroupId,
      XmtpConsentType::InboxId => ConsentEntityType::InboxId,
      XmtpConsentType::Address => ConsentEntityType::Address,
    }
  }
}

#[napi(object)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Consent {
  pub entity_type: ConsentEntityType,
  pub state: ConsentState,
  pub entity: String,
}

impl From<Consent> for StoredConsentRecord {
  fn from(consent: Consent) -> Self {
    Self {
      entity_type: consent.entity_type.into(),
      state: consent.state.into(),
      entity: consent.entity,
    }
  }
}

impl From<StoredConsentRecord> for Consent {
  fn from(consent: StoredConsentRecord) -> Self {
    Self {
      entity_type: consent.entity_type.into(),
      state: consent.state.into(),
      entity: consent.entity,
    }
  }
}

#[napi]
impl Client {
  #[napi]
  pub async fn set_consent_states(&self, records: Vec<Consent>) -> Result<()> {
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
    entity_type: ConsentEntityType,
    entity: String,
  ) -> Result<ConsentState> {
    let result = self
      .inner_client()
      .get_consent_state(entity_type.into(), entity)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(result.into())
  }
}
