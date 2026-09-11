use bindings_wasm_macros::wasm_bindgen_numbered_enum;
use serde::{Deserialize, Serialize};
use tsify::Tsify;
use wasm_bindgen::{JsError, prelude::wasm_bindgen};
use xmtp_id::associations::{Identifier as XmtpIdentifier, ident};

#[derive(Tsify, Clone, Debug, Hash, PartialEq, Eq, Serialize, Deserialize)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct Identifier {
  pub identifier: String,
  #[serde(rename = "identifierKind")]
  pub identifier_kind: IdentifierKind,
}

#[wasm_bindgen_numbered_enum]
#[derive(Hash)]
pub enum IdentifierKind {
  Ethereum = 0,
  Passkey = 1,
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

#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi, large_number_types_as_bigints)]
#[serde(rename_all = "camelCase")]
pub struct ApiStats {
  pub publish: u64,
  pub query: u64,
  pub query_newest: u64,
  pub subscribe: u64,
  pub subscribe_static: u64,
}

impl From<xmtp_proto::api_client::ApiStats> for ApiStats {
  fn from(stats: xmtp_proto::api_client::ApiStats) -> Self {
    Self {
      publish: stats.publish.get_count() as u64,
      query: stats.query.get_count() as u64,
      query_newest: stats.query_newest.get_count() as u64,
      subscribe: stats.subscribe.get_count() as u64,
      subscribe_static: stats.subscribe_static.get_count() as u64,
    }
  }
}

#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi, large_number_types_as_bigints)]
#[serde(rename_all = "camelCase")]
pub struct IdentityStats {
  pub get_inbox_ids: u64,
  pub verify_smart_contract_wallet_signatures: u64,
}

impl From<xmtp_proto::api_client::IdentityStats> for IdentityStats {
  fn from(stats: xmtp_proto::api_client::IdentityStats) -> Self {
    Self {
      get_inbox_ids: stats.get_inbox_ids.get_count() as u64,
      verify_smart_contract_wallet_signatures: stats
        .verify_smart_contract_wallet_signatures
        .get_count() as u64,
    }
  }
}
