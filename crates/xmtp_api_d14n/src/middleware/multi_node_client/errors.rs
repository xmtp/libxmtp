use thiserror::Error;
use xmtp_api_grpc::error::GrpcBuilderError;
use xmtp_common::RetryableError;
use xmtp_proto::api::{ApiClientError, BodyError};

/// Errors that can occur during multi-node client operations.
#[derive(Debug, Error)]
pub enum MultiNodeClientError {
    #[error("all node clients failed to build")]
    AllNodeClientsFailedToBuild,
    #[error(transparent)]
    BodyError(#[from] BodyError),
    #[error("node {} timed out under {}ms latency", node_id, latency)]
    NodeTimedOut { node_id: u32, latency: u64 },
    #[error("no nodes found")]
    NoNodesFound,
    #[error("no responsive nodes found under {latency}ms latency")]
    NoResponsiveNodesFound { latency: u64 },
    #[error("client builder tls channel does not match url tls channel")]
    TlsChannelMismatch {
        url_is_tls: bool,
        client_builder_tls_channel: bool,
    },
    #[error("unhealthy node {}", node_id)]
    UnhealthyNode { node_id: u32 },
    #[error(transparent)]
    Url(#[from] url::ParseError),
    #[error(transparent)]
    GrpcBuilder(#[from] GrpcBuilderError),
}

///// From<MultiNodeClientError> for ApiClientError is used to convert the MultiNodeClientError to an ApiClientError.
///// Required by the Client trait implementation, as request and stream can return MultiNodeClientError.
impl From<MultiNodeClientError> for ApiClientError {
    fn from(value: MultiNodeClientError) -> ApiClientError {
        ApiClientError::Other(Box::new(value))
    }
}

/// Implements RetryableError to enable proper retry behavior in the API client error handling system.
impl RetryableError for MultiNodeClientError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::BodyError(e) => e.is_retryable(),
            _ => false,
        }
    }
}

/// Errors that can occur when building a MultiNodeClient.
#[derive(Debug, Error)]
pub enum MultiNodeClientBuilderError {
    #[error(transparent)]
    GrpcBuilderError(#[from] GrpcBuilderError),
    #[error("timeout must be greater than 0")]
    InvalidTimeout,
    #[error("gateway builder is required")]
    MissingGatewayBuilder,
    #[error("required fields missing from MultiNodeClientBuilder {0}")]
    Builder(#[from] derive_builder::UninitializedFieldError),
}
