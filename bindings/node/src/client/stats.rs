use crate::{
  client::Client,
  stats::{ApiStats, IdentityStats},
};
use napi_derive::napi;
use xmtp_proto::api_client::AggregateStats;

#[napi]
impl Client {
  #[napi]
  pub fn api_statistics(&self) -> ApiStats {
    self.inner_client.api_stats().into()
  }

  #[napi]
  pub fn api_identity_statistics(&self) -> IdentityStats {
    self.inner_client.identity_api_stats().into()
  }

  #[napi]
  pub fn api_aggregate_statistics(&self) -> String {
    let api = self.inner_client.api_stats();
    let identity = self.inner_client.identity_api_stats();
    let aggregate = AggregateStats { mls: api, identity };
    format!("{:?}", aggregate)
  }

  #[napi]
  pub fn clear_all_statistics(&self) {
    self.inner_client.clear_stats()
  }
}
