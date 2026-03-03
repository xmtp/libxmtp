use crate::ErrorWrapper;
use crate::client::gateway_auth::{AuthCallback, AuthHandle};
use crate::client::options::XmtpEnv;
use napi::bindgen_prelude::Result;
use napi_derive::napi;
use std::sync::{Arc, Mutex};
use xmtp_api_d14n::{ClientBundleBuilder, validate_and_resolve};

#[xmtp_macro::napi_builder]
pub struct BackendBuilder {
  #[builder(required)]
  pub env: XmtpEnv,

  pub api_url: Option<String>,

  pub gateway_host: Option<String>,

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
  pub async fn build(&self) -> Result<Backend> {
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

    let has_auth = {
      let cb = self
        .auth_callback
        .lock()
        .map_err(|_| napi::Error::from_reason("BackendBuilder lock poisoned"))?;
      let ah = self
        .auth_handle
        .lock()
        .map_err(|_| napi::Error::from_reason("BackendBuilder lock poisoned"))?;
      cb.is_some() || ah.is_some()
    };

    let config = validate_and_resolve(
      self.env.into(),
      self.api_url.clone(),
      self.gateway_host.clone(),
      self.readonly.unwrap_or(false),
      self.app_version.clone(),
      has_auth,
    )
    .map_err(ErrorWrapper::from)?;

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
        auth_callback.map(|c| Arc::new(c) as Arc<dyn xmtp_api_d14n::AuthCallback>),
      )
      .maybe_auth_handle(auth_handle.map(|h: AuthHandle| h.into()));

    let bundle = builder.build().map_err(ErrorWrapper::from)?;
    Ok(Backend {
      bundle,
      env: self.env,
      v3_host: config.api_url,
      gateway_host: config.gateway_host,
      app_version,
    })
  }
}

#[napi]
#[derive(Clone)]
pub struct Backend {
  pub(crate) bundle: xmtp_mls::XmtpClientBundle,
  env: XmtpEnv,
  v3_host: Option<String>,
  gateway_host: Option<String>,
  app_version: String,
}

#[napi]
impl Backend {
  #[napi(getter)]
  pub fn env(&self) -> XmtpEnv {
    self.env
  }

  #[napi(getter, js_name = "v3Host")]
  pub fn v3_host(&self) -> Option<String> {
    self.v3_host.clone()
  }

  #[napi(getter, js_name = "gatewayHost")]
  pub fn gateway_host(&self) -> Option<String> {
    self.gateway_host.clone()
  }

  #[napi(getter, js_name = "appVersion")]
  pub fn app_version(&self) -> String {
    self.app_version.clone()
  }
}
