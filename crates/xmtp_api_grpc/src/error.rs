use thiserror::Error;
use xmtp_common::ErrorCode;
use xmtp_proto::ConversionError;

#[derive(Debug, Error, ErrorCode)]
pub enum GrpcBuilderError {
    /// Missing app version.
    ///
    /// App version not set on builder. Not retryable.
    #[error("app version required to create client")]
    MissingAppVersion,
    /// Missing LibXMTP version.
    ///
    /// Core library version not set. Not retryable.
    #[error("libxmtp core library version required to create client")]
    MissingLibxmtpVersion,
    /// Missing host URL.
    ///
    /// Host URL not set on builder. Not retryable.
    #[error("host url required to create client")]
    MissingHostUrl,
    /// Missing gateway URL.
    ///
    /// xmtpd gateway URL not set. Not retryable.
    #[error("xmtpd gateway url required to create client")]
    MissingXmtpdGatewayUrl,
    /// Metadata error.
    ///
    /// Invalid gRPC metadata value. Not retryable.
    #[error(transparent)]
    Metadata(#[from] tonic::metadata::errors::InvalidMetadataValue),
    /// Invalid URI.
    ///
    /// URI is malformed. Not retryable.
    #[error("Invalid URI during channel creation")]
    InvalidUri(#[from] http::uri::InvalidUri),
    /// URL parse error.
    ///
    /// URL string is malformed. Not retryable.
    #[error(transparent)]
    Url(#[from] url::ParseError),
    /// Transport error.
    ///
    /// gRPC transport creation failed (native only). Not retryable.
    #[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
    #[error(transparent)]
    Transport(#[from] tonic::transport::Error),
}

#[derive(Debug, Error, ErrorCode)]
pub enum GrpcError {
    /// Invalid URI.
    ///
    /// URI for channel creation is malformed. Not retryable.
    #[error("Invalid URI during channel creation")]
    InvalidUri(#[from] http::uri::InvalidUri),
    /// Metadata error.
    ///
    /// Invalid gRPC metadata value. Not retryable.
    #[error(transparent)]
    Metadata(#[from] tonic::metadata::errors::InvalidMetadataValue),
    /// gRPC status error.
    ///
    /// gRPC call returned error status. Retryable.
    #[error(transparent)]
    Status(#[from] tonic::Status),
    /// Not found.
    ///
    /// Requested resource not found or empty. Not retryable.
    #[error("{0} not found/empty")]
    NotFound(String),
    /// Unexpected payload.
    ///
    /// Payload not expected in response. Not retryable.
    #[error("Payload not expected")]
    UnexpectedPayload,
    /// Missing payload.
    ///
    /// Expected payload not in response. Not retryable.
    #[error("payload is missing")]
    MissingPayload,
    #[error(transparent)]
    #[error_code(inherit)]
    Proto(#[from] xmtp_proto::ProtoError),
    /// Decode error.
    ///
    /// Protobuf decoding failed. Not retryable.
    #[error(transparent)]
    Decode(#[from] prost::DecodeError),
    /// Unreachable.
    ///
    /// Infallible error -- should never occur. Not retryable.
    #[error("unreachable (Infallible)")]
    Unreachable,
    /// Transport error.
    ///
    /// gRPC transport layer error (native only). Retryable.
    #[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
    #[error(transparent)]
    Transport(#[from] tonic::transport::Error),
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
