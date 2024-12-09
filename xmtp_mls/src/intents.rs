//! an "Intent" can be thought of as a commitment by an individual 'user', which drives
//! the state of a group chat forward.
//! Examples of an Intent:
//!     - Sending a message
//!     - Adding a member
//!     - Removing a member
//!
//! Intents are written to local storage (SQLite), before being published to the delivery service via gRPC. An
//! intent is fully resolved (success or failure) once it

use crate::retry::RetryableError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProcessIntentError {
    #[error("[{0}] already processed")]
    AlreadyProcessed(u64),
    #[error("diesel error: {0}")]
    Diesel(#[from] diesel::result::Error),
    #[error("storage error: {0}")]
    Storage(#[from] crate::storage::StorageError),
}

impl RetryableError for ProcessIntentError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::AlreadyProcessed(_) => false,
            Self::Diesel(err) => err.is_retryable(),
            Self::Storage(err) => err.is_retryable(),
        }
    }
}
