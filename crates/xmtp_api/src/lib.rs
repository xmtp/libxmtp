#![warn(clippy::unwrap_used)]

pub mod identity;
pub mod mls;
pub mod scw_verifier;
#[cfg(any(test, feature = "test-utils"))]
pub mod test_utils;

pub mod debug_wrapper;
pub use debug_wrapper::*;

use std::sync::Arc;

use xmtp_common::{ExponentialBackoff, Retry, RetryableError, retryable};
pub use xmtp_proto::api_client::XmtpApi;

pub use identity::*;
pub use mls::*;
mod xmtp_query;

pub type Result<T> = std::result::Result<T, ApiError>;

pub mod strategies {
    use super::*;
    pub fn exponential_cooldown() -> Retry<ExponentialBackoff> {
        xmtp_common::Retry::builder().build()
    }
}

// Erases Api Error type (which may be Http or Grpc)
pub fn dyn_err(e: impl RetryableError + 'static) -> ApiError {
    ApiError::Api(Box::new(e))
}

#[derive(Debug, thiserror::Error, xmtp_common::ErrorCode)]
pub enum ApiError {
    #[error("api client error {0}")]
    Api(Box<dyn RetryableError>),
    #[error(
        "mismatched number of results, key packages {} != installation_keys {}",
        .key_packages,
        .installation_keys
    )]
    MismatchedKeyPackages {
        key_packages: usize,
        installation_keys: usize,
    },
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
    #[ctor::ctor]
    fn _setup() {
        xmtp_common::logger()
    }
}
