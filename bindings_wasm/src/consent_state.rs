use serde::{Deserialize, Serialize};
use wasm_bindgen::{prelude::wasm_bindgen, JsError};
use xmtp_db::consent_record::{
  ConsentState as XmtpConsentState, ConsentType as XmtpConsentType, StoredConsentRecord,
};

use crate::{client::Client, conversation::Conversation};

#[wasm_bindgen]
#[derive(Copy, Clone, Serialize, Deserialize)]
#[repr(u16)]
pub enum ConsentState {
  Unknown = 0,
  Allowed = 1,
  Denied = 2,
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

#[wasm_bindgen]
#[derive(Copy, Clone, Serialize, Deserialize)]
#[repr(u16)]
pub enum ConsentEntityType {
  GroupId = 0,
  InboxId = 1,
}

impl From<ConsentEntityType> for XmtpConsentType {
  fn from(entity_type: ConsentEntityType) -> Self {
    match entity_type {
      ConsentEntityType::GroupId => XmtpConsentType::ConversationId,
      ConsentEntityType::InboxId => XmtpConsentType::InboxId,
    }
  }
}

fn entity_to_u16<S>(consent_entity_type: &ConsentEntityType, s: S) -> Result<S::Ok, S::Error>
where
  S: serde::Serializer,
{
  let num: u16 = (*consent_entity_type) as u16;
  s.serialize_u16(num)
}

fn state_to_u16<S>(consent_state: &ConsentState, s: S) -> Result<S::Ok, S::Error>
where
  S: serde::Serializer,
{
  let num: u16 = (*consent_state) as u16;
  s.serialize_u16(num)
}

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone, Serialize, Deserialize)]
pub struct Consent {
  #[wasm_bindgen(js_name = entityType)]
  #[serde(rename = "entityType", serialize_with = "entity_to_u16")]
  pub entity_type: ConsentEntityType,
  #[serde(serialize_with = "state_to_u16")]
  pub state: ConsentState,
  pub entity: String,
}

#[wasm_bindgen]
impl Consent {
  #[wasm_bindgen(constructor)]
  pub fn new(entity_type: ConsentEntityType, state: ConsentState, entity: String) -> Self {
    Self {
      entity_type,
      state,
      entity,
    }
  }
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
  fn from(value: StoredConsentRecord) -> Self {
    Self {
      entity: value.entity,
      entity_type: match value.entity_type {
        XmtpConsentType::ConversationId => ConsentEntityType::GroupId,
        XmtpConsentType::InboxId => ConsentEntityType::InboxId,
      },
      state: value.state.into(),
    }
  }
}

#[wasm_bindgen]
impl Client {
  #[wasm_bindgen(js_name = setConsentStates)]
  pub async fn set_consent_states(&self, records: Vec<Consent>) -> Result<(), JsError> {
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
    entity_type: ConsentEntityType,
    entity: String,
  ) -> Result<ConsentState, JsError> {
    let result = self
      .inner_client()
      .get_consent_state(entity_type.into(), entity)
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;

    Ok(result.into())
  }
}

#[wasm_bindgen]
impl Conversation {
  #[wasm_bindgen(js_name = consentState)]
  pub fn consent_state(&self) -> Result<ConsentState, JsError> {
    let group = self.to_mls_group();
    let state = group
      .consent_state()
      .map_err(|e| JsError::new(&format!("{e}")))?;

    Ok(state.into())
  }

  #[wasm_bindgen(js_name = updateConsentState)]
  pub fn update_consent_state(&self, state: ConsentState) -> Result<(), JsError> {
    let group = self.to_mls_group();

    group
      .update_consent_state(state.into())
      .map_err(|e| JsError::new(&format!("{e}")))?;

    Ok(())
  }
}
