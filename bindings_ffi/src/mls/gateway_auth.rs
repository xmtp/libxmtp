use xmtp_api_d14n::AuthHandle;
use xmtp_common::BoxDynError;

use crate::GenericError;
use std::sync::Arc;

#[derive(uniffi::Record)]
pub struct FfiCredential {
    name: String,
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
    pub async fn set(&self, credential: FfiCredential) -> Result<(), GenericError> {
        let credential = credential.try_into()?;
        self.handle.set(credential).await;
        Ok(())
    }
}

impl From<&FfiAuthHandle> for xmtp_api_d14n::AuthHandle {
    fn from(handle: &FfiAuthHandle) -> Self {
        handle.handle.clone()
    }
}

#[uniffi::export(with_foreign)]
#[async_trait::async_trait]
pub trait FfiAuthCallback: Send + Sync + 'static {
    async fn on_auth_required(&self) -> Result<FfiCredential, GenericError>;
}

impl TryFrom<FfiCredential> for xmtp_api_d14n::Credential {
    type Error = GenericError;
    fn try_from(ffi_auth: FfiCredential) -> Result<Self, Self::Error> {
        let credential = xmtp_api_d14n::Credential::new(
            (&ffi_auth.name)
                .try_into()
                .map_err(|_| GenericError::Generic {
                    err: format!("Invalid header name for credential: {}", ffi_auth.name),
                })?,
            (&ffi_auth.value)
                .try_into()
                .map_err(|_| GenericError::Generic {
                    err: format!("Invalid header value for credential: {}", ffi_auth.value),
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

#[async_trait::async_trait]
impl xmtp_api_d14n::AuthCallback for FfiAuthCallbackBridge {
    async fn on_auth_required(&self) -> Result<xmtp_api_d14n::Credential, BoxDynError> {
        let ffi_auth = self.callback.on_auth_required().await?;
        ffi_auth.try_into().map_err(Into::into)
    }
}
