use napi::bindgen_prelude::Result;
use napi_derive::napi;
use xmtp_id::associations::PublicIdentifier;
use xmtp_mls::storage::{
  consent_record::{
    ConsentEntity, ConsentState as XmtpConsentState, StoredConsentRecord,
    StoredConsentType as XmtpConsentType, StoredIdentityKind as XmtpConsentIdentityKind,
  },
  MissingRequired,
};

use crate::{client::Client, ErrorWrapper};

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

#[napi]
pub enum ConsentIdentityKind {
  Ethereum,
  Passkey,
}

impl From<ConsentIdentityKind> for XmtpConsentIdentityKind {
  fn from(kind: ConsentIdentityKind) -> Self {
    match kind {
      ConsentIdentityKind::Passkey => Self::Passkey,
      ConsentIdentityKind::Ethereum => Self::Ethereum,
    }
  }
}

#[napi(object)]
pub struct Consent {
  pub entity_type: ConsentEntityType,
  pub state: ConsentState,
  pub entity: String,
  pub identity_kind: Option<ConsentIdentityKind>,
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
    identifier_kind: Option<ConsentIdentityKind>,
  ) -> Result<ConsentState> {
    let consent_entity = match entity_type {
      ConsentEntityType::GroupId => {
        ConsentEntity::ConversationId(hex::decode(entity).map_err(ErrorWrapper::from)?)
      }
      ConsentEntityType::InboxId => ConsentEntity::InboxId(entity),
      ConsentEntityType::Identity => {
        let Some(kind) = identifier_kind else {
          return Err(ErrorWrapper::from(MissingRequired::IdentifierKind))?;
        };
        let ident = match kind {
          ConsentIdentityKind::Passkey => PublicIdentifier::passkey_str(&entity),
          ConsentIdentityKind::Ethereum => PublicIdentifier::eth(&entity),
        }
        .map_err(ErrorWrapper::from)?;
        ConsentEntity::Identity(ident)
      }
    };

    let result = self
      .inner_client()
      .get_consent_state(consent_entity)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(result.into())
  }
}
