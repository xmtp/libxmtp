use serde::Serialize;
use wasm_bindgen::{prelude::wasm_bindgen, JsError};
use xmtp_id::associations::{ident, PublicIdentifier as XMTPPublicIdentifier};

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone, Debug, Hash, PartialEq, Eq, Serialize)]
pub struct PublicIdentifier {
  pub identifier: String,
  pub identifier_kind: PublicIdentifierKind,
  pub relying_partner: Option<String>,
}

#[wasm_bindgen]
#[derive(Clone, Debug, Hash, PartialEq, Eq, Serialize)]
pub enum PublicIdentifierKind {
  Ethereum,
  Passkey,
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
  type Error = JsError;
  fn try_from(ident: PublicIdentifier) -> Result<Self, Self::Error> {
    let ident = match ident.identifier_kind {
      PublicIdentifierKind::Ethereum => Self::eth(ident.identifier)?,
      PublicIdentifierKind::Passkey => Self::passkey_str(&ident.identifier, ident.relying_partner)?,
    };
    Ok(ident)
  }
}

pub trait IdentityExt<T, U> {
  fn to_internal(self) -> Result<Vec<U>, JsError>;
}

impl IdentityExt<PublicIdentifier, XMTPPublicIdentifier> for Vec<PublicIdentifier> {
  fn to_internal(self) -> Result<Vec<XMTPPublicIdentifier>, JsError> {
    let ident: Result<Vec<_>, JsError> = self.into_iter().map(|ident| ident.try_into()).collect();
    ident
  }
}
