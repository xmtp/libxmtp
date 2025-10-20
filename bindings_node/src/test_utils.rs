#![allow(clippy::unwrap_used)]
use crate::{
  client::{Client, LogOptions, SyncWorkerMode, create_client},
  identity::Identifier,
};
use napi::bindgen_prelude::Uint8Array;
use napi_derive::napi;
use xmtp_proto::{ToxicProxies, init_toxi};

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
  disable_events: Option<bool>,
) -> Result<TestClient, napi::Error> {
  let proxy = init_toxi(&["http://localhost:5556"]).await;
  let api_addr = format!("http://localhost:{}", proxy.port(0));

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
    disable_events,
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
