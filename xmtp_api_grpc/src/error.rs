use thiserror::Error;
use xmtp_proto::ConversionError;

#[derive(Debug, Error)]
pub enum GrpcBuilderError {
    #[error("app version required to create client")]
    MissingAppVersion,
    #[error("libxmtp core library version required to create client")]
    MissingLibxmtpVersion,
    #[error("host url required to create client")]
    MissingHostUrl,
    #[error("payer url required to create client")]
    MissingPayerUrl,
    #[error(transparent)]
    Metadata(#[from] tonic::metadata::errors::InvalidMetadataValue),
    #[error(transparent)]
    Transport(#[from] tonic::transport::Error),
    #[error("Invalid URI during channel creation")]
    InvalidUri(#[from] http::uri::InvalidUri),
}

#[derive(Debug, Error)]
pub enum GrpcError {
    #[error("Invalid URI during channel creation")]
    InvalidUri(#[from] http::uri::InvalidUri),
    #[error(transparent)]
    Transport(#[from] tonic::transport::Error),
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

impl xmtp_proto::XmtpApiError for GrpcError {
    fn api_call(&self) -> Option<xmtp_proto::ApiEndpoint> {
        None
    }

    fn code(&self) -> Option<xmtp_proto::Code> {
        match &self {
            GrpcError::Status(status) => Some(status.code().into()),
            _ => None,
        }
    }

    fn grpc_message(&self) -> Option<&str> {
        match &self {
            GrpcError::Status(status) => Some(status.message()),
            _ => None,
        }
    }
}
