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

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
pub struct ApiStats {
  pub upload_key_package: u64,
  pub fetch_key_package: u64,
  pub send_group_messages: u64,
  pub send_welcome_messages: u64,
  pub query_group_messages: u64,
  pub query_welcome_messages: u64,
}

#[wasm_bindgen]
impl ApiStats {
  #[wasm_bindgen(constructor)]
  pub fn new(
    upload_key_package: u64,
    fetch_key_package: u64,
    send_group_messages: u64,
    send_welcome_messages: u64,
    query_group_messages: u64,
    query_welcome_messages: u64,
  ) -> Self {
    Self {
      upload_key_package,
      fetch_key_package,
      send_group_messages,
      send_welcome_messages,
      query_group_messages,
      query_welcome_messages,
    }
  }
}

impl From<xmtp_proto::api_client::ApiStats> for ApiStats {
  fn from(stats: xmtp_proto::api_client::ApiStats) -> Self {
    Self {
      upload_key_package: stats.upload_key_package.get_count() as u64,
      fetch_key_package: stats.fetch_key_package.get_count() as u64,
      send_group_messages: stats.send_group_messages.get_count() as u64,
      send_welcome_messages: stats.send_welcome_messages.get_count() as u64,
      query_group_messages: stats.query_group_messages.get_count() as u64,
      query_welcome_messages: stats.query_welcome_messages.get_count() as u64,
    }
  }
}

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
pub struct IdentityStats {
  pub publish_identity_update: u64,
  pub get_identity_updates_v2: u64,
  pub get_inbox_ids: u64,
  pub verify_smart_contract_wallet_signature: u64,
}

#[wasm_bindgen]
impl IdentityStats {
  #[wasm_bindgen(constructor)]
  pub fn new(
    publish_identity_update: u64,
    get_identity_updates_v2: u64,
    get_inbox_ids: u64,
    verify_smart_contract_wallet_signature: u64,
  ) -> Self {
    Self {
      publish_identity_update,
      get_identity_updates_v2,
      get_inbox_ids,
      verify_smart_contract_wallet_signature,
    }
  }
}

impl From<xmtp_proto::api_client::IdentityStats> for IdentityStats {
  fn from(stats: xmtp_proto::api_client::IdentityStats) -> Self {
    Self {
      publish_identity_update: stats.publish_identity_update.get_count() as u64,
      get_identity_updates_v2: stats.get_identity_updates_v2.get_count() as u64,
      get_inbox_ids: stats.get_inbox_ids.get_count() as u64,
      verify_smart_contract_wallet_signature: stats
        .verify_smart_contract_wallet_signature
        .get_count() as u64,
    }
  }
}

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
pub struct StreamStats {
  pub subscribe_messages: u64,
  pub subscribe_welcomes: u64,
}

#[wasm_bindgen]
impl StreamStats {
  #[wasm_bindgen(constructor)]
  pub fn new(
    subscribe_messages: u64,
    subscribe_welcomes: u64,
  ) -> Self {
    Self {
      subscribe_messages,
      subscribe_welcomes,
    }
  }
}

impl From<xmtp_proto::api_client::ApiStats> for StreamStats {
  fn from(stats: xmtp_proto::api_client::ApiStats) -> Self {
    Self {
      subscribe_messages: stats.subscribe_messages.get_count() as u64,
      subscribe_welcomes: stats.subscribe_welcomes.get_count() as u64,
    }
  }
}
