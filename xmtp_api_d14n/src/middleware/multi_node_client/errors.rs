use thiserror::Error;
use xmtp_api_grpc::error::GrpcError;
use xmtp_common::RetryableError;
use xmtp_proto::api::{ApiClientError, BodyError};

/// Errors that can occur during multi-node client operations.
#[derive(Debug, Error)]
pub enum MultiNodeClientError {
    #[error("all node clients failed to build")]
    AllNodeClientsFailedToBuild,
    #[error(transparent)]
    BodyError(#[from] BodyError),
    #[error(transparent)]
    GrpcError(#[from] ApiClientError<GrpcError>),
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
}

/// From<MultiNodeClientError> for ApiClientError<E> is used to convert the MultiNodeClientError to an ApiClientError.
/// Required by the Client trait implementation, as request and stream can return MultiNodeClientError.
impl<E> From<MultiNodeClientError> for ApiClientError<E>
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn from(value: MultiNodeClientError) -> ApiClientError<E> {
        ApiClientError::<E>::Other(Box::new(value))
    }
}

/// Implements RetryableError to enable proper retry behavior in the API client error handling system.
impl RetryableError for MultiNodeClientError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::GrpcError(e) => e.is_retryable(),
            Self::BodyError(e) => e.is_retryable(),
            _ => false,
        }
    }
}
