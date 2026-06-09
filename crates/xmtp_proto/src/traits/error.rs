use std::fmt::Display;

use crate::{ApiEndpoint, ProtoError};
use thiserror::Error;
use xmtp_common::{BoxDynError, Retryable, RetryableError};

#[derive(Debug, Error, Retryable)]
#[non_exhaustive]
pub enum ApiClientError {
    /// The client encountered an error.
    #[error("api client at endpoint \"{}\" has error {}", endpoint, source)]
    #[retry(when = source.is_retryable())]
    ClientWithEndpoint {
        endpoint: String,
        /// The client error.
        source: NetworkError,
    },
    #[error("client errored {}", source)]
    #[retry(inherit)]
    Client { source: NetworkError },
    #[error(transparent)]
    #[retry(true)]
    Http(#[from] http::Error),
    #[error(transparent)]
    #[retry(inherit)]
    Body(#[from] BodyError),
    #[error(transparent)]
    DecodeError(#[from] prost::DecodeError),
    #[error(transparent)]
    Conversion(#[from] crate::ConversionError),
    #[error(transparent)]
    ProtoError(#[from] ProtoError),
    #[error(transparent)]
    InvalidUri(#[from] http::uri::InvalidUri),
    #[error(transparent)]
    #[retry(true)]
    Expired(#[from] xmtp_common::time::Expired),
    #[error("{0}")]
    #[retry(inherit)]
    Other(Box<dyn RetryableError>),
    #[error("{0}")]
    OtherUnretryable(BoxDynError),
    #[error("Writes are disabled on this client.")]
    WritesDisabled,
}

/// A lower level NetworkError, like gRPC/QUIC/HTTP/1.1 errors go here.
/// use [`ApiClientError::new`] to construct
// needed because of AsDynError sealed trait
#[derive(Debug, Retryable)]
#[retry(when = self.source.is_retryable())]
pub struct NetworkError {
    source: Box<dyn RetryableError>,
}

impl std::error::Error for NetworkError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.source.as_ref())
    }
}

impl Display for NetworkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.source)
    }
}

impl NetworkError {
    pub fn new(e: impl RetryableError + 'static) -> Self {
        NetworkError {
            source: Box::new(e),
        }
    }
}

impl ApiClientError {
    pub fn new(endpoint: ApiEndpoint, source: impl RetryableError + 'static) -> Self {
        Self::ClientWithEndpoint {
            endpoint: endpoint.to_string(),
            source: NetworkError::new(source),
        }
    }

    /// add an endpoint to a ApiError::Client error
    pub fn endpoint(self, endpoint: impl ToString) -> Self {
        match self {
            Self::Client { source } => Self::ClientWithEndpoint {
                source,
                endpoint: endpoint.to_string(),
            },
            v => v,
        }
    }

    pub fn client(client: impl RetryableError + 'static) -> Self {
        Self::Client {
            source: NetworkError::new(client),
        }
    }

    /// Try to pull a [`NetworkError`] out of this error enum.
    /// returns None if there's no match
    pub fn network_error(&self) -> Option<&NetworkError> {
        use ApiClientError::*;
        match self {
            ClientWithEndpoint { source, .. } | Client { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl ApiClientError {
    pub fn other<R: RetryableError + 'static>(e: R) -> Self {
        ApiClientError::Other(Box::new(e))
    }
}

// Infallible errors by definition can never occur
impl From<std::convert::Infallible> for ApiClientError {
    fn from(_v: std::convert::Infallible) -> ApiClientError {
        unreachable!("Infallible errors can never occur")
    }
}

#[derive(Debug, Error, Retryable)]
pub enum BodyError {
    #[error(transparent)]
    UninitializedField(#[from] derive_builder::UninitializedFieldError),
    #[error(transparent)]
    Conversion(#[from] crate::ConversionError),
}
