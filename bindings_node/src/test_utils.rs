#![allow(clippy::unwrap_used)]
use crate::{
  client::{create_client, Client, LogOptions, SyncWorkerMode},
  identity::Identifier,
};
use napi::bindgen_prelude::Uint8Array;
use napi_derive::napi;
use std::sync::atomic::Ordering;
use toxiproxy_rust::proxy::{Proxy, ProxyPack};
use xmtp_mls::utils::{TOXIPROXY, TOXI_PORT};

#[napi]
pub struct TestClient {
  inner: Client,
  proxy: Proxy,
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
  let toxiproxy = TOXIPROXY
    .get_or_init(|| async {
      let toxiproxy = toxiproxy_rust::client::Client::new("0.0.0.0:8474");
      toxiproxy.reset().await.unwrap();
      toxiproxy
    })
    .await;

  let port = TOXI_PORT.fetch_add(1, Ordering::SeqCst);

  let result = toxiproxy
    .populate(vec![
      ProxyPack::new(
        format!("Proxy {port}"),
        format!("[::]:{port}"),
        format!("node:{}", "5556"),
      )
      .await,
    ])
    .await
    .unwrap();

  let proxy = result.into_iter().next().unwrap();
  let api_addr = format!("http://localhost:{port}");

  let c = create_client(
    api_addr,
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
      .with_timeout(stream, duration, toxicity as f32)
      .await;
  }

  #[napi]
  pub async fn delete_all_toxics(&self) {
    self.proxy.delete_all_toxics().await.unwrap()
  }
}
