#![warn(clippy::unwrap_used)]

pub mod attachments;
pub mod configuration;
pub mod identity;
pub mod mls;
mod notification;
pub mod scw_verifier;
#[cfg(any(test, feature = "test-utils"))]
pub mod test_utils;

use std::sync::Arc;

use xmtp_common::{ErrorCode, ExponentialBackoff, Retry, RetryableError, retryable};
pub use xmtp_proto::api_client::XmtpApi;

pub use identity::*;
pub use mls::*;
pub mod chunk;
pub use chunk::PublishUnit;

pub type Result<T> = std::result::Result<T, ApiError>;

pub mod strategies {
    use super::*;
    pub fn exponential_cooldown() -> Retry<ExponentialBackoff> {
        xmtp_common::Retry::builder().build()
    }
}

/// Preserve auth codes before transport errors lose their concrete type.
///
/// The error may arrive boxed, and `Box<E>` implements `RetryableError`, so a
/// downcast of the outer value alone would miss the code. Check the source
/// chain too: `ApiClientError::Auth` is `#[error(transparent)]` over the
/// `AuthError`.
// implements: AUTH-026
pub fn dyn_err(e: impl RetryableError + 'static) -> ApiError {
    fn find(error: &(dyn std::any::Any + 'static)) -> Option<xmtp_proto::api::AuthError> {
        if let Some(xmtp_proto::api::ApiClientError::Auth(auth)) = error.downcast_ref() {
            return Some(*auth);
        }
        if let Some(auth) = error.downcast_ref::<xmtp_proto::api::AuthError>() {
            return Some(*auth);
        }
        // A boxed error downcasts as the box, never as its contents. Unwrap the
        // shapes that reach this function; `source()` cannot help, because
        // `#[error(transparent)]` forwards Display without forwarding `source`.
        if let Some(boxed) = error.downcast_ref::<Box<xmtp_proto::api::ApiClientError>>() {
            return find(&**boxed);
        }
        if let Some(boxed) = error.downcast_ref::<Box<xmtp_proto::api::AuthError>>() {
            return Some(**boxed);
        }
        None
    }
    if let Some(auth) = find(&e) {
        return ApiError::Auth(auth);
    }
    ApiError::Api(xmtp_proto::api::NetworkError::new(e))
}

#[derive(Debug, thiserror::Error, ErrorCode)]
pub enum ApiError {
    #[error(transparent)]
    #[error_code(inherit)]
    Auth(#[from] xmtp_proto::api::AuthError),
    /// API client error.
    ///
    /// API operation error (network, deserialization, or other). May be retryable.
    #[error("api client error {0}")]
    Api(#[source] xmtp_proto::api::NetworkError),
    /// The backend rejected a stale identity update. Not retryable here.
    #[error("identity history changed")]
    IdentityUpdateConflict,
    /// One envelope exceeds the configured byte limit. Not retryable.
    #[error("envelope exceeds the byte limit")]
    EnvelopeTooLarge,
    /// One atomic publish unit exceeds a request limit. Not retryable.
    #[error("atomic publish unit exceeds a request limit")]
    UnitTooLarge,
    /// A single-topic response still exceeds a backend limit. Not retryable.
    #[error("one envelope response exceeds a backend limit")]
    ResponseTooLarge,
    /// The request has invalid input. Not retryable.
    #[error("invalid backend request: {0}")]
    InvalidRequest(&'static str),
    /// A response does not match the request. Not retryable.
    #[error("invalid backend response: {0}")]
    InvalidResponse(&'static str),
    /// The payload cannot be parsed. Not retryable.
    #[error(transparent)]
    InvalidEnvelope(#[from] xmtp_mls_validation::ValidationError),
    /// A returned backend envelope cannot be decoded. Not retryable.
    #[error(transparent)]
    Envelope(#[from] xmtp_api_backend::envelope::EnvelopeError),
    /// Proto conversion error.
    ///
    /// Protobuf conversion failed. Not retryable.
    #[error(transparent)]
    ProtoConversion(#[from] xmtp_proto::ConversionError),
}

impl RetryableError for ApiError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::Auth(e) => retryable!(e),
            Self::Api(e) => retryable!(e),
            _ => false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ApiClientWrapper<ApiClient> {
    // todo: this should be private to impl
    pub api_client: ApiClient,
    pub(crate) retry_strategy: Arc<Retry<ExponentialBackoff>>,
    pub(crate) inbox_id: Option<String>,
    /// What the deployment published about the shapes it accepts.
    /// The compiled defaults until a client resolves a snapshot and installs
    /// it, which is what keeps a bare wrapper — a test double, the static
    /// configuration fetch — chunking exactly as it did before this existed.
    pub(crate) configuration: Arc<xmtp_configuration::ServerConfiguration>,
}

impl<ApiClient> ApiClientWrapper<ApiClient> {
    pub fn new(api_client: ApiClient, retry_strategy: Retry<ExponentialBackoff>) -> Self {
        Self {
            api_client,
            retry_strategy: retry_strategy.into(),
            inbox_id: None,
            configuration: Arc::default(),
        }
    }

    pub fn map<F, NewApiClient>(self, f: F) -> ApiClientWrapper<NewApiClient>
    where
        F: FnOnce(ApiClient) -> NewApiClient,
    {
        ApiClientWrapper {
            api_client: f(self.api_client),
            retry_strategy: self.retry_strategy,
            inbox_id: self.inbox_id,
            configuration: self.configuration,
        }
    }

    /// Chunk and pre-validate every later request against this snapshot.
    /// Called once, by `build`, before the client runs.
    pub fn set_configuration(
        &mut self,
        configuration: Arc<xmtp_configuration::ServerConfiguration>,
    ) {
        self.configuration = configuration;
    }

    /// What this wrapper chunks and pre-validates against.
    pub fn configuration(&self) -> &xmtp_configuration::ServerConfiguration {
        &self.configuration
    }

    /// The request shapes the deployment accepts.
    pub fn limits(&self) -> &xmtp_configuration::LimitsConfiguration {
        &self.configuration.limits
    }

    /// Attach an InboxId to this API Client Wrapper.
    /// Attaches an inbox_id context to tracing logs, useful for debugging
    pub fn attach_inbox_id(&mut self, inbox_id: Option<String>) {
        self.inbox_id = inbox_id;
    }
}

xmtp_common::if_native! {
    #[cfg(test)]
    #[ctor::ctor(unsafe)]
    fn _setup() {
        xmtp_common::logger()
    }
}

#[cfg(test)]
mod tests;
