use std::sync::Arc;
use xmtp_common::{BoxDynError, MaybeSend, MaybeSync};

use crate::{XmtpError, foreign};

#[derive(Clone, uniffi::Record)]
pub struct Credential {
    pub name: Option<String>,
    pub value: String,
    pub expires_at_seconds: i64,
}

impl std::fmt::Debug for Credential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credential")
            .field("name", &self.name)
            .field("value", &"[redacted]")
            .field("expires_at_seconds", &self.expires_at_seconds)
            .finish()
    }
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

/// Use connection options or an existing backend connection.
#[derive(Clone, uniffi::Enum)]
pub enum BackendSource {
    Options { options: BackendOptions },
    Connected { backend: Arc<Backend> },
}

impl Default for BackendSource {
    fn default() -> Self {
        Self::Options {
            options: BackendOptions::default(),
        }
    }
}

impl BackendSource {
    pub(crate) async fn resolve(&self) -> Result<Arc<Backend>, XmtpError> {
        match self {
            Self::Options { options } => Ok(Arc::new(Backend::connect(options.clone()).await?)),
            Self::Connected { backend } => Ok(backend.clone()),
        }
    }

    pub(crate) fn app_version(&self) -> Option<String> {
        match self {
            Self::Options { options } => options.app_version.clone(),
            Self::Connected { backend } => backend.options.app_version.clone(),
        }
    }
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
    pub(crate) options: BackendOptions,
}

impl Backend {
    pub(crate) fn from_options(options: BackendOptions) -> Result<Self, XmtpError> {
        #[cfg(not(target_arch = "wasm32"))]
        xmtp_cryptography::install_crypto_provider();
        let original_options = options.clone();
        let mut builder = xmtp_api_backend::MessageBackendBuilder::new();
        builder.host(&options.url);
        if let Some(version) = options.app_version {
            builder.app_version(version);
        }
        // Keep a handle for a credential that the app sets after creation.
        // An empty SDK handle does not satisfy a required-credential deployment.
        let auth_handle = Some(
            if options.credential.is_some() || options.credentials.is_some() {
                xmtp_api_backend::AuthHandle::new()
            } else {
                xmtp_api_backend::AuthHandle::sdk_placeholder()
            },
        );
        builder.maybe_auth_handle(auth_handle.clone());
        builder.maybe_auth_callback(options.credentials.map(|source| {
            Arc::new(AuthBridge::new(source)) as Arc<dyn xmtp_api_backend::AuthCallback>
        }));
        Ok(Self {
            api: builder.build().map_err(XmtpError::from_core)?,
            auth_handle,
            options: original_options,
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

#[cfg(test)]
mod credential_debug_tests {
    use super::Credential;

    #[xmtp_common::test(unwrap_try = true)]
    async fn credential_debug_redacts_direct_and_nested_value() {
        let secret = "credential-bearer-sentinel-5e9741";
        let credential = Credential {
            name: Some("authorization".to_string()),
            value: secret.to_string(),
            expires_at_seconds: 123,
        };
        assert_eq!(credential.value, secret);
        assert_eq!(credential.name.as_deref(), Some("authorization"));
        assert_eq!(credential.expires_at_seconds, 123);
        assert!(!format!("{credential:?}").contains(secret));
        assert!(!format!("{:?}", Some(vec![credential])).contains(secret));
    }
}
