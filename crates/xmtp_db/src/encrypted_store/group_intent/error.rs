use thiserror::Error;
use xmtp_common::{ErrorCode, Retryable};
use xmtp_proto::types::{CursorList, GroupId};

use crate::group_intent::PayloadHash;

#[derive(Debug, Error, ErrorCode, Retryable)]
#[error_code(internal)]
#[retry(default = true)]
pub enum GroupIntentError {
    /// More than one dependency.
    ///
    /// Intent has multiple dependencies in same epoch. Retryable.
    #[error(
        "intent {} for group {group_id} has invalid dependencies={}. one message cannot have more than 1 dependency in same epoch",
        hex::encode(payload_hash),
        cursors
    )]
    MoreThanOneDependency {
        payload_hash: PayloadHash,
        cursors: CursorList,
        group_id: GroupId,
    },
    /// No dependency found.
    ///
    /// Intent has no known dependencies. Retryable.
    #[error("intent with hash {hash} has no known dependencies")]
    NoDependencyFound { hash: PayloadHash },
}
