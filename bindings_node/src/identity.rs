use crate::ErrorWrapper;
use napi_derive::napi;
use xmtp_cryptography::signature::IdentifierValidationError;
use xmtp_id::associations::{ident, Identifier as XmtpIdentifier};

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
