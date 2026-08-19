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
use xmtp_common::Retryable;
use xmtp_proto::types::Cursor;

use crate::groups::summary::MessageIdentifier;

#[derive(Debug, Error, Retryable)]
pub enum ProcessIntentError {
    #[error("message with cursor [{}] for group [{}] already processed", _0.cursor, xmtp_common::fmt::debug_hex(_0.group_id))]
    MessageAlreadyProcessed(MessageIdentifier),
    #[error("welcome with cursor [{0}] already processed")]
    WelcomeAlreadyProcessed(Cursor),
    #[error("storage error: {0}")]
    #[retry(inherit)]
    Storage(#[from] xmtp_db::StorageError),
}
