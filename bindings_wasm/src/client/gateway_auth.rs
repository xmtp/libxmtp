use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use xmtp_common::BoxDynError;

#[wasm_bindgen]
#[derive(Clone, Serialize, Deserialize)]
pub struct Credential {
  name: Option<String>,
  value: String,
  expires_at_seconds: i64,
}

#[wasm_bindgen]
impl Credential {
  #[wasm_bindgen(constructor)]
  pub fn new(name: Option<String>, value: String, expires_at_seconds: i64) -> Self {
    Self {
      name,
      value,
      expires_at_seconds,
    }
  }
}

impl TryFrom<Credential> for xmtp_api_d14n::Credential {
  type Error = BoxDynError;
  fn try_from(credential: Credential) -> Result<Self, Self::Error> {
    Ok(xmtp_api_d14n::Credential::new(
      credential.name.map(|n| n.try_into()).transpose()?,
      credential.value.try_into()?,
      credential.expires_at_seconds,
    ))
  }
}

#[wasm_bindgen]
extern "C" {
  pub type AuthCallback;

  #[wasm_bindgen(catch, method)]
  pub async fn on_auth_required(this: &AuthCallback) -> Result<JsValue, JsValue>;
}

#[async_trait::async_trait(?Send)]
impl xmtp_api_d14n::AuthCallback for AuthCallback {
  async fn on_auth_required(&self) -> Result<xmtp_api_d14n::Credential, BoxDynError> {
    let cred: JsValue = self.on_auth_required().await.map_err(|e| {
      let result = serde_wasm_bindgen::from_value::<serde_json::Value>(e);
      if let Ok(value) = result {
        let is_empty = value.is_null()
          || (value.is_object() && value.as_object().unwrap().is_empty())
          || (value.is_array() && value.as_array().unwrap().is_empty());
        if !is_empty {
          return format!("Auth callback failed: {value}");
        }
      }
      "Auth callback failed with unknown error".to_string()
    })?;
    let cred: Credential = serde_wasm_bindgen::from_value(cred)
      .map_err(|e| format!("Failed to parse credential from auth callback: {e}"))?;
    Ok(cred.try_into()?)
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
  pub async fn set(&self, credential: Credential) -> Result<(), JsError> {
    let cred =
      xmtp_api_d14n::Credential::try_from(credential).map_err(|e| JsError::new(&e.to_string()))?;
    self.handle.set(cred).await;
    Ok(())
  }
  pub fn id(&self) -> usize {
    self.handle.id()
  }
}
