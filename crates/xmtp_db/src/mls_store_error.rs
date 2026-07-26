//! The MLS key-store error type, shared by both storage tracks.
//!
//! This lives outside the `#[cfg(feature = "sync")]`-gated `sql_key_store`
//! module so the async (sqlx/Postgres) provider — and the many `xmtp_mls` error
//! enums that `#[from]` it — can name `SqlKeyStoreError` on both tracks. The only
//! track-specific variant is the diesel one, gated to `sync`; async database
//! failures arrive through `Connection` (which carries `sqlx::Error`).

use xmtp_common::{ErrorCode, RetryableError, retryable};

/// Errors thrown by the key store.
/// General error type for Mls Storage Trait
#[derive(thiserror::Error, Debug, ErrorCode)]
pub enum SqlKeyStoreError {
    /// Unsupported value type.
    ///
    /// Key store does not allow storing serialized values. Not retryable.
    #[error("The key store does not allow storing serialized values.")]
    UnsupportedValueTypeBytes,
    /// Unsupported method.
    ///
    /// PSK operations not supported by this key store. Not retryable.
    #[error("Updating is not supported by this key store.")]
    UnsupportedMethod,
    /// Serialization error.
    ///
    /// Failed to serialize value for key store. Not retryable.
    #[error("Error serializing value.")]
    SerializationError,
    /// Value not found.
    ///
    /// Requested key not in OpenMLS key store. Not retryable.
    #[error("Value does not exist.")]
    NotFound,
    /// Database error.
    ///
    /// Underlying Diesel database error (sync track). May be retryable.
    #[cfg(feature = "sync")]
    #[error("database error: {0}")]
    Storage(#[from] diesel::result::Error),
    /// Connection error.
    ///
    /// Database connection error (carries `sqlx::Error` on the async track).
    /// Retryable.
    #[error("connection {0}")]
    Connection(#[from] crate::ConnectionError),
}

impl RetryableError for SqlKeyStoreError {
    fn is_retryable(&self) -> bool {
        use SqlKeyStoreError::*;
        match self {
            #[cfg(feature = "sync")]
            Storage(err) => retryable!(err),
            SerializationError => false,
            UnsupportedMethod => false,
            UnsupportedValueTypeBytes => false,
            NotFound => false,
            Connection(c) => retryable!(c),
        }
    }
}

impl From<bincode::Error> for SqlKeyStoreError {
    fn from(_: bincode::Error) -> Self {
        Self::SerializationError
    }
}
