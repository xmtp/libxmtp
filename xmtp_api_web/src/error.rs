use thiserror::Error;
use xmtp_proto::ConversionError;

// This can also be shared with gRPC
#[derive(Debug, Error)]
pub enum GrpcError {
    #[error("Invalid URI during channel creation")]
    InvalidUri(#[from] http::uri::InvalidUri),
    #[error(transparent)]
    Metadata(#[from] tonic::metadata::errors::InvalidMetadataValue),
    #[error(transparent)]
    Status(#[from] tonic::Status),
    #[error("{0} not found/empty")]
    NotFound(String),
    #[error("Payload not expected")]
    UnexpectedPayload,
    #[error("payload is missing")]
    MissingPayload,
    #[error(transparent)]
    Proto(#[from] xmtp_proto::ProtoError),
    #[error(transparent)]
    Decode(#[from] prost::DecodeError),
    #[error("unreachable (Infallible)")]
    Unreachable,
}

impl From<ConversionError> for GrpcError {
    fn from(error: ConversionError) -> Self {
        GrpcError::NotFound(error.to_string())
    }
}

impl xmtp_common::retry::RetryableError for GrpcError {
    fn is_retryable(&self) -> bool {
        true
    }
}
