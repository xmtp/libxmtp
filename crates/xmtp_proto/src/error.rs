use std::array::TryFromSliceError;

use thiserror::Error;
use xmtp_common::{ErrorCode, RetryableError};

#[derive(Clone, Debug, PartialEq, Eq)]
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
    PublishCommitLog,
    QueryCommitLog,
    HealthCheck,
    GetNodes,
    Path(String),
    GetNewestGroupMessage,
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
            PublishCommitLog => write!(f, "publish_commit_log"),
            QueryCommitLog => write!(f, "query_commit_log"),
            HealthCheck => write!(f, "health_check"),
            GetNodes => write!(f, "get_nodes"),
            Path(s) => write!(f, "{}", s),
            GetNewestGroupMessage => write!(f, "get_newest_group_message"),
        }
    }
}

/// General Error types for use when converting/deserializing From/To Protos
/// Loosely Modeled after serdes [error](https://docs.rs/serde/latest/serde/de/value/struct.Error.html) type.
/// This general error type avoid circular hard-dependencies on crates further up the tree
/// (xmtp_id/xmtp_mls) if they had defined the error themselves.
#[derive(thiserror::Error, Debug, ErrorCode)]
pub enum ConversionError {
    /// Missing field.
    ///
    /// Required field missing during proto conversion. Not retryable.
    #[error("missing field {} of type {} during conversion from protobuf", .item, .r#type)]
    Missing {
        /// the item being converted
        item: &'static str,
        /// type of the item being converted
        r#type: &'static str,
    },
    /// Unspecified field.
    ///
    /// Protobuf field is unspecified. Not retryable.
    #[error("field {} unspecified", _0)]
    Unspecified(&'static str),
    /// Deprecated field.
    ///
    /// A deprecated protobuf field was used. Not retryable.
    #[error("field {} deprecated", _0)]
    Deprecated(&'static str),
    /// Invalid length.
    ///
    /// Data has wrong length for conversion. Not retryable.
    #[error("type {} has invalid length. expected {} got {}", .item, .expected, .got)]
    InvalidLength {
        /// the item being converted
        item: &'static str,
        /// expected length of the item being converted
        expected: usize,
        /// the length of the received item
        got: usize,
    },
    /// Invalid value.
    ///
    /// Data has unexpected value. Not retryable.
    #[error("type {} invalid. expected {}, got {}", .item, .expected, .got)]
    InvalidValue {
        /// the item being converted
        item: &'static str,
        /// description of the item expected, i.e 'a negative integer'
        expected: &'static str,
        /// description of the value received i.e 'a positive integer'
        got: String,
    },
    /// Decode error.
    ///
    /// Protobuf decoding failed. Not retryable.
    #[error("decoding proto {0}")]
    Decode(#[from] prost::DecodeError),
    /// Encode error.
    ///
    /// Protobuf encoding failed. Not retryable.
    #[error("encoding proto {0}")]
    Encode(#[from] prost::EncodeError),
    /// Unknown enum value.
    ///
    /// Protobuf enum has unrecognized value. Not retryable.
    #[error("Unknown enum value {0}")]
    UnknownEnumValue(#[from] prost::UnknownEnumValue),
    /// Ed25519 signature error.
    ///
    /// Ed25519 signature bytes invalid. Not retryable.
    // we keep Ed signature bytes on ProtoBuf definitions
    #[error(transparent)]
    EdSignature(#[from] ed25519_dalek::ed25519::Error),

    /// Invalid public key.
    ///
    /// Public key validation failed. Not retryable.
    #[error("{} is invalid: {:?}", .description, .value)]
    InvalidPublicKey {
        // What kind of key is invalid
        description: &'static str,
        // What is the value
        value: Option<String>,
    },
    /// Invalid version.
    ///
    /// Protocol version not supported. Not retryable.
    #[error("version not supported")]
    InvalidVersion,
    /// OpenMLS error.
    ///
    /// OpenMLS library error. Not retryable.
    // TODO: Probably should not be apart of conversion,
    // conversions using openml sshould be put further up the stack
    #[error(transparent)]
    OpenMls(#[from] openmls::prelude::Error),
    /// Protocol message error.
    ///
    /// MLS protocol message error. Not retryable.
    #[error(transparent)]
    Protocol(#[from] openmls::framing::errors::ProtocolMessageError),
    /// Builder error.
    ///
    /// Builder field not initialized. Not retryable.
    #[error(transparent)]
    Builder(#[from] derive_builder::UninitializedFieldError),
    /// Slice error.
    ///
    /// Byte slice conversion failed. Not retryable.
    #[error(transparent)]
    Slice(#[from] TryFromSliceError),
}

// Conversion errors themselves not really retryable because the bytes are static,
// the conversions are done in-memory, so a retrying a conversion should not change the outcome.
// The API call is what should be retried.
// If retry on a conversion error is desired a new error enum + custom Retrayble implementation
// should be preferred.
impl RetryableError for ConversionError {
    fn is_retryable(&self) -> bool {
        false
    }
}

/// Error resulting from proto conversions/mutations
#[derive(Debug, Error, ErrorCode)]
pub enum ProtoError {
    /// Hex error.
    ///
    /// Hex encoding/decoding failed. Not retryable.
    #[error(transparent)]
    Hex(#[from] hex::FromHexError),
    /// Decode error.
    ///
    /// Protobuf decoding failed. Not retryable.
    #[error(transparent)]
    Decode(#[from] prost::DecodeError),
    /// Encode error.
    ///
    /// Protobuf encoding failed. Not retryable.
    #[error(transparent)]
    Encode(#[from] prost::EncodeError),
    /// OpenMLS error.
    ///
    /// OpenMLS library error. Not retryable.
    #[error("Open MLS {0}")]
    OpenMls(#[from] openmls::prelude::Error),
    /// MLS protocol message error.
    ///
    /// MLS framing error. Not retryable.
    #[error(transparent)]
    MlsProtocolMessage(#[from] openmls::framing::errors::ProtocolMessageError),
    /// Key package error.
    ///
    /// Key package verification failed. Not retryable.
    #[error(transparent)]
    KeyPackage(#[from] openmls::prelude::KeyPackageVerifyError),
    /// Not found.
    ///
    /// Proto resource not found. Not retryable.
    #[error("{0} not found")]
    NotFound(String),
}
