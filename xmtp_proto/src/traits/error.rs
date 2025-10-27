use crate::api_client::AggregateStats;
use crate::{ApiEndpoint, ProtoError};
use thiserror::Error;
use xmtp_common::{RetryableError, retryable};

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ApiClientError<E: std::error::Error> {
    #[error(
        "api client at endpoint \"{}\" has error {}. \n {:?} \n",
        endpoint,
        source,
        stats
    )]
    ClientWithEndpointAndStats {
        endpoint: String,
        source: E,
        stats: AggregateStats,
    },
    #[error("API Error {}, \n {:?} \n", e, stats)]
    ErrorWithStats {
        e: Box<dyn RetryableError + Send + Sync>,
        stats: AggregateStats,
    },
    /// The client encountered an error.
    #[error("api client at endpoint \"{}\" has error {}", endpoint, source)]
    ClientWithEndpoint {
        endpoint: String,
        /// The client error.
        source: E,
    },
    #[error("client errored {}", source)]
    Client { source: E },
    #[error(transparent)]
    Http(#[from] http::Error),
    #[error(transparent)]
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
    Expired(#[from] xmtp_common::time::Expired),
    #[error("{0}")]
    Other(Box<dyn RetryableError + Send + Sync>),
    #[error("{0}")]
    OtherUnretryable(Box<dyn std::error::Error + Send + Sync>),
}

impl<E> ApiClientError<E>
where
    E: std::error::Error + 'static,
{
    pub fn new(endpoint: ApiEndpoint, source: E) -> Self {
        Self::ClientWithEndpoint {
            endpoint: endpoint.to_string(),
            source,
        }
    }

    /// add an endpoint to a ApiError::Client error
    pub fn endpoint(self, endpoint: String) -> Self {
        match self {
            Self::Client { source } => Self::ClientWithEndpoint { source, endpoint },
            v => v,
        }
    }
}

impl<E> RetryableError for ApiClientError<E>
where
    E: RetryableError + std::error::Error + 'static,
{
    fn is_retryable(&self) -> bool {
        use ApiClientError::*;
        match self {
            ClientWithEndpointAndStats { source, .. } => retryable!(source),
            ErrorWithStats { e, .. } => retryable!(e),
            Client { source } => retryable!(*source),
            ClientWithEndpoint { source, .. } => retryable!(source),
            Body(e) => retryable!(e),
            Http(_) => true,
            DecodeError(_) => false,
            Conversion(_) => false,
            ProtoError(_) => false,
            InvalidUri(_) => false,
            Expired(_) => true,
            Other(r) => retryable!(r),
            OtherUnretryable(_) => false,
        }
    }
}

// Infallible errors by definition can never occur
impl<E: std::error::Error> From<std::convert::Infallible> for ApiClientError<E> {
    fn from(_v: std::convert::Infallible) -> ApiClientError<E> {
        unreachable!("Infallible errors can never occur")
    }
}

#[derive(Debug, Error)]
pub enum BodyError {
    #[error(transparent)]
    UninitializedField(#[from] derive_builder::UninitializedFieldError),
    #[error(transparent)]
    Conversion(#[from] crate::ConversionError),
}

impl RetryableError for BodyError {
    fn is_retryable(&self) -> bool {
        false
    }
}
