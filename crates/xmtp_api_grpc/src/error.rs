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
    /// Retryability depends on the gRPC status code.
    #[error("{0}")]
    Status(#[from] tonic::Status),
    /// Not found.
    ///
    /// Requested resource not found, empty, or proto conversion failed. Not retryable.
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
    /// Infallible error. Not retryable.
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

impl GrpcError {
    /// Return whether the server reports that the RPC is not implemented.
    pub fn is_unimplemented(&self) -> bool {
        matches!(self, Self::Status(status) if status.code() == tonic::Code::Unimplemented)
    }
}

impl xmtp_common::retry::RetryableError for GrpcError {
    fn is_retryable(&self) -> bool {
        use tonic::Code;

        match self {
            Self::Status(status) => match status.code() {
                Code::InvalidArgument
                | Code::OutOfRange
                | Code::Unimplemented
                | Code::Aborted
                | Code::NotFound
                | Code::AlreadyExists
                | Code::FailedPrecondition
                | Code::PermissionDenied
                | Code::Unauthenticated
                | Code::DataLoss
                | Code::Ok => false,
                Code::Cancelled => is_transport_cancellation(status),
                Code::Unavailable
                | Code::ResourceExhausted
                | Code::DeadlineExceeded
                | Code::Unknown
                | Code::Internal => true,
            },
            #[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
            Self::Transport(_) => true,
            Self::Proto(_)
            | Self::InvalidUri(_)
            | Self::Metadata(_)
            | Self::NotFound(_)
            | Self::UnexpectedPayload
            | Self::MissingPayload
            | Self::Decode(_)
            | Self::Unreachable => false,
        }
    }
}

/// A closed Hyper connection can cancel a request that was not sent.
/// Keep explicit RPC cancellation permanent when this transport cause is absent.
fn is_transport_cancellation(status: &tonic::Status) -> bool {
    let mut source = std::error::Error::source(status);
    while let Some(error) = source {
        if error
            .downcast_ref::<hyper::Error>()
            .is_some_and(hyper::Error::is_canceled)
        {
            return true;
        }
        source = error.source();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::GrpcError;
    use tonic::{Code, Status};
    use xmtp_common::RetryableError;

    #[rstest::rstest]
    #[case(Code::Ok, false)]
    #[case(Code::Cancelled, false)]
    #[case(Code::Unknown, true)]
    #[case(Code::InvalidArgument, false)]
    #[case(Code::DeadlineExceeded, true)]
    #[case(Code::NotFound, false)]
    #[case(Code::AlreadyExists, false)]
    #[case(Code::PermissionDenied, false)]
    #[case(Code::ResourceExhausted, true)]
    #[case(Code::FailedPrecondition, false)]
    #[case(Code::Aborted, false)]
    #[case(Code::OutOfRange, false)]
    #[case(Code::Unimplemented, false)]
    #[case(Code::Internal, true)]
    #[case(Code::Unavailable, true)]
    #[case(Code::DataLoss, false)]
    #[case(Code::Unauthenticated, false)]
    #[xmtp_common::test(unwrap_try = true)]
    async fn retry_by_status_code(#[case] code: Code, #[case] retryable: bool) {
        for message in ["", "UNAVAILABLE", "INVALID_ARGUMENT", "request too large"] {
            let error = GrpcError::Status(Status::new(code, message));
            assert_eq!(error.is_retryable(), retryable, "{code:?}: {message}");
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn explicit_rpc_cancellation_is_not_retryable() {
        use xmtp_proto::api::{ApiClientError, NetworkError, grpc_status};

        for message in ["", "operation was canceled", "connection closed"] {
            let error = NetworkError::new(ApiClientError::client(GrpcError::Status(
                Status::cancelled(message),
            )));
            assert_eq!(grpc_status(&error)?.code(), Code::Cancelled);
            assert!(!error.is_retryable());
        }
        let mut status = Status::cancelled("operation was canceled");
        status.set_source(std::sync::Arc::new(std::io::Error::other(
            "connection closed",
        )));
        assert!(!GrpcError::Status(status).is_retryable());
    }

    #[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
    #[xmtp_common::test(unwrap_try = true)]
    async fn cancelled_hyper_request_is_retryable_through_tonic_and_client_wrappers() {
        use http_body_util::Empty;
        use hyper_util::rt::TokioIo;
        use prost::bytes::Bytes;
        use xmtp_common::time::{Duration, timeout};
        use xmtp_proto::api::{ApiClientError, NetworkError, grpc_status};

        type Body = Empty<Bytes>;
        for wrapped in [false, true] {
            let (io, _peer) = tokio::io::duplex(64);
            let (mut sender, connection) =
                hyper::client::conn::http1::handshake::<_, Body>(TokioIo::new(io)).await?;
            drop(connection);
            let cancelled = sender
                .send_request(http::Request::new(Body::new()))
                .await
                .expect_err("the dropped connection cannot send the request");
            assert!(cancelled.is_canceled());

            let status = if wrapped {
                let mut cancelled = Some(cancelled);
                let connector = tower::service_fn(move |_| {
                    std::future::ready(Err::<TokioIo<tokio::io::DuplexStream>, _>(
                        cancelled.take().expect("one connection attempt"),
                    ))
                });
                let endpoint = tonic::transport::Endpoint::from_static("http://unused.invalid");
                let transport = timeout(
                    Duration::from_secs(5),
                    endpoint.connect_with_connector(connector),
                )
                .await?
                .expect_err("the connector returns the cancelled request");
                // Connection-open failures map to Unavailable. Attach only their typed cause.
                let mut status = Status::cancelled("retained transport cause");
                status.set_source(std::sync::Arc::new(transport));
                status
            } else {
                Status::from_error(Box::new(cancelled))
            };
            assert_eq!(status.code(), Code::Cancelled);
            let error = GrpcError::Status(status);
            assert!(error.is_retryable());
            let error = ApiClientError::client(error);
            assert!(error.is_retryable());
            let error = NetworkError::new(error);
            assert_eq!(grpc_status(&error)?.code(), Code::Cancelled);
            assert!(error.is_retryable());
        }
    }
}

#[cfg(test)]
mod status_sources {
    use super::GrpcError;
    use xmtp_proto::api::{ApiClientError, grpc_status};

    #[xmtp_common::test(unwrap_try = true)]
    fn typed_status_survives_client_error_wrappers() {
        let error =
            ApiClientError::client(GrpcError::Status(tonic::Status::aborted("unchanged text")));
        assert_eq!(grpc_status(&error)?.code(), tonic::Code::Aborted);
        let error = ApiClientError::other(GrpcError::Status(tonic::Status::out_of_range(
            "unchanged text",
        )));
        assert_eq!(grpc_status(&error)?.code(), tonic::Code::OutOfRange);
    }
}
