use crate::ErrorWrapper;
use napi::bindgen_prelude::BigInt;
use napi_derive::napi;
use xmtp_cryptography::signature::IdentifierValidationError;
use xmtp_id::associations::{Identifier as XmtpIdentifier, ident};

#[napi(object)]
#[derive(Clone, Hash, PartialEq, Eq)]
pub struct Identifier {
  pub identifier: String,
  pub identifier_kind: IdentifierKind,
}

#[napi]
#[derive(Hash, PartialEq, Eq)]
pub enum IdentifierKind {
  Ethereum,
  Passkey,
  // more to come...
}

impl From<XmtpIdentifier> for Identifier {
  fn from(ident: XmtpIdentifier) -> Self {
    match ident {
      XmtpIdentifier::Ethereum(ident::Ethereum(addr)) => Self {
        identifier: addr,
        identifier_kind: IdentifierKind::Ethereum,
      },
      XmtpIdentifier::Passkey(ident::Passkey { key, .. }) => Self {
        identifier: hex::encode(key),
        identifier_kind: IdentifierKind::Passkey,
      },
    }
  }
}

impl TryFrom<Identifier> for XmtpIdentifier {
  type Error = ErrorWrapper<IdentifierValidationError>;
  fn try_from(ident: Identifier) -> Result<Self, Self::Error> {
    let ident = match ident.identifier_kind {
      IdentifierKind::Ethereum => Self::eth(ident.identifier)?,
      IdentifierKind::Passkey => Self::passkey_str(&ident.identifier, None)?,
    };
    Ok(ident)
  }
}

pub trait IdentityExt<T, U> {
  fn to_internal(self) -> Result<Vec<U>, ErrorWrapper<IdentifierValidationError>>;
}

impl IdentityExt<Identifier, XmtpIdentifier> for Vec<Identifier> {
  fn to_internal(self) -> Result<Vec<XmtpIdentifier>, ErrorWrapper<IdentifierValidationError>> {
    let ident: Result<Vec<_>, ErrorWrapper<IdentifierValidationError>> =
      self.into_iter().map(|ident| ident.try_into()).collect();
    ident
  }
}

#[napi(object)]
pub struct ApiStats {
  pub upload_key_package: BigInt,
  pub fetch_key_package: BigInt,
  pub send_group_messages: BigInt,
  pub send_welcome_messages: BigInt,
  pub query_group_messages: BigInt,
  pub query_welcome_messages: BigInt,
  pub subscribe_messages: BigInt,
  pub subscribe_welcomes: BigInt,
}

impl From<xmtp_proto::api_client::ApiStats> for ApiStats {
  fn from(stats: xmtp_proto::api_client::ApiStats) -> Self {
    Self {
      upload_key_package: BigInt::from(stats.upload_key_package.get_count() as u64),
      fetch_key_package: BigInt::from(stats.fetch_key_package.get_count() as u64),
      send_group_messages: BigInt::from(stats.send_group_messages.get_count() as u64),
      send_welcome_messages: BigInt::from(stats.send_welcome_messages.get_count() as u64),
      query_group_messages: BigInt::from(stats.query_group_messages.get_count() as u64),
      query_welcome_messages: BigInt::from(stats.query_welcome_messages.get_count() as u64),
      subscribe_messages: BigInt::from(stats.subscribe_messages.get_count() as u64),
      subscribe_welcomes: BigInt::from(stats.subscribe_welcomes.get_count() as u64),
    }
  }
}

#[napi(object)]
pub struct IdentityStats {
  pub publish_identity_update: BigInt,
  pub get_identity_updates_v2: BigInt,
  pub get_inbox_ids: BigInt,
  pub verify_smart_contract_wallet_signature: BigInt,
}

impl From<xmtp_proto::api_client::IdentityStats> for IdentityStats {
  fn from(stats: xmtp_proto::api_client::IdentityStats) -> Self {
    Self {
      publish_identity_update: BigInt::from(stats.publish_identity_update.get_count() as u64),
      get_identity_updates_v2: BigInt::from(stats.get_identity_updates_v2.get_count() as u64),
      get_inbox_ids: BigInt::from(stats.get_inbox_ids.get_count() as u64),
      verify_smart_contract_wallet_signature: BigInt::from(
        stats.verify_smart_contract_wallet_signature.get_count() as u64,
      ),
    }
  }
}
