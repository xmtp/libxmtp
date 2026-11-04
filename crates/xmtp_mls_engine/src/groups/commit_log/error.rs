//! Commit log errors and their retry and reconnect classification.

use super::*;

#[derive(Debug, Error)]
pub enum CommitLogError {
    #[error("generic storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("diesel error: {0}")]
    Diesel(#[from] xmtp_db::diesel::result::Error),
    #[error("generic api error: {0}")]
    Api(#[from] ApiError),
    #[error("connection error: {0}")]
    Connection(#[from] xmtp_db::ConnectionError),
    #[error("prost decode error: {0}")]
    Prost(#[from] prost::DecodeError),
    #[error("keystore error: {0}")]
    KeystoreError(#[from] xmtp_db::sql_key_store::SqlKeyStoreError),
    #[error("group error: {0}")]
    GroupError(#[from] GroupError),
    #[error("crypto error: {0}")]
    CryptoError(#[from] openmls_traits::types::CryptoError),
    #[error("try from slice error: {0}")]
    TryFromSliceError(#[from] std::array::TryFromSliceError),
    #[error("conversion error: {0}")]
    Conversion(#[from] xmtp_proto::ConversionError),
    #[error("Group did not pass readd validation: {0}")]
    GroupReaddValidationError(String),
    #[error("sync error: {0}")]
    SyncError(#[from] SyncSummary),
    #[error("failed to send readd request for group {group_id}: {source}")]
    FailedToSendReadd {
        group_id: GroupId,
        source: Box<CommitLogError>,
    },
    #[error("{count} readd request(s) failed: {errors:?}", count = errors.len())]
    FailedReadds { errors: Vec<CommitLogError> },
    #[error("no latest commit sequence id found for forked group {group_id}")]
    MissingLatestCommitSequenceId { group_id: GroupId },
}

impl RetryableError for CommitLogError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::Storage(storage_error) => storage_error.is_retryable(),
            Self::Diesel(diesel_error) => diesel_error.is_retryable(),
            Self::Api(api_error) => api_error.is_retryable(),
            Self::Connection(connection_error) => connection_error.is_retryable(),
            Self::Prost(_prost_error) => false,
            Self::KeystoreError(keystore_error) => keystore_error.is_retryable(),
            Self::GroupError(group_error) => group_error.is_retryable(),
            Self::CryptoError(_crypto_error) => false,
            Self::TryFromSliceError(_try_from_slice_error) => false,
            Self::Conversion(_) => false,
            Self::GroupReaddValidationError(_group_readd_validation_error) => false,
            Self::SyncError(sync_error) => sync_error.is_retryable(),
            Self::FailedToSendReadd { source, .. } => source.is_retryable(),
            Self::FailedReadds { errors } => errors.iter().any(|e| e.is_retryable()),
            Self::MissingLatestCommitSequenceId { .. } => false,
        }
    }

    fn rate_limit_delay(&self) -> Option<xmtp_common::time::Duration> {
        match self {
            Self::Api(api_error) => api_error.rate_limit_delay(),
            Self::GroupError(group_error) => group_error.rate_limit_delay(),
            Self::SyncError(sync_error) => sync_error.rate_limit_delay(),
            Self::FailedToSendReadd { source, .. } => source.rate_limit_delay(),
            Self::FailedReadds { errors } => errors
                .iter()
                .filter_map(RetryableError::rate_limit_delay)
                .max(),
            _ => None,
        }
    }
}

impl NeedsDbReconnect for CommitLogError {
    fn restart_delay(&self) -> Option<Duration> {
        RetryableError::rate_limit_delay(self)
    }

    #[deny(clippy::wildcard_enum_match_arm)]
    fn needs_db_reconnect(&self) -> bool {
        match self {
            Self::Storage(s) => s.db_needs_connection(),
            // Only a `PoolNeedsConnection` connection error means the pool is gone.
            Self::Connection(c) => c.db_needs_connection(),
            // A dropped pool can be wrapped inside a GroupError; forward.
            Self::GroupError(e) => e.needs_db_reconnect(),
            // A readd send failure wraps an inner CommitLogError; forward.
            Self::FailedToSendReadd { source, .. } => source.needs_db_reconnect(),
            Self::FailedReadds { errors } => errors.iter().any(|e| e.needs_db_reconnect()),
            Self::SyncError(summary) => summary.needs_db_reconnect(),
            // Remaining variants can't carry a dropped-pool signal.
            Self::Diesel(_)
            | Self::Api(_)
            | Self::Prost(_)
            | Self::KeystoreError(_)
            | Self::CryptoError(_)
            | Self::TryFromSliceError(_)
            | Self::Conversion(_)
            | Self::GroupReaddValidationError(_)
            | Self::MissingLatestCommitSequenceId { .. } => false,
        }
    }
}
