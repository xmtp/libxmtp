#![warn(clippy::unwrap_used)]

pub mod identity;
pub mod mls;
#[cfg(any(test, feature = "test-utils"))]
pub mod test_utils;

use std::sync::Arc;

use xmtp_common::{ExponentialBackoff, Retry, RetryableError};
pub use xmtp_proto::api_client::trait_impls::XmtpApi;
use xmtp_proto::ApiError;

pub use identity::*;
pub use mls::*;

pub type Result<T> = std::result::Result<T, Error>;

pub mod strategies {
    use super::*;
    pub fn exponential_cooldown() -> Retry<ExponentialBackoff> {
        xmtp_common::Retry::builder().build()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("API client error: {0}")]
    Api(#[from] ApiError),
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

impl RetryableError for Error {
    fn is_retryable(&self) -> bool {
        matches!(self, Self::Api(_))
    }
}

#[derive(Clone, Debug)]
pub struct ApiClientWrapper<ApiClient> {
    // todo: this should be private to impl
    pub api_client: Arc<ApiClient>,
    pub(crate) retry_strategy: Arc<Retry<ExponentialBackoff>>,
    pub(crate) inbox_id: Option<String>,
}

impl<ApiClient> ApiClientWrapper<ApiClient> {
    pub fn new(api_client: Arc<ApiClient>, retry_strategy: Retry<ExponentialBackoff>) -> Self {
        Self {
            api_client,
            retry_strategy: retry_strategy.into(),
            inbox_id: None,
        }
    }

    /// Attach an InboxId to this API Client Wrapper.
    /// Attaches an inbox_id context to tracing logs, useful for debugging
    pub fn attach_inbox_id(&mut self, inbox_id: Option<String>) {
        self.inbox_id = inbox_id;
    }
}

#[cfg(test)]
pub(crate) mod tests {

    #[cfg(all(
        not(any(target_arch = "wasm32", feature = "http-api")),
        feature = "grpc-api"
    ))]
    pub type TestClient = xmtp_api_grpc::grpc_api_helper::Client;

    #[cfg(any(feature = "http-api", target_arch = "wasm32",))]
    pub type TestClient = xmtp_api_http::XmtpHttpApiClient;

    // Execute once before any tests are run
    #[cfg_attr(not(target_arch = "wasm32"), ctor::ctor)]
    #[cfg(not(target_arch = "wasm32"))]
    fn _setup() {
        xmtp_common::logger();
    }
}
