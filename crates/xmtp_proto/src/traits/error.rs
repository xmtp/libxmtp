use std::fmt::Display;

use crate::{ApiEndpoint, ProtoError};
use thiserror::Error;
use xmtp_common::{BoxDynError, ErrorCode, RetryableError, retryable};

/// Authentication failures with no credential or callback error text.
#[derive(Clone, Copy, Debug, Error, ErrorCode)]
pub enum AuthError {
    /// The backend rejected the credential. Retryable if a callback can run.
    #[error("credential rejected")]
    CredentialRejected { retryable: bool },
    /// The callback failed. Retryable if a callback can run.
    #[error("auth callback failed")]
    CallbackFailed { retryable: bool },
    /// Authentication is locked until the cool-down ends. Not retryable.
    #[error("auth attempts exhausted")]
    Exhausted,
    /// No credential was set on the handle. Not retryable.
    #[error("auth credential missing")]
    MissingCredential,
}

impl RetryableError for AuthError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::CredentialRejected { retryable } | Self::CallbackFailed { retryable } => {
                *retryable
            }
            Self::Exhausted | Self::MissingCredential => false,
        }
    }
}

impl AuthError {
    /// True while the lockout cool-down runs. The error is not retryable now,
    /// but it clears when the cool-down ends, so a long-lived transport must
    /// wait instead of shutting down. Every other variant needs the caller or
    /// the application to act, so none of them clears on its own.
    pub fn is_locked_out(&self) -> bool {
        matches!(self, Self::Exhausted)
    }
}

impl ApiClientError {
    /// True while an authentication cool-down runs. `#[error(transparent)]`
    /// forwards Display but not `source()`, so the inner `AuthError` cannot be
    /// reached by walking the chain. Match the variant instead.
    pub fn is_locked_out(&self) -> bool {
        matches!(self, Self::Auth(auth) if auth.is_locked_out())
    }
}

#[derive(Debug, Error, ErrorCode)]
#[non_exhaustive]
pub enum ApiClientError {
    #[error(transparent)]
    #[error_code(inherit)]
    Auth(#[from] AuthError),
    /// The client encountered an error.
    #[error("api client at endpoint \"{}\" has error {}", endpoint, source)]
    ClientWithEndpoint {
        endpoint: String,
        /// The client error.
        source: NetworkError,
    },
    /// The transport failed. Retryability follows the source.
    #[error("client errored {}", source)]
    Client { source: NetworkError },
    /// The HTTP request is invalid. Not retryable.
    #[error(transparent)]
    Http(#[from] http::Error),
    /// The request body is invalid. Not retryable.
    #[error(transparent)]
    Body(#[from] BodyError),
    /// The response cannot be decoded. Not retryable.
    #[error(transparent)]
    DecodeError(#[from] prost::DecodeError),
    /// A protocol conversion failed. Not retryable.
    #[error(transparent)]
    Conversion(#[from] crate::ConversionError),
    /// A protocol operation failed. Not retryable.
    #[error(transparent)]
    ProtoError(#[from] ProtoError),
    /// The URI is invalid. Not retryable.
    #[error(transparent)]
    InvalidUri(#[from] http::uri::InvalidUri),
    /// The request expired. Retryable.
    #[error(transparent)]
    Expired(#[from] xmtp_common::time::Expired),
    /// A client operation failed. Retryability follows the source.
    #[error("{0}")]
    Other(Box<dyn RetryableError>),
    /// A client operation failed. Not retryable.
    #[error("{0}")]
    OtherUnretryable(BoxDynError),
    /// Writes are disabled. Not retryable.
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
        Some(self.source.as_ref())
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

impl RetryableError for ApiClientError {
    fn is_retryable(&self) -> bool {
        use ApiClientError::*;
        match self {
            Client { source } => retryable!(*source),
            ClientWithEndpoint { source, .. } => retryable!(source),
            Auth(e) => retryable!(e),
            Body(e) => retryable!(e),
            Http(_) => false,
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

/// Find a typed gRPC status through transport and wrapper error sources.
pub fn grpc_status<'a>(
    mut error: &'a (dyn std::error::Error + 'static),
) -> Option<&'a tonic::Status> {
    loop {
        if let Some(status) = error.downcast_ref::<tonic::Status>() {
            return Some(status);
        }
        error = match error.downcast_ref::<ApiClientError>() {
            Some(ApiClientError::Other(inner)) => inner.as_ref(),
            Some(ApiClientError::OtherUnretryable(inner)) => inner.as_ref(),
            _ => error.source()?,
        };
    }
}
