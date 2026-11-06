use thiserror::Error;
use xmtp_common::{ErrorCode, time::Duration};
use xmtp_proto::ConversionError;

const ERROR_INFO_TYPE_URL: &str = "type.googleapis.com/google.rpc.ErrorInfo";
const RETRY_INFO_TYPE_URL: &str = "type.googleapis.com/google.rpc.RetryInfo";

/// The `RetryInfo` delay of a rate-limit rejection (RATE-040) found anywhere in
/// the error's sources, or `None` when the error is not one. A rejection is a
/// `RESOURCE_EXHAUSTED` status with an `ErrorInfo` of the rate-limit reason and
/// domain. Any other status, `RESOURCE_EXHAUSTED` without that detail and any
/// other code with it included, keeps its own handling. A missing or negative
/// delay counts as zero. The status message is never read.
// implements: RATE-041
pub fn rate_limit_delay(error: &(dyn std::error::Error + 'static)) -> Option<Duration> {
    status_rate_limit_delay(xmtp_proto::api::grpc_status(error)?)
}

/// A rate-limit rejection whose `RetryInfo` delay is these raw protobuf
/// fields, which may be out of range, for tests.
#[cfg(any(test, feature = "test-utils"))]
pub fn rate_limited_status_raw(seconds: i64, nanos: i32) -> tonic::Status {
    use prost::Message;
    use tonic_types::pb;
    let code = tonic::Code::ResourceExhausted;
    let mut info = pb::RetryInfo::default();
    let mut delay = info.retry_delay.take().unwrap_or_default();
    delay.seconds = seconds;
    delay.nanos = nanos;
    info.retry_delay = Some(delay);
    let mut status = pb::Status {
        code: code as i32,
        message: "rate limit exceeded".into(),
        details: vec![Default::default(), Default::default()],
    };
    let error_info = pb::ErrorInfo {
        reason: xmtp_configuration::RATE_LIMITED_REASON.into(),
        domain: xmtp_configuration::RATE_LIMITED_DOMAIN.into(),
        metadata: Default::default(),
    };
    status.details[0].type_url = ERROR_INFO_TYPE_URL.into();
    status.details[0].value = error_info.encode_to_vec();
    status.details[1].type_url = RETRY_INFO_TYPE_URL.into();
    status.details[1].value = info.encode_to_vec();
    tonic::Status::with_details(code, "rate limit exceeded", status.encode_to_vec().into())
}

/// A rate-limit rejection as the backend sends it (RATE-040), for tests.
#[cfg(any(test, feature = "test-utils"))]
pub fn rate_limited_status(category: &str, delay: Duration) -> tonic::Status {
    use tonic_types::{ErrorDetails, StatusExt};
    let mut details = ErrorDetails::with_error_info(
        xmtp_configuration::RATE_LIMITED_REASON,
        xmtp_configuration::RATE_LIMITED_DOMAIN,
        [("category".to_string(), category.to_string())],
    );
    details.set_retry_info(Some(delay));
    tonic::Status::with_error_details(
        tonic::Code::ResourceExhausted,
        "rate limit exceeded",
        details,
    )
}

/// Decode each detail on its own, so a malformed detail beside the rate-limit
/// `ErrorInfo` cannot hide it.
fn status_rate_limit_delay(status: &tonic::Status) -> Option<Duration> {
    use prost::Message;
    use tonic_types::pb;
    if status.code() != tonic::Code::ResourceExhausted {
        return None;
    }
    let details = pb::Status::decode(status.details()).ok()?.details;
    let rate_limited = details
        .iter()
        .filter(|detail| detail.type_url == ERROR_INFO_TYPE_URL)
        .filter_map(|detail| pb::ErrorInfo::decode(detail.value.as_slice()).ok())
        .any(|info| {
            info.reason == xmtp_configuration::RATE_LIMITED_REASON
                && info.domain == xmtp_configuration::RATE_LIMITED_DOMAIN
        });
    if !rate_limited {
        return None;
    }
    let delay = details
        .iter()
        .filter(|detail| detail.type_url == RETRY_INFO_TYPE_URL)
        .filter_map(|detail| pb::RetryInfo::decode(detail.value.as_slice()).ok())
        .find_map(|info| info.retry_delay)
        .map_or(Duration::ZERO, |delay| {
            non_negative(delay.seconds, delay.nanos)
        });
    Some(delay)
}

/// A protobuf duration as a `Duration`; a negative one counts as zero.
fn non_negative(seconds: i64, nanos: i32) -> Duration {
    const NANOS_PER_SEC: i128 = 1_000_000_000;
    let total = i128::from(seconds) * NANOS_PER_SEC + i128::from(nanos);
    if total <= 0 {
        return Duration::ZERO;
    }
    Duration::new(
        u64::try_from(total / NANOS_PER_SEC).unwrap_or(u64::MAX),
        (total % NANOS_PER_SEC) as u32,
    )
}

#[derive(Debug, Error, ErrorCode)]
pub enum GrpcBuilderError {
    /// Unsupported backend URL components. Not retryable.
    #[error("backend URL must be an origin without userinfo, query, or fragment")]
    InvalidBackendUrl,
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

    fn rate_limit_delay(&self) -> Option<Duration> {
        match self {
            Self::Status(status) => status_rate_limit_delay(status),
            _ => None,
        }
    }
}

/// Hyper connection closure and Tonic's local timeout can produce Cancelled.
/// Keep explicit RPC cancellation permanent when these typed causes are absent.
fn is_transport_cancellation(status: &tonic::Status) -> bool {
    let mut source = std::error::Error::source(status);
    while let Some(error) = source {
        if error.is::<tonic::TimeoutExpired>()
            || error
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

        for message in [
            "",
            "operation was canceled",
            "connection closed",
            "Timeout expired",
        ] {
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
    async fn local_timeout_is_retryable_through_tonic_and_client_wrappers() {
        use hyper_util::rt::TokioIo;
        use xmtp_common::time::{Duration, timeout};
        use xmtp_proto::api::{ApiClientError, NetworkError, grpc_status};

        for wrapped in [false, true] {
            let status = if wrapped {
                let connector = tower::service_fn(move |_| {
                    std::future::ready(Err::<TokioIo<tokio::io::DuplexStream>, _>(
                        tonic::TimeoutExpired(()),
                    ))
                });
                let endpoint = tonic::transport::Endpoint::from_static("http://unused.invalid");
                let transport = timeout(
                    Duration::from_secs(5),
                    endpoint.connect_with_connector(connector),
                )
                .await?
                .expect_err("the connector returns the local timeout");
                // Keep the actual transport wrapper and its typed timeout cause.
                let mut status = Status::cancelled("retained transport cause");
                status.set_source(std::sync::Arc::new(transport));
                status
            } else {
                Status::from_error(Box::new(tonic::TimeoutExpired(())))
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
pub(crate) mod rate_limit_tests {
    use super::{ERROR_INFO_TYPE_URL, GrpcError, RETRY_INFO_TYPE_URL, rate_limit_delay};
    use prost::Message;
    use tonic::{Code, Status};
    use tonic_types::{ErrorDetails, StatusExt, pb};
    use xmtp_common::{RetryableError, time::Duration};
    use xmtp_proto::api::ApiClientError;

    const DAY: Duration = Duration::from_secs(24 * 60 * 60);

    fn status(code: Code, reason: &str, domain: &str, delay: Option<Duration>) -> Status {
        let mut details = ErrorDetails::with_error_info(
            reason,
            domain,
            [("category".to_string(), "publish".to_string())],
        );
        if let Some(delay) = delay {
            details.set_retry_info(Some(delay));
        }
        Status::with_error_details(code, "rate limited", details)
    }

    fn any(type_url: &str, value: Vec<u8>) -> prost_types::Any {
        prost_types::Any {
            type_url: type_url.into(),
            value,
        }
    }

    fn rate_limit_info() -> prost_types::Any {
        any(
            ERROR_INFO_TYPE_URL,
            pb::ErrorInfo {
                reason: "RATE_LIMITED".into(),
                domain: "xmtp.org".into(),
                metadata: Default::default(),
            }
            .encode_to_vec(),
        )
    }

    fn raw_status(details: Vec<prost_types::Any>) -> Status {
        let code = Code::ResourceExhausted;
        let details = pb::Status {
            code: code as i32,
            message: "rate limited".into(),
            details,
        };
        Status::with_details(code, "rate limited", details.encode_to_vec().into())
    }

    // verifies: RATE-041
    #[rstest::rstest]
    #[case::delay(
        Code::ResourceExhausted,
        "RATE_LIMITED",
        "xmtp.org",
        Some(Duration::from_secs(5)),
        Some(Duration::from_secs(5))
    )]
    #[case::no_retry_info(
        Code::ResourceExhausted,
        "RATE_LIMITED",
        "xmtp.org",
        None,
        Some(Duration::ZERO)
    )]
    #[case::long_window(Code::ResourceExhausted, "RATE_LIMITED", "xmtp.org", Some(DAY * 20), Some(DAY * 20))]
    #[case::other_domain(
        Code::ResourceExhausted,
        "RATE_LIMITED",
        "example.com",
        Some(Duration::from_secs(5)),
        None
    )]
    #[case::other_reason(
        Code::ResourceExhausted,
        "TOO_MANY_TOPICS",
        "xmtp.org",
        Some(Duration::from_secs(5)),
        None
    )]
    #[case::unavailable(
        Code::Unavailable,
        "RATE_LIMITED",
        "xmtp.org",
        Some(Duration::from_secs(5)),
        None
    )]
    #[case::permission_denied(
        Code::PermissionDenied,
        "RATE_LIMITED",
        "xmtp.org",
        Some(Duration::from_secs(5)),
        None
    )]
    #[case::out_of_range(
        Code::OutOfRange,
        "RATE_LIMITED",
        "xmtp.org",
        Some(Duration::from_secs(5)),
        None
    )]
    #[xmtp_common::test(unwrap_try = true)]
    async fn rate_limit_is_resource_exhausted_with_the_detail(
        #[case] code: Code,
        #[case] reason: &str,
        #[case] domain: &str,
        #[case] delay: Option<Duration>,
        #[case] expected: Option<Duration>,
    ) {
        let error = GrpcError::Status(status(code, reason, domain, delay));
        assert_eq!(error.rate_limit_delay(), expected);
        for wrapped in [
            ApiClientError::client(GrpcError::Status(status(code, reason, domain, delay))),
            ApiClientError::other(GrpcError::Status(status(code, reason, domain, delay))),
        ] {
            assert_eq!(wrapped.rate_limit_delay(), expected);
            assert_eq!(rate_limit_delay(&wrapped), expected);
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn a_plain_resource_exhausted_status_is_not_a_rate_limit() {
        let error = GrpcError::Status(Status::resource_exhausted("RATE_LIMITED xmtp.org"));
        assert!(error.is_retryable());
        assert_eq!(error.rate_limit_delay(), None);
        assert_eq!(rate_limit_delay(&error), None);
    }

    /// One malformed detail does not hide the rate-limit `ErrorInfo`.
    // verifies: RATE-041
    #[xmtp_common::test(unwrap_try = true)]
    fn a_malformed_detail_does_not_hide_the_rate_limit() {
        let delay = prost_types::Duration {
            seconds: 5,
            nanos: 0,
        };
        let retry = any(
            RETRY_INFO_TYPE_URL,
            pb::RetryInfo {
                retry_delay: Some(delay),
            }
            .encode_to_vec(),
        );
        let malformed_debug = any("type.googleapis.com/google.rpc.DebugInfo", vec![0xff]);
        let error = GrpcError::Status(raw_status(vec![rate_limit_info(), malformed_debug, retry]));
        assert_eq!(error.rate_limit_delay(), Some(Duration::from_secs(5)));
        let malformed_retry = any(RETRY_INFO_TYPE_URL, vec![0xff]);
        let error = GrpcError::Status(raw_status(vec![rate_limit_info(), malformed_retry]));
        assert_eq!(error.rate_limit_delay(), Some(Duration::ZERO));
    }

    /// A negative `retry_delay` counts as zero.
    // verifies: RATE-041
    #[rstest::rstest]
    #[case(-5, 0)]
    #[case(0, -1)]
    #[xmtp_common::test(unwrap_try = true)]
    async fn a_negative_delay_counts_as_zero(#[case] seconds: i64, #[case] nanos: i32) {
        let retry = any(
            RETRY_INFO_TYPE_URL,
            pb::RetryInfo {
                retry_delay: Some(prost_types::Duration { seconds, nanos }),
            }
            .encode_to_vec(),
        );
        let error = GrpcError::Status(raw_status(vec![rate_limit_info(), retry]));
        assert_eq!(error.rate_limit_delay(), Some(Duration::ZERO));
    }

    /// The details survive the gRPC trailers a status is carried in.
    // verifies: RATE-041
    #[xmtp_common::test(unwrap_try = true)]
    fn the_rate_limit_survives_the_status_trailers() {
        let sent = super::rate_limited_status("stream_rent", Duration::from_secs(5));
        let mut trailers = http::HeaderMap::new();
        sent.add_header(&mut trailers)?;
        let received = Status::from_header_map(&trailers)?;
        let error = ApiClientError::client(GrpcError::Status(received));
        assert_eq!(rate_limit_delay(&error), Some(Duration::from_secs(5)));
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
