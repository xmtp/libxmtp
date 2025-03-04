use crate::ErrorWrapper;
use napi_derive::napi;
use xmtp_cryptography::signature::IdentifierValidationError;
use xmtp_id::associations::{ident, PublicIdentifier as XMTPPublicIdentifier};

#[napi(object)]
#[derive(Clone, Hash, PartialEq, Eq)]
pub struct Identifier {
  pub identifier: String,
  pub identifier_kind: IdentifierKind,
  pub relying_partner: Option<String>,
}

#[napi]
#[derive(Hash, PartialEq, Eq)]
pub enum IdentifierKind {
  Ethereum,
  Passkey,
  // more to come...
}

impl From<XMTPPublicIdentifier> for Identifier {
  fn from(ident: XMTPPublicIdentifier) -> Self {
    match ident {
      XMTPPublicIdentifier::Ethereum(ident::Ethereum(addr)) => Self {
        identifier: addr,
        identifier_kind: IdentifierKind::Ethereum,
        relying_partner: None,
      },
      XMTPPublicIdentifier::Passkey(ident::Passkey {
        key,
        relying_partner,
      }) => Self {
        identifier: hex::encode(key),
        identifier_kind: IdentifierKind::Passkey,
        relying_partner,
      },
    }
  }
}

impl TryFrom<Identifier> for XMTPPublicIdentifier {
  type Error = ErrorWrapper<IdentifierValidationError>;
  fn try_from(ident: Identifier) -> Result<Self, Self::Error> {
    let ident = match ident.identifier_kind {
      IdentifierKind::Ethereum => Self::eth(ident.identifier)?,
      IdentifierKind::Passkey => Self::passkey_str(&ident.identifier, ident.relying_partner)?,
    };
    Ok(ident)
  }
}

pub trait IdentityExt<T, U> {
  fn to_internal(self) -> Result<Vec<U>, ErrorWrapper<IdentifierValidationError>>;
}

impl IdentityExt<Identifier, XMTPPublicIdentifier> for Vec<Identifier> {
  fn to_internal(
    self,
  ) -> Result<Vec<XMTPPublicIdentifier>, ErrorWrapper<IdentifierValidationError>> {
    let ident: Result<Vec<_>, ErrorWrapper<IdentifierValidationError>> =
      self.into_iter().map(|ident| ident.try_into()).collect();
    ident
  }
}
