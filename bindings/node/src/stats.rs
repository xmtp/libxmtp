use napi::bindgen_prelude::BigInt;
use napi_derive::napi;

#[napi(object)]
pub struct ApiStats {
  pub publish: BigInt,
  pub query: BigInt,
  pub query_newest: BigInt,
  pub get: BigInt,
  pub subscribe: BigInt,
  pub subscribe_static: BigInt,
}

impl From<xmtp_proto::api_client::ApiStats> for ApiStats {
  fn from(stats: xmtp_proto::api_client::ApiStats) -> Self {
    Self {
      publish: BigInt::from(stats.publish.get_count() as u64),
      query: BigInt::from(stats.query.get_count() as u64),
      query_newest: BigInt::from(stats.query_newest.get_count() as u64),
      get: BigInt::from(stats.get.get_count() as u64),
      subscribe: BigInt::from(stats.subscribe.get_count() as u64),
      subscribe_static: BigInt::from(stats.subscribe_static.get_count() as u64),
    }
  }
}

#[napi(object)]
pub struct IdentityStats {
  pub get_inbox_ids: BigInt,
  pub verify_smart_contract_wallet_signatures: BigInt,
}

impl From<xmtp_proto::api_client::IdentityStats> for IdentityStats {
  fn from(stats: xmtp_proto::api_client::IdentityStats) -> Self {
    Self {
      get_inbox_ids: BigInt::from(stats.get_inbox_ids.get_count() as u64),
      verify_smart_contract_wallet_signatures: BigInt::from(
        stats.verify_smart_contract_wallet_signatures.get_count() as u64,
      ),
    }
  }
}
