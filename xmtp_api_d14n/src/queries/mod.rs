mod combined;
pub use combined::*;

mod d14n;
pub use d14n::*;

mod v3;
pub use v3::*;

use std::error::Error as StdError;
use xmtp_common::{RetryableError, retryable};
use xmtp_proto::{
    ConversionError,
    traits::{ApiClientError, BodyError},
};

#[derive(thiserror::Error, Debug)]
pub enum QueryError<E: StdError> {
    #[error(transparent)]
    ApiClient(#[from] ApiClientError<E>),
    #[error(transparent)]
    Envelope(#[from] crate::protocol::EnvelopeError),
    #[error(transparent)]
    Conversion(#[from] ConversionError),
    #[error(transparent)]
    Body(#[from] BodyError),
}

impl<E> From<crate::protocol::EnvelopeError> for ApiClientError<E>
where
    E: StdError + Send + Sync,
{
    fn from(e: crate::protocol::EnvelopeError) -> ApiClientError<E> {
        ApiClientError::Other(Box::new(e))
    }
}

impl<E> RetryableError for QueryError<E>
where
    E: StdError + RetryableError + 'static,
{
    fn is_retryable(&self) -> bool {
        match self {
            Self::ApiClient(c) => retryable!(c),
            Self::Envelope(_e) => false,
            Self::Conversion(_c) => false,
            Self::Body(b) => retryable!(b),
        }
    }
}
