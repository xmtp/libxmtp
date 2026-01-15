#![allow(clippy::unwrap_used)]
use crate::{
  client::{
    Client,
    create_client::create_client,
    options::{LogOptions, SyncWorkerMode},
  },
  identity::Identifier,
};
use napi::bindgen_prelude::Uint8Array;
use napi_derive::napi;
use xmtp_api_grpc::test::ToxicNodeGoClient;
use xmtp_configuration::GrpcUrlsToxic;
use xmtp_mls::ToxicTestClient;
use xmtp_proto::api_client::ToxicProxies;

#[napi]
pub struct TestClient {
  inner: Client,
  proxy: ToxicProxies,
}

#[allow(clippy::too_many_arguments)]
#[napi]
pub async fn create_local_toxic_client(
  db_path: Option<String>,
  inbox_id: String,
  account_identifier: Identifier,
  encryption_key: Option<Uint8Array>,
  device_sync_server_url: Option<String>,
  device_sync_worker_mode: Option<SyncWorkerMode>,
  log_options: Option<LogOptions>,
  allow_offline: Option<bool>,
) -> Result<TestClient, napi::Error> {
  let api_addr = GrpcUrlsToxic::NODE.to_string();
  let proxy = ToxicNodeGoClient::proxies().await;

  let c = create_client(
    api_addr,
    None,
    false,
    db_path,
    inbox_id,
    account_identifier,
    encryption_key,
    device_sync_server_url,
    device_sync_worker_mode,
    log_options,
    allow_offline,
    None,
    None,
    None,
    None,
    None,
  )
  .await?;
  Ok(TestClient { inner: c, proxy })
}

#[napi]
impl TestClient {
  #[napi(getter)]
  pub fn client(&self) -> Client {
    self.inner.clone()
  }

  #[napi]
  pub async fn with_timeout(&self, stream: String, duration: u32, toxicity: f64) {
    self
      .proxy
      .proxy(0)
      .with_timeout(stream, duration, toxicity as f32)
      .await;
  }

  #[napi]
  pub async fn delete_all_toxics(&self) {
    self.proxy.proxy(0).delete_all_toxics().await.unwrap()
  }
}
