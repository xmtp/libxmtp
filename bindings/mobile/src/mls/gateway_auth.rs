use xmtp_api_d14n::AuthHandle;
use xmtp_common::BoxDynError;

use crate::{FfiError, GenericError};
use std::sync::Arc;

#[derive(uniffi::Record)]
pub struct FfiCredential {
    name: Option<String>,
    value: String,
    expires_at_seconds: i64,
}

#[derive(uniffi::Object, Clone, Default)]
pub struct FfiAuthHandle {
    handle: AuthHandle,
}

#[uniffi::export(async_runtime = "tokio")]
impl FfiAuthHandle {
    #[uniffi::constructor]
    pub fn new() -> Self {
        Self {
            handle: AuthHandle::new(),
        }
    }
    pub async fn set(&self, credential: FfiCredential) -> Result<(), FfiError> {
        let credential = credential.try_into()?;
        self.handle.set(credential).await;
        Ok(())
    }
    pub fn id(&self) -> u64 {
        self.handle.id() as u64
    }
}

impl From<FfiAuthHandle> for xmtp_api_d14n::AuthHandle {
    fn from(handle: FfiAuthHandle) -> Self {
        handle.handle
    }
}

#[uniffi::export(with_foreign)]
#[xmtp_common::async_trait]
pub trait FfiAuthCallback: Send + Sync + 'static {
    async fn on_auth_required(&self) -> Result<FfiCredential, FfiError>;
}

impl TryFrom<FfiCredential> for xmtp_api_d14n::Credential {
    type Error = GenericError;
    fn try_from(ffi_auth: FfiCredential) -> Result<Self, Self::Error> {
        let credential = xmtp_api_d14n::Credential::new(
            ffi_auth
                .name
                .map(|n| {
                    n.as_str().try_into().map_err(|_| GenericError::Generic {
                        err: format!("Invalid header name for credential: {n}"),
                    })
                })
                .transpose()?,
            (&ffi_auth.value)
                .try_into()
                .map_err(|_| GenericError::Generic {
                    err: "Invalid header value for credential".into(),
                })?,
            ffi_auth.expires_at_seconds,
        );
        Ok(credential)
    }
}

pub(crate) struct FfiAuthCallbackBridge {
    callback: Arc<dyn FfiAuthCallback>,
}

impl FfiAuthCallbackBridge {
    pub fn new(callback: Arc<dyn FfiAuthCallback>) -> Self {
        Self { callback }
    }
}

#[xmtp_common::async_trait]
impl xmtp_api_d14n::AuthCallback for FfiAuthCallbackBridge {
    async fn on_auth_required(&self) -> Result<xmtp_api_d14n::Credential, BoxDynError> {
        let ffi_auth = self.callback.on_auth_required().await?;
        ffi_auth.try_into().map_err(Into::into)
    }
}
