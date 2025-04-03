//! an "Intent" can be thought of as a commitment by an individual 'user', which drives
//! the state of a group chat forward.
//! Examples of an Intent:
//!     - Sending a message
//!     - Adding a member
//!     - Removing a member
//!
//! Intents are written to local storage (SQLite), before being published to the delivery service via gRPC. An
//! intent is fully resolved (success or failure) once it

use thiserror::Error;
use xmtp_common::RetryableError;

#[derive(Debug, Error)]
pub enum ProcessIntentError {
    #[error("[{0}] already processed")]
    AlreadyProcessed(u64),
    #[error("storage error: {0}")]
    Storage(#[from] xmtp_db::StorageError),
}

impl RetryableError for ProcessIntentError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::AlreadyProcessed(_) => false,
            Self::Storage(err) => err.is_retryable(),
        }
    }
}
