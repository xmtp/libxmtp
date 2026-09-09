use super::auth::{AuthCallback, AuthHandle};
use crate::errors::ErrorWrapper;
use std::sync::Arc;
use wasm_bindgen::prelude::*;
use xmtp_api_backend::MessageBackendBuilder;

/// Backend configuration failed. This error is not retryable.
#[derive(Debug, thiserror::Error, xmtp_common::ErrorCode)]
#[error(transparent)]
pub(crate) struct BackendBuilderError(#[from] pub xmtp_api_backend::MessageBackendBuilderError);

#[xmtp_macro::wasm_builder]
pub struct BackendBuilder {
  #[builder(required)]
  pub backend_url: String,

  pub env: Option<String>,

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
    let app_version = self.app_version.clone().unwrap_or_default();
    let mut builder = MessageBackendBuilder::default();
    builder
      .host(&self.backend_url)
      .readonly(self.readonly.unwrap_or_default())
      .app_version(app_version.clone())
      .maybe_auth_callback(
        self
          .auth_callback
          .take()
          .map(|c| Arc::new(c) as Arc<dyn xmtp_api_backend::AuthCallback>),
      )
      .maybe_auth_handle(self.auth_handle.take().map(|h| h.handle));

    let api_client = builder
      .build()
      .map_err(BackendBuilderError)
      .map_err(ErrorWrapper::js)?;
    Ok(Backend {
      api_client,
      env: self.env.clone(),
      backend_url: self.backend_url.clone(),
      app_version: app_version.clone(),
    })
  }
}

#[wasm_bindgen]
#[derive(Clone)]
pub struct Backend {
  pub(crate) api_client: xmtp_mls::XmtpApiClient,
  env: Option<String>,
  backend_url: String,
  app_version: String,
}

#[wasm_bindgen]
impl Backend {
  #[wasm_bindgen(getter)]
  pub fn env(&self) -> Option<String> {
    self.env.clone()
  }

  #[wasm_bindgen(getter, js_name = "backendUrl")]
  pub fn backend_url(&self) -> String {
    self.backend_url.clone()
  }

  /// Key for an SDK cache of API clients. The environment does not change it.
  #[wasm_bindgen(getter, js_name = "cacheKey")]
  pub fn cache_key(&self) -> String {
    format!("{}|{}", self.backend_url, self.app_version)
  }

  #[wasm_bindgen(getter, js_name = "appVersion")]
  pub fn app_version(&self) -> String {
    self.app_version.clone()
  }
}

/// Create a client from a pre-built Backend.
///
/// The Backend holds the backend URL, app version, and authentication.
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
  #[wasm_bindgen(js_name = workerConfig)] worker_config: Option<super::WorkerConfigOptions>,
  #[wasm_bindgen(js_name = logOptions)] log_options: Option<super::LogOptions>,
  #[wasm_bindgen(js_name = allowOffline)] allow_offline: Option<bool>,
  nonce: Option<u64>,
  #[wasm_bindgen(js_name = changeCallbacks)] change_callbacks: Option<
    super::change_callbacks::UnstableChangeCallbacks,
  >,
) -> Result<super::Client, JsError> {
  super::init_logging(log_options.unwrap_or_default())?;

  let store = super::build_store(db_path, encryption_key).await?;

  let api_client = backend.api_client.clone();

  super::create_client_inner(
    api_client,
    store,
    inbox_id,
    account_identifier,
    device_sync_worker_mode,
    worker_config,
    allow_offline,
    Some(backend.app_version()),
    nonce.unwrap_or(1),
    change_callbacks,
  )
  .await
}
