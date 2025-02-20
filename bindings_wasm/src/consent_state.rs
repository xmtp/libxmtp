use wasm_bindgen::{prelude::wasm_bindgen, JsError};
use xmtp_mls::storage::consent_record::{
  ConsentState as XmtpConsentState, StoredConsentRecord, StoredConsentType as XmtpConsentType,
  StoredIdentityKind,
};

use crate::{client::Client, conversation::Conversation};

#[wasm_bindgen]
#[derive(Clone, serde::Serialize)]
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

#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub enum ConsentEntityType {
  GroupId,
  InboxId,
  Identity,
}

impl From<ConsentEntityType> for XmtpConsentType {
  fn from(entity_type: ConsentEntityType) -> Self {
    match entity_type {
      ConsentEntityType::GroupId => XmtpConsentType::ConversationId,
      ConsentEntityType::InboxId => XmtpConsentType::InboxId,
      ConsentEntityType::Identity => XmtpConsentType::Identity,
    }
  }
}

#[wasm_bindgen(getter_with_clone)]
pub struct Consent {
  #[wasm_bindgen(js_name = entityType)]
  pub entity_type: ConsentEntityType,
  pub state: ConsentState,
  pub entity: String,
  #[wasm_bindgen(js_name = identityKind)]
  pub identity_kind: Option<IdentityKind>,
}

#[wasm_bindgen]
#[derive(Clone)]
pub enum IdentityKind {
  Ethereum,
  Passkey,
}

impl From<StoredIdentityKind> for IdentityKind {
  fn from(kind: StoredIdentityKind) -> Self {
    match kind {
      StoredIdentityKind::Ethereum => Self::Ethereum,
      StoredIdentityKind::Passkey => Self::Passkey,
    }
  }
}
impl From<IdentityKind> for StoredIdentityKind {
  fn from(kind: IdentityKind) -> Self {
    match kind {
      IdentityKind::Ethereum => Self::Ethereum,
      IdentityKind::Passkey => Self::Passkey,
    }
  }
}

#[wasm_bindgen]
impl Consent {
  #[wasm_bindgen(constructor)]
  pub fn new(
    entity_type: ConsentEntityType,
    state: ConsentState,
    entity: String,
    mut identity_kind: Option<IdentityKind>,
  ) -> Result<Self, JsError> {
    if entity_type != ConsentEntityType::Identity {
      identity_kind = None;
    } else if identity_kind.is_none() {
      return Err(JsError::new(
        "identity_kind is required when entity_type is `Identity`",
      ));
    }

    Ok(Self {
      entity_type,
      state,
      entity,
      identity_kind,
    })
  }
}

impl From<Consent> for StoredConsentRecord {
  fn from(consent: Consent) -> Self {
    Self {
      entity_type: consent.entity_type.into(),
      state: consent.state.into(),
      entity: consent.entity,
      identity_kind: consent.identity_kind.map(Into::into),
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
