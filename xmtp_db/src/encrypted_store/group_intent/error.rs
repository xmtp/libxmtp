use thiserror::Error;
use xmtp_common::RetryableError;
use xmtp_proto::types::{CursorList, GroupId};

use crate::group_intent::PayloadHash;

#[derive(Debug, Error)]
pub enum GroupIntentError {
    #[error(
        "intent {} for group {group_id} has invalid dependencies={}. one message cannot have more than 1 dependency in same epoch",
        hex::encode(payload_hash),
        cursors
    )]
    MoreThanTwoDependencies {
        payload_hash: PayloadHash,
        cursors: CursorList,
        group_id: GroupId,
    },
    #[error("intent with hash {hash} has no known dependencies")]
    NoDependencyFound { hash: PayloadHash },
}

impl RetryableError for GroupIntentError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::MoreThanTwoDependencies { .. } => true,
            Self::NoDependencyFound { .. } => true,
        }
    }
}
