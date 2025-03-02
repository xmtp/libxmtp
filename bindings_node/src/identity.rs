use crate::ErrorWrapper;
use napi_derive::napi;
use xmtp_cryptography::signature::IdentifierValidationError;
use xmtp_id::associations::{ident, PublicIdentifier as XMTPPublicIdentifier};

#[napi(object)]
#[derive(Clone, Hash, PartialEq, Eq)]
pub struct PublicIdentifier {
  pub identifier: String,
  pub identifier_kind: PublicIdentifierKind,
  pub relying_partner: Option<String>,
}

#[napi]
#[derive(Hash, PartialEq, Eq)]
pub enum PublicIdentifierKind {
  Ethereum,
  Passkey,
  // more to come...
}

impl From<XMTPPublicIdentifier> for PublicIdentifier {
  fn from(ident: XMTPPublicIdentifier) -> Self {
    match ident {
      XMTPPublicIdentifier::Ethereum(ident::Ethereum(addr)) => Self {
        identifier: addr,
        identifier_kind: PublicIdentifierKind::Ethereum,
        relying_partner: None,
      },
      XMTPPublicIdentifier::Passkey(ident::Passkey {
        key,
        relying_partner,
      }) => Self {
        identifier: hex::encode(key),
        identifier_kind: PublicIdentifierKind::Passkey,
        relying_partner,
      },
    }
  }
}

impl TryFrom<PublicIdentifier> for XMTPPublicIdentifier {
  type Error = ErrorWrapper<IdentifierValidationError>;
  fn try_from(ident: PublicIdentifier) -> Result<Self, Self::Error> {
    let ident = match ident.identifier_kind {
      PublicIdentifierKind::Ethereum => Self::eth(ident.identifier)?,
      PublicIdentifierKind::Passkey => Self::passkey_str(&ident.identifier, ident.relying_partner)?,
    };
    Ok(ident)
  }
}

pub trait IdentityExt<T, U> {
  fn to_internal(self) -> Result<Vec<U>, ErrorWrapper<IdentifierValidationError>>;
}

impl IdentityExt<PublicIdentifier, XMTPPublicIdentifier> for Vec<PublicIdentifier> {
  fn to_internal(
    self,
  ) -> Result<Vec<XMTPPublicIdentifier>, ErrorWrapper<IdentifierValidationError>> {
    let ident: Result<Vec<_>, ErrorWrapper<IdentifierValidationError>> =
      self.into_iter().map(|ident| ident.try_into()).collect();
    ident
  }
}
