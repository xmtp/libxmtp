use serde::Serialize;
use wasm_bindgen::{prelude::wasm_bindgen, JsError};
use xmtp_id::associations::{ident, RootIdentifier as XmtpRootIdentifier};

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone, Debug, Hash, PartialEq, Eq, Serialize)]
pub struct RootIdentifier {
  identifier: String,
  identifier_kind: RootIdentifierKind,
}

#[wasm_bindgen]
#[derive(Clone, Debug, Hash, PartialEq, Eq, Serialize)]
pub enum RootIdentifierKind {
  Ethereum,
  Passkey,
}

impl From<XmtpRootIdentifier> for RootIdentifier {
  fn from(ident: XmtpRootIdentifier) -> Self {
    match ident {
      XmtpRootIdentifier::Ethereum(ident::Ethereum(addr)) => Self {
        identifier: addr,
        identifier_kind: RootIdentifierKind::Ethereum,
      },
      XmtpRootIdentifier::Passkey(ident::Passkey(key)) => Self {
        identifier: hex::encode(key),
        identifier_kind: RootIdentifierKind::Passkey,
      },
    }
  }
}
impl TryFrom<RootIdentifier> for XmtpRootIdentifier {
  type Error = JsError;
  fn try_from(ident: RootIdentifier) -> Result<Self, Self::Error> {
    let ident = match ident.identifier_kind {
      RootIdentifierKind::Ethereum => {
        Self::eth(ident.identifier).map_err(|e| JsError::new(&e.to_string()))?
      }
      RootIdentifierKind::Passkey => {
        Self::passkey(hex::decode(ident.identifier).map_err(|e| JsError::new(&e.to_string()))?)
      }
    };
    Ok(ident)
  }
}
