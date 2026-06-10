use std::fmt::Display;

use crate::{ApiEndpoint, ProtoError};
use thiserror::Error;
use xmtp_common::{BoxDynError, Retryable, RetryableError};

#[derive(Debug, Error, Retryable)]
#[non_exhaustive]
pub enum ApiClientError {
    /// The client encountered an error.
    #[error("api client at endpoint \"{}\" has error {}", endpoint, source)]
    #[retry(when = source.is_retryable())]
    ClientWithEndpoint {
        endpoint: String,
        /// The client error.
        source: NetworkError,
    },
    #[error("client errored {}", source)]
    #[retry(inherit)]
    Client { source: NetworkError },
    #[error(transparent)]
    #[retry(true)]
    Http(#[from] http::Error),
    #[error(transparent)]
    #[retry(inherit)]
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
    #[retry(true)]
    Expired(#[from] xmtp_common::time::Expired),
    #[error("{0}")]
    #[retry(inherit)]
    Other(Box<dyn RetryableError>),
    #[error("{0}")]
    OtherUnretryable(BoxDynError),
    #[error("Writes are disabled on this client.")]
    WritesDisabled,
}

/// A lower level NetworkError, like gRPC/QUIC/HTTP/1.1 errors go here.
/// use [`ApiClientError::new`] to construct
// needed because of AsDynError sealed trait
#[derive(Debug, Retryable)]
#[retry(when = self.source.is_retryable())]
pub struct NetworkError {
    source: Box<dyn RetryableError>,
}

impl std::error::Error for NetworkError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.source.as_ref())
    }
}

impl Display for NetworkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.source)
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

    /// Try to pull a [`NetworkError`] out of this error enum.
    /// returns None if there's no match
    pub fn network_error(&self) -> Option<&NetworkError> {
        use ApiClientError::*;
        match self {
            ClientWithEndpoint { source, .. } | Client { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl ApiClientError {
    pub fn other<R: RetryableError + 'static>(e: R) -> Self {
        ApiClientError::Other(Box::new(e))
    }
}

// Infallible errors by definition can never occur
impl From<std::convert::Infallible> for ApiClientError {
    fn from(_v: std::convert::Infallible) -> ApiClientError {
        unreachable!("Infallible errors can never occur")
    }
}

#[derive(Debug, Error, Retryable)]
pub enum BodyError {
    #[error(transparent)]
    UninitializedField(#[from] derive_builder::UninitializedFieldError),
    #[error(transparent)]
    Conversion(#[from] crate::ConversionError),
}

#[cfg(test)]
mod retryable_golden_tests {
    //! Golden tests pinning the retryability of [`ApiClientError`] after its
    //! migration to `#[derive(Retryable)]`.
    use super::*;

    /// A real `prost::DecodeError` (`DecodeError::new` is deprecated).
    fn decode_error() -> prost::DecodeError {
        <() as prost::Message>::decode(b"\xff".as_slice()).unwrap_err()
    }

    #[xmtp_common::test]
    fn api_client_error_http_and_expired_are_retryable() {
        // Previously: `Http(_) => true,`
        let http_err = http::Error::from("".parse::<http::Uri>().unwrap_err());
        assert!(ApiClientError::Http(http_err).is_retryable());
        // Previously: `Expired(_) => true,`
        assert!(ApiClientError::Expired(xmtp_common::time::Expired).is_retryable());
    }

    #[xmtp_common::test]
    fn api_client_error_client_variants_forward_to_source() {
        // Previously: `Client { source } => retryable!(*source),`
        let retryable_net = NetworkError::new(ApiClientError::Expired(xmtp_common::time::Expired));
        assert!(
            ApiClientError::Client {
                source: retryable_net
            }
            .is_retryable()
        );
        // Previously: `ClientWithEndpoint { source, .. } => retryable!(source),`
        let unretryable_net = NetworkError::new(ApiClientError::WritesDisabled);
        assert!(
            !ApiClientError::ClientWithEndpoint {
                endpoint: "endpoint".into(),
                source: unretryable_net
            }
            .is_retryable()
        );
    }

    #[xmtp_common::test]
    fn api_client_error_non_retryable_variants() {
        // Previously: `DecodeError(_) => false,`
        assert!(!ApiClientError::DecodeError(decode_error()).is_retryable());
        // Previously: `WritesDisabled => false,`
        assert!(!ApiClientError::WritesDisabled.is_retryable());
    }
}
