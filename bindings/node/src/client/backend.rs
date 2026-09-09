use crate::ErrorWrapper;
use crate::client::auth::{AuthCallback, AuthHandle};
use napi::bindgen_prelude::Result;
use napi_derive::napi;
use std::sync::{Arc, Mutex};
use xmtp_api_backend::MessageBackendBuilder;

/// Backend configuration failed. This error is not retryable.
#[derive(Debug, thiserror::Error, xmtp_common::ErrorCode)]
#[error(transparent)]
pub(crate) struct BackendBuilderError(#[from] pub xmtp_api_backend::MessageBackendBuilderError);

#[xmtp_macro::napi_builder]
pub struct BackendBuilder {
  #[builder(required)]
  pub backend_url: String,

  pub env: Option<String>,

  pub readonly: Option<bool>,

  pub app_version: Option<String>,

  #[builder(skip)]
  auth_callback: Mutex<Option<AuthCallback>>,

  #[builder(skip)]
  auth_handle: Mutex<Option<AuthHandle>>,

  #[builder(skip)]
  consumed: Mutex<bool>,
}

#[napi]
impl BackendBuilder {
  #[napi]
  pub fn auth_callback(&mut self, callback: &AuthCallback) {
    *self.auth_callback.lock().expect("lock poisoned") = Some(callback.clone());
  }

  #[napi(js_name = "authHandle")]
  pub fn auth_handle(&mut self, handle: &AuthHandle) {
    *self.auth_handle.lock().expect("lock poisoned") = Some(handle.clone());
  }

  #[napi]
  #[xmtp_common::err_span]
  pub async fn build(&self) -> Result<Backend> {
    // Ensure the rustls crypto provider is installed before any TLS/HTTP client is built,
    // independent of whether the `#[ctor::ctor(unsafe)]` in `xmtp_cryptography` fired. Idempotent.
    // See issue #3846.
    xmtp_cryptography::install_crypto_provider();
    {
      let mut consumed = self
        .consumed
        .lock()
        .map_err(|_| napi::Error::from_reason("BackendBuilder lock poisoned"))?;
      if *consumed {
        return Err(napi::Error::from_reason(
          "BackendBuilder has already been consumed by build()",
        ));
      }
      *consumed = true;
    }
    let auth_callback = self
      .auth_callback
      .lock()
      .map_err(|_| napi::Error::from_reason("BackendBuilder lock poisoned"))?
      .take();
    let auth_handle = self
      .auth_handle
      .lock()
      .map_err(|_| napi::Error::from_reason("BackendBuilder lock poisoned"))?
      .take();

    let app_version = self.app_version.clone().unwrap_or_default();
    let mut builder = MessageBackendBuilder::default();
    builder
      .host(&self.backend_url)
      .readonly(self.readonly.unwrap_or(false))
      .app_version(app_version.clone())
      .maybe_auth_callback(
        auth_callback.map(|c| Arc::new(c) as Arc<dyn xmtp_api_backend::AuthCallback>),
      )
      .maybe_auth_handle(auth_handle.map(|h: AuthHandle| h.into()));

    let api_client = builder
      .build()
      .map_err(BackendBuilderError)
      .map_err(ErrorWrapper::from)?;
    Ok(Backend {
      api_client,
      env: self.env.clone(),
      backend_url: self.backend_url.clone(),
      app_version,
    })
  }
}

#[napi]
#[derive(Clone)]
pub struct Backend {
  pub(crate) api_client: xmtp_mls::XmtpApiClient,
  env: Option<String>,
  backend_url: String,
  app_version: String,
}

#[napi]
impl Backend {
  #[napi(getter)]
  pub fn env(&self) -> Option<String> {
    self.env.clone()
  }

  #[napi(getter, js_name = "backendUrl")]
  pub fn backend_url(&self) -> String {
    self.backend_url.clone()
  }

  /// Key for an SDK cache of API clients. The environment does not change it.
  #[napi(getter, js_name = "cacheKey")]
  pub fn cache_key(&self) -> String {
    format!("{}|{}", self.backend_url, self.app_version)
  }

  #[napi(getter, js_name = "appVersion")]
  pub fn app_version(&self) -> String {
    self.app_version.clone()
  }
}
