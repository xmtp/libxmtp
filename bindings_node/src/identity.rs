use crate::ErrorWrapper;
use napi_derive::napi;
use xmtp_cryptography::signature::IdentifierValidationError;
use xmtp_id::associations::{ident, PublicIdentifier as XMTPPublicIdentifier};

#[napi(object)]
#[derive(Clone, Hash, PartialEq, Eq)]
pub struct PublicIdentifier {
  pub identifier: String,
  pub identifier_kind: PublicIdentifierKind,
}

#[napi]
#[derive(Hash, PartialEq, Eq)]
pub enum PublicIdentifierKind {
  Ethereum,
  Passkey,
  // more to come...
}

#[napi(object)]
#[derive(Clone)]
/// These are just the PublicIdentifiers that can sign.
/// Strictly an FFI param type to ensure integrators don't try to
/// sign with identifier kinds that can't sign.
pub struct RootIdentifier {
  pub identifier: String,
  pub identifier_kind: RootIdentifierKind,
}
#[napi]
pub enum RootIdentifierKind {
  Ethereum,
  Passkey,
}

impl RootIdentifier {
  pub fn to_public(self) -> PublicIdentifier {
    self.into()
  }
}

impl From<RootIdentifier> for PublicIdentifier {
  fn from(ident: RootIdentifier) -> Self {
    Self {
      identifier: ident.identifier,
      identifier_kind: ident.identifier_kind.into(),
    }
  }
}
impl From<PublicIdentifier> for RootIdentifier {
  fn from(ident: PublicIdentifier) -> Self {
    Self {
      identifier: ident.identifier,
      identifier_kind: ident.identifier_kind.into(),
    }
  }
}

impl From<RootIdentifierKind> for PublicIdentifierKind {
  fn from(kind: RootIdentifierKind) -> Self {
    match kind {
      RootIdentifierKind::Ethereum => Self::Ethereum,
      RootIdentifierKind::Passkey => Self::Passkey,
    }
  }
}
impl From<PublicIdentifierKind> for RootIdentifierKind {
  fn from(kind: PublicIdentifierKind) -> Self {
    match kind {
      PublicIdentifierKind::Ethereum => Self::Ethereum,
      PublicIdentifierKind::Passkey => Self::Passkey,
    }
  }
}

impl From<XMTPPublicIdentifier> for PublicIdentifier {
  fn from(ident: XMTPPublicIdentifier) -> Self {
    match ident {
      XMTPPublicIdentifier::Ethereum(ident::Ethereum(addr)) => Self {
        identifier: addr,
        identifier_kind: PublicIdentifierKind::Ethereum,
      },
      XMTPPublicIdentifier::Passkey(ident::Passkey(key)) => Self {
        identifier: hex::encode(key),
        identifier_kind: PublicIdentifierKind::Passkey,
      },
    }
  }
}

impl TryFrom<PublicIdentifier> for XMTPPublicIdentifier {
  type Error = ErrorWrapper<IdentifierValidationError>;
  fn try_from(ident: PublicIdentifier) -> Result<Self, Self::Error> {
    let ident = match ident.identifier_kind {
      PublicIdentifierKind::Ethereum => Self::eth(ident.identifier)?,
      PublicIdentifierKind::Passkey => Self::passkey_str(&ident.identifier)?,
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
    Ok(ident?)
  }
}
