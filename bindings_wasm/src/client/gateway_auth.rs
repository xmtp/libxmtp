use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use xmtp_common::BoxDynError;

#[wasm_bindgen]
#[derive(Clone, Serialize, Deserialize)]
pub struct Credential {
  name: String,
  value: String,
  expires_at_seconds: i64,
}

#[wasm_bindgen]
impl Credential {
  #[wasm_bindgen(constructor)]
  pub fn new(name: String, value: String, expires_at_seconds: i64) -> Self {
    Self {
      name,
      value,
      expires_at_seconds,
    }
  }
}

#[wasm_bindgen]
extern "C" {
  pub type AuthCallback;

  #[wasm_bindgen(method)]
  pub async fn on_auth_required(this: &AuthCallback) -> JsValue;
}

#[async_trait::async_trait(?Send)]
impl xmtp_api_d14n::AuthCallback for AuthCallback {
  async fn on_auth_required(&self) -> Result<xmtp_api_d14n::Credential, BoxDynError> {
    let cred = self.on_auth_required().await;
    let cred: Credential = serde_wasm_bindgen::from_value(cred).map_err(|e| e.to_string())?;
    Ok(xmtp_api_d14n::Credential::new(
      cred.name.try_into()?,
      cred.value.try_into()?,
      cred.expires_at_seconds,
    ))
  }
}

#[wasm_bindgen]
#[derive(Clone, Default)]
pub struct AuthHandle {
  pub(crate) handle: xmtp_api_d14n::AuthHandle,
}

#[wasm_bindgen]
impl AuthHandle {
  #[wasm_bindgen(constructor)]
  pub fn new() -> Self {
    Self {
      handle: xmtp_api_d14n::AuthHandle::new(),
    }
  }
}
