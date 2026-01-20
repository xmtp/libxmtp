use napi_derive::napi;
use xmtp_common::time::now_ns;
use xmtp_db::consent_record::{
  ConsentState as XmtpConsentState, ConsentType as XmtpConsentType, StoredConsentRecord,
};

#[napi]
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
pub enum ConsentEntityType {
  GroupId,
  InboxId,
}

impl From<ConsentEntityType> for XmtpConsentType {
  fn from(entity_type: ConsentEntityType) -> Self {
    match entity_type {
      ConsentEntityType::GroupId => XmtpConsentType::ConversationId,
      ConsentEntityType::InboxId => XmtpConsentType::InboxId,
    }
  }
}

impl From<XmtpConsentType> for ConsentEntityType {
  fn from(entity_type: XmtpConsentType) -> Self {
    match entity_type {
      XmtpConsentType::ConversationId => ConsentEntityType::GroupId,
      XmtpConsentType::InboxId => ConsentEntityType::InboxId,
    }
  }
}

#[napi(object)]
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
      consented_at_ns: now_ns(),
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
