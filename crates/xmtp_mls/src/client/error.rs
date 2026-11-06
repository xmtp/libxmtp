//! Client errors: the client error type, its conversions and retry classification, and
//! the stream failure details.

use super::*;

#[derive(Debug, Error, ErrorCode)]
pub enum ClientError {
    #[error(transparent)]
    #[error_code(inherit)]
    AddressValidation(#[from] IdentifierValidationError),
    /// Could not publish.
    ///
    /// Failed to publish messages to the network. May be retryable.
    #[error("could not publish: {0}")]
    PublishError(String),
    /// Storage error.
    ///
    /// Database operation failed. May be retryable.
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    /// API error.
    ///
    /// Network request to XMTP backend failed. Retryable.
    #[error("API error: {0}")]
    Api(#[from] xmtp_api::ApiError),
    /// Identity error.
    ///
    /// Problem with identity operations. Not retryable.
    #[error("identity error: {0}")]
    Identity(#[from] crate::identity::IdentityError),
    /// TLS Codec error.
    ///
    /// Encoding/decoding MLS TLS structures failed. Not retryable.
    #[error("TLS Codec error: {0}")]
    TlsError(#[from] TlsCodecError),
    /// Key package verification failed.
    ///
    /// Invalid key package received from network. Not retryable.
    #[error("key package verification: {0}")]
    KeyPackageVerification(#[from] KeyPackageVerificationError),
    /// Stream inconsistency.
    ///
    /// Message stream state became inconsistent. Not retryable.
    #[error("Stream inconsistency error: {0}")]
    StreamInconsistency(String),
    /// Association error.
    ///
    /// Identity association operation failed. Not retryable.
    #[error("Association error: {0}")]
    Association(#[from] AssociationError),
    /// Signature validation error.
    ///
    /// A signature failed verification. Not retryable.
    #[error("signature validation error: {0}")]
    SignatureValidation(#[from] SignatureError),
    /// Identity update error.
    ///
    /// Failed to process identity update. Not retryable.
    #[error(transparent)]
    IdentityUpdate(#[from] IdentityUpdateError),
    /// Signature request error.
    ///
    /// Failed to create/process signature request. Not retryable.
    #[error(transparent)]
    SignatureRequest(#[from] SignatureRequestError),
    /// Group error.
    ///
    /// Group operation failed. May be retryable.
    // the box is to prevent infinite cycle between client and group errors
    #[error(transparent)]
    Group(Box<GroupError>),
    /// Local event error.
    ///
    /// Failed to process local event. Not retryable.
    #[error(transparent)]
    LocalEvent(#[from] LocalEventError),
    /// Database connection error.
    ///
    /// Connection to database failed. Retryable.
    #[error(transparent)]
    Db(#[from] xmtp_db::ConnectionError),
    /// Message decode failed.
    ///
    /// Enrichment returned no decoded message for a stored message. Not
    /// retryable.
    #[error("Failed to decode message")]
    MessageDecodeFailed,
    /// MLS store error.
    ///
    /// OpenMLS key store operation failed. Not retryable.
    #[error(transparent)]
    MlsStore(#[from] MlsStoreError),
    /// Message enrichment error.
    ///
    /// Failed to enrich message content. Not retryable.
    #[error(transparent)]
    EnrichMessage(#[from] EnrichMessageError),
    /// Conversion Error
    ///
    /// Data type failed to convert. Not retryable.
    #[error(transparent)]
    Conversion(#[from] xmtp_proto::ConversionError),
    #[error(transparent)]
    #[error_code(inherit)]
    Handle(#[from] crate::inbox_handles::HandleError),
    /// Registration not visible.
    ///
    /// The registration receipt was invalid or was not visible before the timeout.
    /// Call `register_identity` again to retry a pending registration.
    #[error("Registration is not visible")]
    RegistrationNotVisible,
    /// Client is closed.
    ///
    /// Operation was attempted on a client that has been shut down via
    /// `Client::close`. Not retryable — build a new client instead.
    #[error("client is closed")]
    AlreadyClosed,
    /// Server configuration unavailable.
    ///
    /// The backend did not serve its configuration, or the answer could not be
    /// stored. A backend older than spec 006 answers `UNIMPLEMENTED`; there is
    /// no compatibility shim. Retryable exactly when the wrapped failure is:
    /// an unreachable backend is worth another attempt, `UNIMPLEMENTED` is not.
    #[error("server configuration unavailable: {0}")]
    ConfigurationUnavailable(#[source] Box<crate::server_configuration::ConfigurationFetchError>),
    /// Server configuration invalid.
    ///
    /// The backend published a configuration this client cannot use: a missing
    /// or malformed identifier, a minimum version that does not parse, or a
    /// chain that is not a CAIP-2 identifier. Not retryable.
    #[error("server configuration invalid: {0}")]
    ConfigurationInvalid(#[from] xmtp_configuration::ServerConfigurationError),
    /// Backend mismatch.
    ///
    /// This database is bound to one backend deployment and a different one
    /// answered. Not retryable — use a database created for the backend this
    /// app now points at.
    #[error("this database is bound to backend {stored}, but {received} answered")]
    BackendMismatch { stored: String, received: String },
    /// Client version too old.
    ///
    /// The backend requires a newer libxmtp than this build. Not retryable —
    /// ship an updated client.
    #[error("client version {client} is below the {minimum} this backend requires")]
    ClientVersionTooOld { client: String, minimum: String },
    /// Authentication required.
    ///
    /// The backend requires a credential and none was configured. Not
    /// retryable — supply an auth callback or handle before building.
    #[error(
        "this backend requires authentication; required scopes: [{}]",
        required_scopes.join(", ")
    )]
    AuthRequired { required_scopes: Vec<String> },
    /// Chain not accepted.
    ///
    /// The backend does not verify smart contract wallet signatures on this
    /// chain. Not retryable — use a chain the deployment accepts.
    #[error(
        "this backend does not accept smart contract wallet signatures on {chain}; accepted: [{}]",
        accepted.join(", ")
    )]
    ChainNotAccepted {
        chain: String,
        accepted: Vec<String>,
    },
    /// Invalid list options.
    ///
    /// A conversation listing named a limit below 1 or above the maximum page
    /// size, a before conversation id without the before time of its list
    /// order, or sync or one-shot groups. Not retryable — fix the options.
    #[error("invalid list options: {0}")]
    InvalidListOptions(&'static str),
    /// Consent act refused.
    ///
    /// The consent entity cannot be stored, or names a conversation the
    /// client does not hold. Not retryable.
    #[error("consent act: {0}")]
    #[error_code(inherit)]
    ConsentAct(#[from] ConsentActError),
}

impl ClientError {
    #[deny(clippy::wildcard_enum_match_arm)]
    pub fn db_needs_connection(&self) -> bool {
        use crate::worker::NeedsDbReconnect;
        match self {
            Self::Storage(s) | Self::ConsentAct(ConsentActError::Storage(s)) => {
                s.db_needs_connection()
            }
            Self::Db(c) => c.db_needs_connection(),
            Self::Group(error) => error.needs_db_reconnect(),
            Self::Identity(error) => error.needs_db_reconnect(),
            Self::IdentityUpdate(error) => error.needs_db_reconnect(),
            Self::MlsStore(error) => error.needs_db_reconnect(),
            Self::ConfigurationUnavailable(error) => error.db_needs_connection(),
            Self::ConsentAct(_)
            | Self::AddressValidation(..)
            | Self::PublishError(..)
            | Self::Api(..)
            | Self::TlsError(..)
            | Self::KeyPackageVerification(..)
            | Self::StreamInconsistency(..)
            | Self::Association(..)
            | Self::SignatureValidation(..)
            | Self::SignatureRequest(..)
            | Self::LocalEvent(..)
            | Self::MessageDecodeFailed
            | Self::EnrichMessage(..)
            | Self::Conversion(..)
            | Self::Handle(..)
            | Self::RegistrationNotVisible
            | Self::AlreadyClosed
            | Self::ConfigurationInvalid(..)
            | Self::BackendMismatch { .. }
            | Self::ClientVersionTooOld { .. }
            | Self::AuthRequired { .. }
            | Self::ChainNotAccepted { .. }
            | Self::InvalidListOptions(..) => false,
        }
    }
}

impl From<NotFound> for ClientError {
    fn from(value: NotFound) -> Self {
        ClientError::Storage(StorageError::NotFound(value))
    }
}

impl From<GroupError> for ClientError {
    fn from(err: GroupError) -> ClientError {
        ClientError::Group(Box::new(err))
    }
}

impl From<BlockedConnection> for ClientError {
    fn from(blocked_connection: BlockedConnection) -> Self {
        match blocked_connection {
            BlockedConnection::BackendMismatch { stored, received } => {
                ClientError::BackendMismatch { stored, received }
            }
            BlockedConnection::ClientVersionTooOld { client, minimum } => {
                ClientError::ClientVersionTooOld { client, minimum }
            }
        }
    }
}

impl From<ServerConfigError> for ClientError {
    fn from(error: ServerConfigError) -> Self {
        match error {
            ServerConfigError::Unavailable(error) => ClientError::ConfigurationUnavailable(error),
            ServerConfigError::Invalid(error) => ClientError::ConfigurationInvalid(error),
            ServerConfigError::Blocked(blocked_connection) => blocked_connection.into(),
            ServerConfigError::Closed => ClientError::AlreadyClosed,
        }
    }
}

impl xmtp_common::RetryableError for ClientError {
    fn is_retryable(&self) -> bool {
        match self {
            ClientError::Group(group_error) => retryable!(group_error),
            ClientError::IdentityUpdate(error) => retryable!(error),
            ClientError::Api(api_error) => retryable!(api_error),
            ClientError::Storage(storage_error) => retryable!(storage_error),
            ClientError::Db(db) => retryable!(db),
            // SCW verification errors carry retryability through SignatureError;
            // transient RPC provider failures must not advance the welcome cursor.
            // See xmtp/libxmtp#3394.
            ClientError::SignatureValidation(e) => retryable!(e),
            // A backend that was briefly unreachable, or a database that was
            // briefly locked, is worth another attempt: the refresh worker's
            // backoff depends on this answer.
            ClientError::ConfigurationUnavailable(e) => retryable!(e),
            _ => false,
        }
    }

    fn rate_limit_delay(&self) -> Option<xmtp_common::time::Duration> {
        match self {
            ClientError::Group(group_error) => group_error.rate_limit_delay(),
            ClientError::IdentityUpdate(error) => error.rate_limit_delay(),
            ClientError::Api(api_error) => api_error.rate_limit_delay(),
            ClientError::SignatureValidation(e) => e.rate_limit_delay(),
            ClientError::ConfigurationUnavailable(e) => e.rate_limit_delay(),
            _ => None,
        }
    }
}

/// The structured stream failure in `error`, read as
/// [`stream_failure_details`](crate::subscriptions::stream_failure::stream_failure_details)
/// does, and also through [`ClientError::Group`], whose `source()` skips the
/// group error because the variant is transparent.
pub fn stream_failure_details(
    error: &(dyn std::error::Error + 'static),
) -> Option<crate::subscriptions::stream_failure::StreamFailureDetails> {
    crate::subscriptions::stream_failure::stream_failure_details_opening(error, |error| {
        match error.downcast_ref::<ClientError>() {
            Some(ClientError::Group(error)) => Some(error.as_ref()),
            _ => None,
        }
    })
}
