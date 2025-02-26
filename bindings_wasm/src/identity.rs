use serde::Serialize;
use wasm_bindgen::{prelude::wasm_bindgen, JsError};
use xmtp_id::associations::{ident, PublicIdentifier as XMTPPublicIdentifier};

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone, Debug, Hash, PartialEq, Eq, Serialize)]
pub struct PublicIdentifier {
  pub identifier: String,
  pub identifier_kind: PublicIdentifierKind,
}

#[wasm_bindgen]
#[derive(Clone, Debug, Hash, PartialEq, Eq, Serialize)]
pub enum PublicIdentifierKind {
  Ethereum,
  Passkey,
}

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone, Debug, Hash, PartialEq, Eq, Serialize)]
pub struct RootIdentifier {
  pub identifier: String,
  pub identifier_kind: RootIdentifierKind,
}

#[wasm_bindgen]
#[derive(Clone, Debug, Hash, PartialEq, Eq, Serialize)]
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

impl From<RootIdentifierKind> for PublicIdentifierKind {
  fn from(kind: RootIdentifierKind) -> Self {
    match kind {
      RootIdentifierKind::Ethereum => Self::Ethereum,
      RootIdentifierKind::Passkey => Self::Passkey,
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
  type Error = JsError;
  fn try_from(ident: PublicIdentifier) -> Result<Self, Self::Error> {
    let ident = match ident.identifier_kind {
      PublicIdentifierKind::Ethereum => Self::eth(ident.identifier)?,
      PublicIdentifierKind::Passkey => Self::passkey_str(&ident.identifier)?,
    };
    Ok(ident)
  }
}
impl TryFrom<PublicIdentifier> for RootIdentifier {
  type Error = JsError;
  fn try_from(ident: PublicIdentifier) -> Result<Self, Self::Error> {
    let ident = Self {
      identifier: ident.identifier,

      identifier_kind: ident.identifier_kind.try_into()?,
    };
    Ok(ident)
  }
}
impl TryFrom<PublicIdentifierKind> for RootIdentifierKind {
  type Error = JsError;
  fn try_from(kind: PublicIdentifierKind) -> Result<Self, Self::Error> {
    let kind = match kind {
      PublicIdentifierKind::Ethereum => Self::Ethereum,
      PublicIdentifierKind::Passkey => Self::Passkey,
    };
    Ok(kind)
  }
}

pub trait IdentityExt<T, U> {
  fn to_internal(self) -> Result<Vec<U>, JsError>;
}

impl IdentityExt<PublicIdentifier, XMTPPublicIdentifier> for Vec<PublicIdentifier> {
  fn to_internal(self) -> Result<Vec<XMTPPublicIdentifier>, JsError> {
    let ident: Result<Vec<_>, JsError> = self.into_iter().map(|ident| ident.try_into()).collect();
    Ok(ident?)
  }
}
