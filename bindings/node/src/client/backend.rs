use crate::ErrorWrapper;
use crate::client::gateway_auth::{AuthCallback, AuthHandle};
use crate::client::options::XmtpEnv;
use napi::bindgen_prelude::Result;
use napi_derive::napi;
use std::sync::{Arc, Mutex};
use xmtp_api_d14n::ClientBundleBuilder;

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
    let mut builder = ClientBundleBuilder::default();
    builder
      .env(self.env.into())
      .maybe_v3_host(self.api_url.clone())
      .maybe_gateway_host(self.gateway_host.clone())
      .readonly(self.readonly.unwrap_or(false))
      .app_version(app_version.clone())
      .maybe_auth_callback(
        auth_callback.map(|c| Arc::new(c) as Arc<dyn xmtp_api_d14n::AuthCallback>),
      )
      .maybe_auth_handle(auth_handle.map(|h: AuthHandle| h.into()));

    let v3_host = builder.get_v3_host().map(ToString::to_string);
    let bundle = builder.build_optional_d14n().map_err(ErrorWrapper::from)?;
    Ok(Backend {
      bundle,
      env: self.env,
      v3_host,
      gateway_host: self.gateway_host.clone(),
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
