use thiserror::Error;
use xmtp_api_grpc::error::GrpcBuilderError;

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
