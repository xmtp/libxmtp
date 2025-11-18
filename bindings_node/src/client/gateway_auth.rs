use std::sync::Arc;

use napi::{bindgen_prelude::Promise, threadsafe_function::ThreadsafeFunction};
use napi_derive::napi;
use xmtp_common::BoxDynError;

#[napi(object, constructor)]
pub struct FfiCredential {
  pub name: Option<String>,
  pub value: String,
  pub expires_at_seconds: i64,
}

impl TryFrom<FfiCredential> for xmtp_api_d14n::Credential {
  type Error = super::Error;
  fn try_from(credential: FfiCredential) -> Result<Self, Self::Error> {
    Ok(xmtp_api_d14n::Credential::new(
      credential
        .name
        .map(|n| n.try_into())
        .transpose()
        .map_err(|_| {
          super::Error::new(
            napi::Status::InvalidArg,
            "Invalid header name for credential",
          )
        })?,
      credential.value.try_into().map_err(|_| {
        super::Error::new(
          napi::Status::InvalidArg,
          "Invalid header value for credential",
        )
      })?,
      credential.expires_at_seconds,
    ))
  }
}

#[napi]
#[derive(Default, Clone)]
pub struct FfiAuthHandle {
  handle: xmtp_api_d14n::AuthHandle,
}

#[napi]
impl FfiAuthHandle {
  #[napi(constructor)]
  pub fn new() -> Self {
    Self {
      handle: xmtp_api_d14n::AuthHandle::new(),
    }
  }

  #[napi]
  pub async fn set(&self, credential: FfiCredential) -> Result<(), super::Error> {
    self.handle.set(credential.try_into()?).await;
    Ok(())
  }

  #[napi]
  pub fn id(&self) -> usize {
    self.handle.id()
  }
}

impl From<FfiAuthHandle> for xmtp_api_d14n::AuthHandle {
  fn from(handle: FfiAuthHandle) -> Self {
    handle.handle
  }
}

#[napi]
#[derive(Clone)]
pub struct FfiAuthCallback {
  callback: Arc<ThreadsafeFunction<(), Promise<FfiCredential>>>,
}

#[napi]
impl FfiAuthCallback {
  #[napi(constructor, ts_args_type = "callback: () => Promise<FfiCredential>")]
  pub fn new(callback: ThreadsafeFunction<(), Promise<FfiCredential>>) -> Self {
    Self {
      callback: Arc::new(callback),
    }
  }
}

#[xmtp_common::async_trait]
impl xmtp_api_d14n::AuthCallback for FfiAuthCallback {
  async fn on_auth_required(&self) -> Result<xmtp_api_d14n::Credential, BoxDynError> {
    let promise = self.callback.call_async(Ok(())).await?;
    let credential = promise.await?;
    Ok(credential.try_into()?)
  }
}
