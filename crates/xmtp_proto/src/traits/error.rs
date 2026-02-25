use std::fmt::Display;

use crate::api_client::AggregateStats;
use crate::{ApiEndpoint, ProtoError};
use thiserror::Error;
use xmtp_common::{BoxDynError, RetryableError, retryable};

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ApiClientError {
    #[error(
        "api client at endpoint \"{}\" has error {}. \n {:?} \n",
        endpoint,
        source,
        stats
    )]
    ClientWithEndpointAndStats {
        endpoint: String,
        source: NetworkError,
        stats: AggregateStats,
    },
    #[error("API Error {}, \n {:?} \n", e, stats)]
    ErrorWithStats {
        e: NetworkError,
        stats: AggregateStats,
    },
    /// The client encountered an error.
    #[error("api client at endpoint \"{}\" has error {}", endpoint, source)]
    ClientWithEndpoint {
        endpoint: String,
        /// The client error.
        source: NetworkError,
    },
    #[error("client errored {}", source)]
    Client { source: NetworkError },
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
    Other(Box<dyn RetryableError>),
    #[error("{0}")]
    OtherUnretryable(BoxDynError),
    #[error("Writes are disabled on this client.")]
    WritesDisabled,
}

/// A lower level NetworkError, like gRPC/QUIC/HTTP/1.1 errors go here.
/// use [`ApiClientError::new`] to construct
// needed because of AsDynError sealed trait
#[derive(Debug)]
pub struct NetworkError {
    source: Box<dyn RetryableError>,
}

impl std::error::Error for NetworkError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.source()
    }
}

impl Display for NetworkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.source)
    }
}

impl RetryableError for NetworkError {
    fn is_retryable(&self) -> bool {
        self.source.is_retryable()
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
}

impl ApiClientError {
    pub fn other<R: RetryableError + 'static>(e: R) -> Self {
        ApiClientError::Other(Box::new(e))
    }
}

impl RetryableError for ApiClientError {
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
            WritesDisabled => false,
        }
    }
}

// Infallible errors by definition can never occur
impl From<std::convert::Infallible> for ApiClientError {
    fn from(_v: std::convert::Infallible) -> ApiClientError {
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
