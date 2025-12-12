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

use std::error::Error as StdError;
use xmtp_common::{MaybeSend, MaybeSync, RetryableError, retryable};
use xmtp_proto::{
    ConversionError,
    api::{ApiClientError, BodyError},
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
    E: StdError + MaybeSend + MaybeSync,
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
