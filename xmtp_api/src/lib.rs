#![warn(clippy::unwrap_used)]

pub mod identity;
pub mod mls;
#[cfg(any(test, feature = "test-utils"))]
pub mod test_utils;

use std::sync::Arc;

use xmtp_common::{Retry, RetryableError};
use xmtp_id::{associations::DeserializationError as AssociationDeserializationError, InboxId};
pub use xmtp_proto::api_client::trait_impls::XmtpApi;
use xmtp_proto::ApiError;

pub use identity::*;
pub use mls::*;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("API client error: {0}")]
    Api(#[from] ApiError),
    #[error("Deserialization error {0}")]
    AssociationDeserialization(#[from] AssociationDeserializationError),
    #[error(
        "mismatched number of results, key packages {} != installation_keys {}",
        .key_packages,
        .installation_keys
    )]
    MismatchedKeyPackages {
        key_packages: usize,
        installation_keys: usize,
    },
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
    pub(crate) retry_strategy: Retry,
    pub(crate) inbox_id: Option<InboxId>,
}

impl<ApiClient> ApiClientWrapper<ApiClient>
where
    ApiClient: XmtpApi,
{
    pub fn new(api_client: Arc<ApiClient>, retry_strategy: Retry) -> Self {
        Self {
            api_client,
            retry_strategy,
            inbox_id: None,
        }
    }

    /// Attach an InboxId to this API Client Wrapper.
    /// Attaches an inbox_id context to tracing logs, useful for debugging
    pub fn attach_inbox_id(&mut self, inbox_id: Option<InboxId>) {
        self.inbox_id = inbox_id;
    }
}
