mod api_stats;
mod boxed_streams;
mod builder;
mod client_bundle;
mod combinators;
mod combined;
mod d14n;
pub mod stream;
mod v3;

pub use api_stats::*;
pub use boxed_streams::*;
pub use builder::*;
pub use client_bundle::*;
pub use combinators::*;
pub use combined::*;
pub use d14n::*;
pub use stream::*;
pub use v3::*;

use xmtp_common::{RetryableError, retryable};
use xmtp_proto::{
    ConversionError,
    api::{ApiClientError, BodyError},
};

#[derive(thiserror::Error, Debug)]
pub enum QueryError {
    #[error(transparent)]
    ApiClient(#[from] ApiClientError),
    #[error(transparent)]
    Envelope(#[from] crate::protocol::EnvelopeError),
    #[error(transparent)]
    Conversion(#[from] ConversionError),
    #[error(transparent)]
    Body(#[from] BodyError),
}

impl From<crate::protocol::EnvelopeError> for ApiClientError {
    fn from(e: crate::protocol::EnvelopeError) -> ApiClientError {
        ApiClientError::Other(Box::new(e))
    }
}

impl RetryableError for QueryError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::ApiClient(c) => retryable!(c),
            Self::Envelope(_e) => false,
            Self::Conversion(_c) => false,
            Self::Body(b) => retryable!(b),
        }
    }
}
