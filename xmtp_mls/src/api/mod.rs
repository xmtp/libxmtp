pub mod identity;
pub mod mls;
#[cfg(test)]
pub mod test_utils;

use crate::{
    retry::{Retry, RetryableError},
    XmtpApi,
};
use thiserror::Error;
use xmtp_id::associations::DeserializationError as AssociationDeserializationError;
use xmtp_proto::api_client::Error as ApiError;

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

#[derive(Debug)]
pub struct ApiClientWrapper<ApiClient> {
    api_client: ApiClient,
    retry_strategy: Retry,
}

impl<ApiClient> ApiClientWrapper<ApiClient>
where
    ApiClient: XmtpApi,
{
    pub fn new(api_client: ApiClient, retry_strategy: Retry) -> Self {
        Self {
            api_client,
            retry_strategy,
        }
    }
}
