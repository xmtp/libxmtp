use std::sync::Arc;
use xmtp_common::{BoxDynError, MaybeSend, MaybeSync};

use crate::{XmtpError, foreign};

#[derive(Clone, Debug, uniffi::Record)]
pub struct Credential {
    pub name: Option<String>,
    pub value: String,
    pub expires_at_seconds: i64,
}

#[xmtp_macro::callback_error]
#[derive(Clone, Debug, thiserror::Error, uniffi::Error)]
pub enum CredentialError {
    #[error("credential callback failed")]
    Failed,
}

impl From<uniffi::UnexpectedUniFFICallbackError> for CredentialError {
    fn from(_: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::Failed
    }
}

// Foreign traits need `with_foreign`, which `sdk_export` cannot emit.
#[uniffi::export(with_foreign)]
#[xmtp_common::async_trait]
pub trait CredentialSource: MaybeSend + MaybeSync + 'static {
    async fn credential(&self) -> Result<Credential, CredentialError>;
}

#[derive(Clone, Default, uniffi::Record)]
pub struct BackendOptions {
    #[uniffi(default = "")]
    pub url: String,
    #[uniffi(default = None)]
    pub app_version: Option<String>,
    #[uniffi(default = None)]
    pub credentials: Option<Arc<dyn CredentialSource>>,
}

pub(crate) struct AuthBridge {
    source: Arc<dyn CredentialSource>,
}

impl AuthBridge {
    pub(crate) fn new(source: Arc<dyn CredentialSource>) -> Self {
        Self { source }
    }
}

#[xmtp_common::async_trait]
impl xmtp_api_backend::AuthCallback for AuthBridge {
    async fn on_auth_required(&self) -> Result<xmtp_api_backend::Credential, BoxDynError> {
        let source = self.source.clone();
        let result = foreign::call(async move { source.credential().await })
            .await
            .map_err(|_| "auth callback failed")?
            .map_err(|_| "auth callback failed")?;
        let name = result
            .name
            .map(|name| name.parse::<http::header::HeaderName>())
            .transpose()
            .map_err(|_| "auth callback failed")?;
        let value = result
            .value
            .parse::<http::header::HeaderValue>()
            .map_err(|_| "auth callback failed")?;
        Ok(xmtp_api_backend::Credential::new(
            name,
            value,
            result.expires_at_seconds,
        ))
    }
}

#[derive(uniffi::Object)]
pub struct Backend {
    pub(crate) api: xmtp_mls::XmtpApiClient,
}

impl Backend {
    pub(crate) fn from_options(options: BackendOptions) -> Result<Self, XmtpError> {
        #[cfg(not(target_arch = "wasm32"))]
        xmtp_cryptography::install_crypto_provider();
        let mut builder = xmtp_api_backend::MessageBackendBuilder::new();
        builder.host(&options.url);
        if let Some(version) = options.app_version {
            builder.app_version(version);
        }
        builder.maybe_auth_callback(options.credentials.map(|source| {
            Arc::new(AuthBridge::new(source)) as Arc<dyn xmtp_api_backend::AuthCallback>
        }));
        Ok(Self {
            api: builder.build().map_err(XmtpError::unknown)?,
        })
    }
}

#[xmtp_macro::sdk_export]
impl Backend {
    #[uniffi::constructor]
    pub async fn connect(options: BackendOptions) -> Result<Self, XmtpError> {
        Self::from_options(options)
    }
}
