use napi::bindgen_prelude::BigInt;
use napi_derive::napi;

#[napi(object)]
pub struct ApiStats {
  pub upload_key_package: BigInt,
  pub fetch_key_package: BigInt,
  pub send_group_messages: BigInt,
  pub send_welcome_messages: BigInt,
  pub query_group_messages: BigInt,
  pub query_welcome_messages: BigInt,
  pub subscribe_messages: BigInt,
  pub subscribe_welcomes: BigInt,
}

impl From<xmtp_proto::api_client::ApiStats> for ApiStats {
  fn from(stats: xmtp_proto::api_client::ApiStats) -> Self {
    Self {
      upload_key_package: BigInt::from(stats.upload_key_package.get_count() as u64),
      fetch_key_package: BigInt::from(stats.fetch_key_package.get_count() as u64),
      send_group_messages: BigInt::from(stats.send_group_messages.get_count() as u64),
      send_welcome_messages: BigInt::from(stats.send_welcome_messages.get_count() as u64),
      query_group_messages: BigInt::from(stats.query_group_messages.get_count() as u64),
      query_welcome_messages: BigInt::from(stats.query_welcome_messages.get_count() as u64),
      subscribe_messages: BigInt::from(stats.subscribe_messages.get_count() as u64),
      subscribe_welcomes: BigInt::from(stats.subscribe_welcomes.get_count() as u64),
    }
  }
}

#[napi(object)]
pub struct IdentityStats {
  pub publish_identity_update: BigInt,
  pub get_identity_updates_v2: BigInt,
  pub get_inbox_ids: BigInt,
  pub verify_smart_contract_wallet_signature: BigInt,
}

impl From<xmtp_proto::api_client::IdentityStats> for IdentityStats {
  fn from(stats: xmtp_proto::api_client::IdentityStats) -> Self {
    Self {
      publish_identity_update: BigInt::from(stats.publish_identity_update.get_count() as u64),
      get_identity_updates_v2: BigInt::from(stats.get_identity_updates_v2.get_count() as u64),
      get_inbox_ids: BigInt::from(stats.get_inbox_ids.get_count() as u64),
      verify_smart_contract_wallet_signature: BigInt::from(
        stats.verify_smart_contract_wallet_signature.get_count() as u64,
      ),
    }
  }
}
