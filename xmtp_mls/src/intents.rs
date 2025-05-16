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

use crate::groups::summary::MessageIdentifier;

#[derive(Debug, Error)]
pub enum ProcessIntentError {
    #[error("message with cursor [{}] for group [{}] already processed", _0.cursor, xmtp_common::fmt::debug_hex(&_0.group_id))]
    MessageAlreadyProcessed(MessageIdentifier),
    #[error("welcome with cursor [{0}] already processed")]
    WelcomeAlreadyProcessed(u64),
    #[error("storage error: {0}")]
    Storage(#[from] xmtp_db::StorageError),
}

impl RetryableError for ProcessIntentError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::MessageAlreadyProcessed(_) => false,
            Self::WelcomeAlreadyProcessed(_) => false,
            Self::Storage(err) => err.is_retryable(),
        }
    }
}
