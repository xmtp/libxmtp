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
use xmtp_proto::api_client::{BoxedApiClient, Error as ApiError};

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
pub struct ApiClientWrapper {
    pub(crate) api_client: BoxedApiClient,
    pub(crate) retry_strategy: Retry,
}

impl ApiClientWrapper {
    pub fn new(api_client: BoxedApiClient, retry_strategy: Retry) -> Self {
        Self {
            api_client,
            retry_strategy,
        }
    }
}
