use thiserror::Error;
use xmtp_common::retry::RetryableError;

pub trait XmtpApiError:
    std::fmt::Debug + std::fmt::Display + std::error::Error + Send + Sync + RetryableError
{
    /// The failing ApiCall
    fn api_call(&self) -> Option<ApiEndpoint>;
    /// grpc status error code
    fn code(&self) -> Option<Code>;
    /// message associated with this gRPC Error, if any.
    /// this is not the same as the Display implementation
    fn grpc_message(&self) -> Option<&str>;
}

#[derive(Error, Debug)]
pub struct ApiError {
    inner: Box<dyn XmtpApiError>,
}

impl RetryableError for ApiError {
    fn is_retryable(&self) -> bool {
        self.inner.is_retryable()
    }

    fn needs_cooldown(&self) -> bool {
        if let Some(code) = self.inner.code() {
            code == Code::ResourceExhausted
        } else {
            false
        }
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl<E> From<E> for ApiError
where
    E: XmtpApiError + std::error::Error + std::fmt::Display + std::fmt::Debug + 'static,
{
    fn from(v: E) -> ApiError {
        Self { inner: Box::new(v) }
    }
}

// GRPC Error Code
#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub enum Code {
    /// The operation completed successfully.
    Ok = 0,
    /// The operation was cancelled.
    Cancelled = 1,
    /// Unknown error.
    Unknown = 2,
    /// Client specified an invalid argument.
    InvalidArgument = 3,
    /// Deadline expired before operation could complete.
    DeadlineExceeded = 4,
    /// Some requested entity was not found.
    NotFound = 5,
    /// Some entity that we attempted to create already exists.
    AlreadyExists = 6,
    /// The caller does not have permission to execute the specified operation.
    PermissionDenied = 7,
    /// Some resource has been exhausted (rate limit).
    ResourceExhausted = 8,
    /// The system is not in a state required for the operation's execution.
    FailedPrecondition = 9,
    /// The operation was aborted.
    Aborted = 10,
    /// Operation was attempted past the valid range.
    OutOfRange = 11,
    /// Operation is not implemented or not supported.
    Unimplemented = 12,
    /// Internal error.
    Internal = 13,
    /// The service is currently unavailable.
    Unavailable = 14,
    /// Unrecoverable data loss or corruption.
    DataLoss = 15,
    /// The request does not have valid authentication credentials
    Unauthenticated = 16,
}

impl From<usize> for Code {
    fn from(v: usize) -> Code {
        use Code::*;
        match v {
            0 => Ok,
            1 => Cancelled,
            2 => Unknown,
            3 => InvalidArgument,
            4 => DeadlineExceeded,
            5 => NotFound,
            6 => AlreadyExists,
            7 => PermissionDenied,
            8 => ResourceExhausted,
            9 => FailedPrecondition,
            10 => Aborted,
            11 => OutOfRange,
            12 => Unimplemented,
            13 => Internal,
            14 => Unavailable,
            15 => DataLoss,
            16 => Unauthenticated,
            _ => Unknown,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod convert {
    impl From<super::Code> for tonic::Code {
        fn from(v: super::Code) -> tonic::Code {
            match v {
                super::Code::Ok => tonic::Code::Ok,
                super::Code::Cancelled => tonic::Code::Cancelled,
                super::Code::Unknown => tonic::Code::Unknown,
                super::Code::InvalidArgument => tonic::Code::InvalidArgument,
                super::Code::DeadlineExceeded => tonic::Code::DeadlineExceeded,
                super::Code::NotFound => tonic::Code::NotFound,
                super::Code::AlreadyExists => tonic::Code::AlreadyExists,
                super::Code::PermissionDenied => tonic::Code::PermissionDenied,
                super::Code::ResourceExhausted => tonic::Code::ResourceExhausted,
                super::Code::FailedPrecondition => tonic::Code::FailedPrecondition,
                super::Code::Aborted => tonic::Code::Aborted,
                super::Code::OutOfRange => tonic::Code::OutOfRange,
                super::Code::Unimplemented => tonic::Code::Unimplemented,
                super::Code::Internal => tonic::Code::Internal,
                super::Code::Unavailable => tonic::Code::Unavailable,
                super::Code::DataLoss => tonic::Code::DataLoss,
                super::Code::Unauthenticated => tonic::Code::Unauthenticated,
            }
        }
    }

    impl From<tonic::Code> for super::Code {
        fn from(v: tonic::Code) -> super::Code {
            match v {
                tonic::Code::Ok => super::Code::Ok,
                tonic::Code::Cancelled => super::Code::Cancelled,
                tonic::Code::Unknown => super::Code::Unknown,
                tonic::Code::InvalidArgument => super::Code::InvalidArgument,
                tonic::Code::DeadlineExceeded => super::Code::DeadlineExceeded,
                tonic::Code::NotFound => super::Code::NotFound,
                tonic::Code::AlreadyExists => super::Code::AlreadyExists,
                tonic::Code::PermissionDenied => super::Code::PermissionDenied,
                tonic::Code::ResourceExhausted => super::Code::ResourceExhausted,
                tonic::Code::FailedPrecondition => super::Code::FailedPrecondition,
                tonic::Code::Aborted => super::Code::Aborted,
                tonic::Code::OutOfRange => super::Code::OutOfRange,
                tonic::Code::Unimplemented => super::Code::Unimplemented,
                tonic::Code::Internal => super::Code::Internal,
                tonic::Code::Unavailable => super::Code::Unavailable,
                tonic::Code::DataLoss => super::Code::DataLoss,
                tonic::Code::Unauthenticated => super::Code::Unauthenticated,
            }
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ApiEndpoint {
    Publish,
    SubscribeGroupMessages,
    SubscribeWelcomes,
    UploadKeyPackage,
    FetchKeyPackages,
    SendGroupMessages,
    SendWelcomeMessages,
    QueryGroupMessages,
    QueryWelcomeMessages,
    PublishIdentityUpdate,
    GetInboxIds,
    GetIdentityUpdatesV2,
    VerifyScwSignature,
    QueryV4Envelopes,
    PublishEnvelopes,
}

impl std::fmt::Display for ApiEndpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        use ApiEndpoint::*;
        match self {
            Publish => write!(f, "publish"),
            SubscribeGroupMessages => write!(f, "subscribe_group_messages"),
            SubscribeWelcomes => write!(f, "subscribe_welcomes"),
            UploadKeyPackage => write!(f, "upload_key_package"),
            FetchKeyPackages => write!(f, "fetch_key_packages"),
            SendGroupMessages => write!(f, "send_group_messages"),
            SendWelcomeMessages => write!(f, "send_welcome_messages"),
            QueryGroupMessages => write!(f, "query_group_messages"),
            QueryWelcomeMessages => write!(f, "query_welcome_messages"),
            PublishIdentityUpdate => write!(f, "publish_identity_update"),
            GetInboxIds => write!(f, "get_inbox_ids"),
            GetIdentityUpdatesV2 => write!(f, "get_identity_updates_v2"),
            VerifyScwSignature => write!(f, "verify_scw_signature"),
            QueryV4Envelopes => write!(f, "query_v4_envelopes"),
            PublishEnvelopes => write!(f, "publish_envelopes"),
        }
    }
}

/// General Error types for use when converting/deserializing From/To Protos
/// Loosely Modeled after serdes [error](https://docs.rs/serde/latest/serde/de/value/struct.Error.html) type.
/// This general error type avoid circular hard-dependencies on crates further up the tree
/// (xmtp_id/xmtp_mls) if they had defined the error themselves.
#[derive(thiserror::Error, Debug)]
pub enum ConversionError {
    #[error("missing field {} of type {} during conversion from protobuf", .item, .r#type)]
    Missing {
        /// the item being converted
        item: &'static str,
        /// type of the item being converted
        r#type: &'static str,
    },
    #[error("type {} has invalid length. expected {} got {}", .item, .expected, .got)]
    InvalidLength {
        /// the item being converted
        item: &'static str,
        /// expected length of the item being converted
        expected: usize,
        /// the length of the received item
        got: usize,
    },
    #[error("type {} invalid. expected {}, got {}", .item, .expected, .got)]
    InvalidValue {
        /// the item being converted
        item: &'static str,
        /// description of the item expected, i.e 'a negative integer'
        expected: &'static str,
        /// description of the value received i.e 'a positive integer'
        got: &'static str,
    },
    #[error("decoding proto {0}")]
    Decode(#[from] prost::DecodeError),
    // we keep Ed signature bytes on ProtoBuf definitions
    #[error(transparent)]
    EdSignature(#[from] ed25519_dalek::ed25519::Error),

    #[error("{} is invalid: {:?}", .description, .value)]
    InvalidPublicKey {
        // What kind of key is invalid
        description: &'static str,
        // What is the value
        value: Option<String>,
    },
}

/// Error resulting from proto conversions/mutations
#[derive(Debug, Error)]
pub enum ProtoError {
    #[error(transparent)]
    Hex(#[from] hex::FromHexError),
    #[error(transparent)]
    Decode(#[from] prost::DecodeError),
    #[error(transparent)]
    Encode(#[from] prost::EncodeError),
    #[error("Open MLS {0}")]
    OpenMls(#[from] openmls::prelude::Error),
    #[error(transparent)]
    MlsProtocolMessage(#[from] openmls::framing::errors::ProtocolMessageError),
    #[error(transparent)]
    KeyPackage(#[from] openmls::prelude::KeyPackageVerifyError),
    #[error("{0} not found")]
    NotFound(String),
}
