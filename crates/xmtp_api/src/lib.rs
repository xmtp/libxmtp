#![warn(clippy::unwrap_used)]

pub mod identity;
pub mod mls;
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

// Erases Api Error type (which may be Http or Grpc)
pub fn dyn_err(e: impl RetryableError + 'static) -> ApiError {
    ApiError::Api(xmtp_proto::api::NetworkError::new(e))
}

#[derive(Debug, thiserror::Error, ErrorCode)]
pub enum ApiError {
    /// API client error.
    ///
    /// API operation error (network, deserialization, or other). May be retryable.
    #[error("api client error {0}")]
    Api(#[source] xmtp_proto::api::NetworkError),
    /// The backend rejected a stale identity update. Not retryable here.
    #[error("identity history changed")]
    IdentityUpdateConflict,
    /// The backend hash differs from the retained envelope hash. Not retryable.
    #[error("publish response hash does not match the envelope")]
    HashMismatch,
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
}

impl<ApiClient> ApiClientWrapper<ApiClient> {
    pub fn new(api_client: ApiClient, retry_strategy: Retry<ExponentialBackoff>) -> Self {
        Self {
            api_client,
            retry_strategy: retry_strategy.into(),
            inbox_id: None,
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
        }
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
