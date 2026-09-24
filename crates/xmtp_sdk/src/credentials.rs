use std::sync::Arc;
use xmtp_common::{BoxDynError, MaybeSend, MaybeSync};

use crate::{XmtpError, foreign};

#[derive(Clone, Debug, uniffi::Record)]
pub struct Credential {
    pub name: Option<String>,
    pub value: String,
    pub expires_at_seconds: i64,
}

impl Credential {
    pub(crate) fn to_backend(&self) -> Result<xmtp_api_backend::Credential, XmtpError> {
        let name = self
            .name
            .as_deref()
            .map(str::parse::<http::header::HeaderName>)
            .transpose()
            .map_err(|_| XmtpError::invalid("credential header name is invalid"))?;
        let value = self
            .value
            .parse::<http::header::HeaderValue>()
            .map_err(|_| XmtpError::invalid("credential header value is invalid"))?;
        Ok(xmtp_api_backend::Credential::new(
            name,
            value,
            self.expires_at_seconds,
        ))
    }
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
    #[uniffi(default = None)]
    pub credential: Option<Credential>,
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
    pub(crate) auth_handle: Option<xmtp_api_backend::AuthHandle>,
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
        let has_credentials = options.credentials.is_some() || options.credential.is_some();
        let auth_handle = has_credentials.then(xmtp_api_backend::AuthHandle::new);
        builder.maybe_auth_handle(auth_handle.clone());
        builder.maybe_auth_callback(options.credentials.map(|source| {
            Arc::new(AuthBridge::new(source)) as Arc<dyn xmtp_api_backend::AuthCallback>
        }));
        Ok(Self {
            api: builder.build().map_err(XmtpError::unknown)?,
            auth_handle,
        })
    }
}

#[xmtp_macro::sdk_export]
impl Backend {
    #[uniffi::constructor]
    pub async fn connect(options: BackendOptions) -> Result<Self, XmtpError> {
        let initial = options.credential.clone();
        let backend = Self::from_options(options)?;
        if let (Some(handle), Some(credential)) = (&backend.auth_handle, initial) {
            handle.set(credential.to_backend()?).await;
        }
        Ok(backend)
    }
}
