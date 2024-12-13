pub mod identity;
pub mod mls;
#[cfg(any(test, feature = "test-utils"))]
pub mod test_utils;

use std::sync::Arc;

use crate::XmtpApi;
use thiserror::Error;
use xmtp_common::{Retry, RetryableError};
use xmtp_id::{associations::DeserializationError as AssociationDeserializationError, InboxId};
use xmtp_proto::Error as ApiError;

pub use identity::*;
pub use mls::*;

#[derive(Debug, Error)]
pub enum WrappedApiError {
    #[error("API client error: {0}")]
    Api(#[from] ApiError),
    #[error("Deserialization error {0}")]
    AssociationDeserialization(#[from] AssociationDeserializationError),
}

impl RetryableError for WrappedApiError {
    fn is_retryable(&self) -> bool {
        matches!(self, Self::Api(_))
    }
}

#[derive(Clone, Debug)]
pub struct ApiClientWrapper<ApiClient> {
    pub(crate) api_client: Arc<ApiClient>,
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
    pub(crate) fn attach_inbox_id(&mut self, inbox_id: Option<InboxId>) {
        self.inbox_id = inbox_id;
    }
}
