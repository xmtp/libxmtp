use super::XmtpEnv;
use super::gateway_auth::{AuthCallback, AuthHandle};
use std::sync::Arc;
use wasm_bindgen::prelude::*;
use xmtp_api_d14n::{ClientBundleBuilder, validate_and_resolve};

#[xmtp_macro::wasm_builder]
pub struct BackendBuilder {
  #[builder(required)]
  pub env: XmtpEnv,

  api_url: Option<String>,

  gateway_host: Option<String>,

  pub readonly: Option<bool>,

  app_version: Option<String>,

  #[builder(skip)]
  auth_callback: Option<AuthCallback>,

  #[builder(skip)]
  auth_handle: Option<AuthHandle>,
}

#[wasm_bindgen]
impl BackendBuilder {
  #[wasm_bindgen(js_name = "authCallback")]
  pub fn auth_callback(&mut self, callback: AuthCallback) {
    self.auth_callback = Some(callback);
  }

  #[wasm_bindgen(js_name = "authHandle")]
  pub fn auth_handle(&mut self, handle: AuthHandle) {
    self.auth_handle = Some(handle);
  }

  #[wasm_bindgen]
  pub fn build(mut self) -> Result<Backend, JsError> {
    let config = validate_and_resolve(
      self.env.into(),
      self.api_url.clone(),
      self.gateway_host.clone(),
      self.readonly.unwrap_or(false),
      self.app_version.clone(),
      self.auth_callback.is_some() || self.auth_handle.is_some(),
    )
    .map_err(|e| JsError::new(&e.to_string()))?;

    let app_version = config.app_version.clone();
    let mut builder = ClientBundleBuilder::default();
    if let Some(url) = &config.api_url {
      builder.v3_host(url);
    }
    if let Some(host) = &config.gateway_host {
      builder.gateway_host(host);
    }
    builder
      .is_secure(config.is_secure)
      .readonly(config.readonly)
      .app_version(config.app_version)
      .maybe_auth_callback(
        self
          .auth_callback
          .take()
          .map(|c| Arc::new(c) as Arc<dyn xmtp_api_d14n::AuthCallback>),
      )
      .maybe_auth_handle(self.auth_handle.take().map(|h| h.handle));

    let bundle = builder.build().map_err(|e| JsError::new(&e.to_string()))?;
    Ok(Backend {
      bundle,
      env: self.env,
      v3_host: config.api_url,
      gateway_host: config.gateway_host,
      app_version,
    })
  }
}

#[wasm_bindgen]
#[derive(Clone)]
pub struct Backend {
  pub(crate) bundle: xmtp_mls::XmtpClientBundle,
  env: XmtpEnv,
  v3_host: Option<String>,
  gateway_host: Option<String>,
  app_version: String,
}

#[wasm_bindgen]
impl Backend {
  #[wasm_bindgen(getter)]
  pub fn env(&self) -> XmtpEnv {
    self.env
  }

  #[wasm_bindgen(getter, js_name = "v3Host")]
  pub fn v3_host(&self) -> Option<String> {
    self.v3_host.clone()
  }

  #[wasm_bindgen(getter, js_name = "gatewayHost")]
  pub fn gateway_host(&self) -> Option<String> {
    self.gateway_host.clone()
  }

  #[wasm_bindgen(getter, js_name = "appVersion")]
  pub fn app_version(&self) -> String {
    self.app_version.clone()
  }
}

/// Create a client from a pre-built Backend.
///
/// The Backend encapsulates all API configuration (env, hosts, auth, TLS).
/// This function only needs identity and database configuration.
#[wasm_bindgen(js_name = createClientWithBackend)]
#[allow(clippy::too_many_arguments)]
pub async fn create_client_with_backend(
  backend: &Backend,
  #[wasm_bindgen(js_name = inboxId)] inbox_id: String,
  #[wasm_bindgen(js_name = accountIdentifier)] account_identifier: crate::identity::Identifier,
  #[wasm_bindgen(js_name = dbPath)] db_path: Option<String>,
  #[wasm_bindgen(js_name = encryptionKey)] encryption_key: Option<js_sys::Uint8Array>,
  #[wasm_bindgen(js_name = deviceSyncMode)] device_sync_worker_mode: Option<super::DeviceSyncMode>,
  #[wasm_bindgen(js_name = logOptions)] log_options: Option<super::LogOptions>,
  #[wasm_bindgen(js_name = allowOffline)] allow_offline: Option<bool>,
  nonce: Option<u64>,
) -> Result<super::Client, JsError> {
  super::init_logging(log_options.unwrap_or_default())?;

  let store = super::build_store(db_path, encryption_key).await?;

  let cursor_store = xmtp_mls::cursor_store::SqliteCursorStore::new(store.db());
  let mut mbb = xmtp_api_d14n::MessageBackendBuilder::default();
  mbb.cursor_store(cursor_store);
  let api_client = mbb
    .clone()
    .from_bundle(backend.bundle.clone())
    .map_err(|e| JsError::new(&e.to_string()))?;
  let sync_api_client = mbb
    .from_bundle(backend.bundle.clone())
    .map_err(|e| JsError::new(&e.to_string()))?;

  super::create_client_inner(
    api_client,
    sync_api_client,
    store,
    inbox_id,
    account_identifier,
    device_sync_worker_mode,
    allow_offline,
    Some(backend.app_version()),
    nonce.unwrap_or(1),
  )
  .await
}
