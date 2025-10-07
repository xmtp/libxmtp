use thiserror::Error;
use xmtp_common::RetryableError;

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
    PublishCommitLog,
    QueryCommitLog,
    HealthCheck,
    GetNodes,
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
    #[error("field {} unspecified", _0)]
    Unspecified(&'static str),
    #[error("field {} deprecated", _0)]
    Deprecated(&'static str),
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
        got: String,
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
    #[error("version not supported")]
    InvalidVersion,
    // TODO: Probably should not be apart of conversion,
    // conversions using openml sshould be put further up the stack
    #[error(transparent)]
    OpenMls(#[from] openmls::prelude::Error),
    #[error(transparent)]
    Protocol(#[from] openmls::framing::errors::ProtocolMessageError),
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
