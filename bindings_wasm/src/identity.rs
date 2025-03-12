use serde::{Deserialize, Serialize};
use tsify_next::Tsify;
use wasm_bindgen::{prelude::wasm_bindgen, JsError};
use xmtp_id::associations::{ident, Identifier as XmtpIdentifier};

#[derive(Tsify, Clone, Debug, Hash, PartialEq, Eq, Serialize, Deserialize)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct Identifier {
  pub identifier: String,
  #[serde(rename = "identifierKind")]
  pub identifier_kind: IdentifierKind,
}

#[wasm_bindgen]
impl Identifier {
  pub fn new(
    identifier: String,
    #[wasm_bindgen(js_name = identifierKind)] identifier_kind: IdentifierKind,
  ) -> Self {
    Self {
      identifier,
      identifier_kind,
    }
  }
}

#[derive(Tsify, Clone, Debug, Hash, PartialEq, Eq, Serialize, Deserialize)]
#[tsify(into_wasm_abi, from_wasm_abi)]
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
      },
      XmtpIdentifier::Passkey(ident::Passkey { key, .. }) => Self {
        identifier: hex::encode(key),
        identifier_kind: IdentifierKind::Passkey,
      },
    }
  }
}

impl TryFrom<Identifier> for XmtpIdentifier {
  type Error = JsError;
  fn try_from(ident: Identifier) -> Result<Self, Self::Error> {
    let ident = match ident.identifier_kind {
      IdentifierKind::Ethereum => Self::eth(ident.identifier)?,
      IdentifierKind::Passkey => Self::passkey_str(&ident.identifier, None)?,
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
