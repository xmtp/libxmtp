//! Identity update errors and installation diff errors, with their retry and
//! reconnect classification.

use super::*;

/// A failure to read, verify, build, or publish identity updates.
#[derive(Debug, Error, ErrorCode)]
pub enum IdentityUpdateError {
    /// Signature request error.
    ///
    /// The signature request is incomplete, or asks for more actions than
    /// one identity update may carry. Not retryable.
    #[error(transparent)]
    InvalidSignatureRequest(#[from] SignatureRequestError),
    /// API error.
    ///
    /// Network request to XMTP backend failed. Retryable when the request is.
    #[error(transparent)]
    Api(#[from] xmtp_api::ApiError),
    /// Validation error.
    ///
    /// The identity update did not validate against its history. Not retryable.
    #[error(transparent)]
    Validation(#[from] xmtp_mls_validation::ValidationError),
    /// Storage error.
    ///
    /// Database operation failed. May be retryable.
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    /// Database connection error.
    ///
    /// Connection to database failed. Retryable.
    #[error(transparent)]
    Db(#[from] xmtp_db::ConnectionError),
    /// Association error.
    ///
    /// Identity association operation failed. Not retryable.
    #[error("Association error: {0}")]
    Association(#[from] AssociationError),
    /// Signature validation error.
    ///
    /// A signature failed verification. Retryable when the verifier failed
    /// transiently.
    #[error("signature validation error: {0}")]
    SignatureValidation(#[from] SignatureError),
    /// Identity error.
    ///
    /// Problem with identity operations. Not retryable.
    #[error("identity error: {0}")]
    Identity(#[from] IdentityError),
    /// Conversion error.
    ///
    /// Data type failed to convert. Not retryable.
    #[error(transparent)]
    Conversion(#[from] xmtp_proto::ConversionError),
    #[error(transparent)]
    #[error_code(inherit)]
    AddressValidation(#[from] IdentifierValidationError),
    #[error(transparent)]
    #[error_code(inherit)]
    Handle(#[from] crate::inbox_handles::HandleError),
    /// The connection is blocked. Not retryable.
    #[error(transparent)]
    #[error_code(inherit)]
    Blocked(#[from] xmtp_mls_server_config::server_configuration::BlockedConnection),
    /// Inbox not found.
    ///
    /// The backend holds no identity update for the inbox. Not retryable.
    #[error("Unable to find provided inbox_id")]
    InboxNotFound,
    /// Empty identity log.
    ///
    /// The inbox's identity updates produced no association state. Not
    /// retryable.
    #[error("Unable to create association state")]
    EmptyIdentityLog,
    /// Publish failed.
    ///
    /// Publishing a signed identity update failed after its conflict retries.
    /// Not retryable: the caller signs and applies a new request.
    #[error(transparent)]
    Publish(Box<IdentityUpdateError>),
}

impl From<xmtp_db::NotFound> for IdentityUpdateError {
    fn from(value: xmtp_db::NotFound) -> Self {
        Self::Storage(StorageError::NotFound(value))
    }
}

impl RetryableError for IdentityUpdateError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::Api(error) => retryable!(error),
            Self::Storage(error) => retryable!(error),
            Self::Db(error) => retryable!(error),
            // SCW verification errors carry retryability through SignatureError;
            // transient RPC provider failures must not advance the welcome cursor.
            // See xmtp/libxmtp#3394.
            Self::SignatureValidation(error) => retryable!(error),
            Self::InvalidSignatureRequest(_)
            | Self::Validation(_)
            | Self::Association(_)
            | Self::Identity(_)
            | Self::Conversion(_)
            | Self::AddressValidation(_)
            | Self::Handle(_)
            | Self::Blocked(_)
            | Self::InboxNotFound
            | Self::EmptyIdentityLog
            | Self::Publish(_) => false,
        }
    }

    fn rate_limit_delay(&self) -> Option<xmtp_common::time::Duration> {
        match self {
            Self::Api(error) => error.rate_limit_delay(),
            Self::SignatureValidation(error) => error.rate_limit_delay(),
            _ => None,
        }
    }
}

impl xmtp_mls_core::worker::NeedsDbReconnect for IdentityUpdateError {
    #[deny(clippy::wildcard_enum_match_arm)]
    fn needs_db_reconnect(&self) -> bool {
        match self {
            Self::Storage(error) => error.db_needs_connection(),
            Self::Db(error) => error.db_needs_connection(),
            Self::Identity(error) => error.needs_db_reconnect(),
            Self::Publish(error) => error.needs_db_reconnect(),
            Self::InvalidSignatureRequest(_)
            | Self::Api(_)
            | Self::Validation(_)
            | Self::Association(_)
            | Self::SignatureValidation(_)
            | Self::Conversion(_)
            | Self::AddressValidation(_)
            | Self::Handle(_)
            | Self::Blocked(_)
            | Self::InboxNotFound
            | Self::EmptyIdentityLog => false,
        }
    }
}

#[derive(Debug, Error)]
pub enum InstallationDiffError {
    #[error(transparent)]
    IdentityDependency(#[from] IdentityDependencyError),
    #[error(transparent)]
    IdentityUpdate(#[from] IdentityUpdateError),
    #[error(transparent)]
    Db(#[from] xmtp_db::ConnectionError),
    #[error(transparent)]
    Storage(#[from] StorageError),
}

impl RetryableError for InstallationDiffError {
    fn is_retryable(&self) -> bool {
        match self {
            InstallationDiffError::IdentityDependency(error) => retryable!(error),
            InstallationDiffError::IdentityUpdate(error) => retryable!(error),
            InstallationDiffError::Storage(e) => retryable!(e),
            InstallationDiffError::Db(e) => retryable!(e),
        }
    }

    fn rate_limit_delay(&self) -> Option<xmtp_common::time::Duration> {
        match self {
            InstallationDiffError::IdentityDependency(error) => error.rate_limit_delay(),
            InstallationDiffError::IdentityUpdate(error) => error.rate_limit_delay(),
            _ => None,
        }
    }
}

impl xmtp_mls_core::worker::NeedsDbReconnect for InstallationDiffError {
    #[deny(clippy::wildcard_enum_match_arm)]
    fn needs_db_reconnect(&self) -> bool {
        match self {
            InstallationDiffError::IdentityDependency(error) => error.needs_db_reconnect(),
            InstallationDiffError::IdentityUpdate(error) => error.needs_db_reconnect(),
            InstallationDiffError::Db(error) => error.db_needs_connection(),
            InstallationDiffError::Storage(error) => error.db_needs_connection(),
        }
    }
}
