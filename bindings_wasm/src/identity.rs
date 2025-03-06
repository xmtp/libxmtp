use serde::Serialize;
use wasm_bindgen::{prelude::wasm_bindgen, JsError};
use xmtp_id::associations::{ident, Identifier as XmtpIdentifier};

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone, Debug, Hash, PartialEq, Eq, Serialize)]
pub struct Identifier {
  pub identifier: String,
  pub identifier_kind: IdentifierKind,
  pub relying_partner: Option<String>,
}

#[wasm_bindgen]
#[derive(Clone, Debug, Hash, PartialEq, Eq, Serialize)]
pub enum IdentifierKind {
  Ethereum,
  Passkey,
}

impl From<XmtpIdentifier> for Identifier {
  fn from(ident: XmtpIdentifier) -> Self {
    match ident {
      XmtpIdentifier::Ethereum(ident::Ethereum(addr)) => Self {
        identifier: addr,
        identifier_kind: IdentifierKind::Ethereum,
        relying_partner: None,
      },
      XmtpIdentifier::Passkey(ident::Passkey {
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

impl TryFrom<Identifier> for XmtpIdentifier {
  type Error = JsError;
  fn try_from(ident: Identifier) -> Result<Self, Self::Error> {
    let ident = match ident.identifier_kind {
      IdentifierKind::Ethereum => Self::eth(ident.identifier)?,
      IdentifierKind::Passkey => Self::passkey_str(&ident.identifier, ident.relying_partner)?,
    };
    Ok(ident)
  }
}

pub trait IdentityExt<T, U> {
  fn to_internal(self) -> Result<Vec<U>, JsError>;
}

impl IdentityExt<Identifier, XmtpIdentifier> for Vec<Identifier> {
  fn to_internal(self) -> Result<Vec<XmtpIdentifier>, JsError> {
    let ident: Result<Vec<_>, JsError> = self.into_iter().map(|ident| ident.try_into()).collect();
    ident
  }
}
