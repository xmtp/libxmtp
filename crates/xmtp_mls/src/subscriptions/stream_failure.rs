//! Structured processing failures shared by string-based binding errors.

use std::error::Error;

use serde::{Deserialize, Serialize};
use xmtp_common::{ErrorCode, RetryableError};

#[cfg(not(target_arch = "wasm32"))]
use super::catch_up::CatchUpError;
use super::{
    SubscribeError,
    barrier::{BarrierCause, BarrierError, BarrierFailure, BarrierTopic},
    incoming::IncomingError,
};
use crate::{
    client::ClientError,
    groups::{GroupError, mls_sync::GroupMessageProcessingError, summary::SyncSummary},
};

/// The suffix version is independent of the normal error message and code.
pub const STREAM_FAILURE_MARKER: &str = "\n[XMTP_STREAM_FAILURE_V1]";

/// The operation whose fixed processing obligations remain unfinished.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamFailureKind {
    Barrier,
    PublishedButUnconfirmed,
    CatchUp,
}

/// Why the barrier stopped waiting. Durable pending work is retained.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamBarrierReason {
    Blocked,
    Deadline,
    Cancelled,
}

/// Stable categories for one unfinished topic's cause.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamBarrierCauseKind {
    TargetPending,
    ReceiptPending,
    ProcessingPending,
    Blocked,
    Storage,
    Receiver,
    InvalidTopic,
}

/// Safe cause metadata. Messages do not contain raw nested error data.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamBarrierCause {
    pub kind: StreamBarrierCauseKind,
    /// Stable source code when the cause has one.
    pub code: Option<String>,
    /// Fixed category text for display, not for control flow.
    pub message: String,
    pub retryable: bool,
}

/// All 64-bit values are decimal strings. Topic bytes use hexadecimal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamBarrierTopic {
    /// Complete topic bytes, encoded as hexadecimal.
    pub topic: String,
    /// Fixed target H. None means capture failed; "0" is a captured empty target.
    pub target: Option<String>,
    /// Durable receipt cursor F. This is not application delivery progress.
    pub received: String,
    /// Resolved processing cursor P, independent of local delivery acknowledgement D.
    pub processed: String,
    /// Exact unresolved Welcome sequence IDs at or below H.
    pub unresolved_welcomes: Vec<String>,
    pub inactive: bool,
    pub cause: Option<StreamBarrierCause>,
}

/// One failed barrier and every topic obligation that it did not complete.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamBarrierFailure {
    pub reason: StreamBarrierReason,
    /// Keep all entries, including sibling failures from the same sync summary.
    pub unfinished: Vec<StreamBarrierTopic>,
}

/// Partial committed catch-up counts, encoded as exact decimal u64 strings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamFailureSummary {
    pub messages: String,
    pub conversations: String,
    pub failed: String,
    pub completed: bool,
}

/// Versioned error details shared by all string-based binding error carriers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamFailureDetails {
    pub kind: StreamFailureKind,
    pub code: String,
    pub message: String,
    pub retryable: bool,
    /// The sole published intent, if there is exactly one.
    pub intent_id: Option<i32>,
    /// All published intent IDs from nested errors, without duplicates.
    pub published_intent_ids: Vec<i32>,
    /// Committed catch-up progress when the error came from a catch-up run.
    pub summary: Option<StreamFailureSummary>,
    /// All failed barriers, not only the first source in an error chain.
    pub barriers: Vec<StreamBarrierFailure>,
}

impl From<&BarrierCause> for StreamBarrierCause {
    fn from(cause: &BarrierCause) -> Self {
        use StreamBarrierCauseKind as Kind;
        let (kind, code, message, retryable) = match cause {
            BarrierCause::TargetPending => {
                (Kind::TargetPending, None, "Target capture is pending", true)
            }
            BarrierCause::ReceiptPending => {
                (Kind::ReceiptPending, None, "Receipt is pending", true)
            }
            BarrierCause::ProcessingPending => {
                (Kind::ProcessingPending, None, "Processing is pending", true)
            }
            BarrierCause::Blocked(code) => (
                Kind::Blocked,
                Some(code.clone()),
                "Processing is blocked",
                false,
            ),
            BarrierCause::Storage(error) => (
                Kind::Storage,
                Some(error.error_code().to_string()),
                "Storage failed",
                error.is_retryable(),
            ),
            BarrierCause::Receiver(error) => (
                Kind::Receiver,
                Some(error.code().to_owned()),
                "The receiver failed",
                error.is_retryable(),
            ),
            BarrierCause::InvalidTopic => (Kind::InvalidTopic, None, "The topic is invalid", false),
        };
        Self {
            kind,
            code,
            message: message.to_owned(),
            retryable,
        }
    }
}

impl From<&BarrierTopic> for StreamBarrierTopic {
    fn from(topic: &BarrierTopic) -> Self {
        Self {
            topic: hex::encode(topic.topic.cloned_vec()),
            target: topic.target.map(|cursor| cursor.0.to_string()),
            received: topic.received.0.to_string(),
            processed: topic.processed.0.to_string(),
            unresolved_welcomes: topic
                .unresolved_welcomes
                .iter()
                .map(|cursor| cursor.0.to_string())
                .collect(),
            inactive: topic.inactive,
            cause: topic.cause.as_ref().map(Into::into),
        }
    }
}

impl From<&BarrierError> for StreamBarrierFailure {
    fn from(error: &BarrierError) -> Self {
        let BarrierError::Incomplete { reason, unfinished } = error;
        Self {
            reason: match reason {
                BarrierFailure::Blocked => StreamBarrierReason::Blocked,
                BarrierFailure::Deadline => StreamBarrierReason::Deadline,
                BarrierFailure::Cancelled => StreamBarrierReason::Cancelled,
            },
            unfinished: unfinished.iter().map(Into::into).collect(),
        }
    }
}

impl StreamFailureDetails {
    fn barrier(error: &BarrierError) -> Self {
        Self {
            kind: StreamFailureKind::Barrier,
            code: error.error_code().to_string(),
            message: "Processing barriers did not complete".into(),
            retryable: error.is_retryable(),
            intent_id: None,
            published_intent_ids: Vec::new(),
            summary: None,
            barriers: vec![error.into()],
        }
    }
}

/// Read known wrapper fields explicitly. Transparent errors can skip them in `source()`.
/// Visit every summary branch so a first error cannot hide sibling obligations.
pub fn stream_failure_details(error: &(dyn Error + 'static)) -> Option<StreamFailureDetails> {
    let mut pending = vec![(error, 0usize)];
    let mut failures = Vec::new();
    while let Some((error, depth)) = pending.pop() {
        if depth >= 64 {
            continue;
        }
        let mut push = |error| pending.push((error, depth + 1));
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(error) = error.downcast_ref::<CatchUpError>() {
            match error {
                CatchUpError::Group(group) => push(group),
                CatchUpError::Incomplete { summary, causes } => {
                    failures.push(StreamFailureDetails {
                        kind: StreamFailureKind::CatchUp,
                        code: error.error_code().to_string(),
                        message: "Catch-up did not complete".into(),
                        retryable: error.is_retryable(),
                        intent_id: None,
                        published_intent_ids: Vec::new(),
                        summary: Some(StreamFailureSummary {
                            messages: summary.messages.to_string(),
                            conversations: summary.conversations.to_string(),
                            failed: summary.failed.to_string(),
                            completed: summary.completed,
                        }),
                        barriers: causes.iter().map(Into::into).collect(),
                    })
                }
            }
            continue;
        }
        if let Some(error) = error.downcast_ref::<GroupError>() {
            match error {
                GroupError::StreamBarrier(error) => {
                    failures.push(StreamFailureDetails::barrier(error))
                }
                GroupError::PublishedButUnconfirmed { intent_id, cause } => {
                    failures.push(StreamFailureDetails {
                        kind: StreamFailureKind::PublishedButUnconfirmed,
                        code: error.error_code().to_string(),
                        message: "The published intent is not confirmed".into(),
                        retryable: error.is_retryable(),
                        intent_id: Some(*intent_id),
                        published_intent_ids: vec![*intent_id],
                        summary: None,
                        barriers: cause.iter().map(|cause| cause.as_ref().into()).collect(),
                    })
                }
                GroupError::Sync(summary) | GroupError::SyncFailedToWait(summary) => {
                    push(summary.as_ref())
                }
                GroupError::Client(error) => push(error),
                _ => {
                    if let Some(source) = error.source() {
                        push(source);
                    }
                }
            }
            continue;
        }
        if let Some(error) = error.downcast_ref::<BarrierError>() {
            failures.push(StreamFailureDetails::barrier(error));
            continue;
        }
        if let Some(summary) = error.downcast_ref::<SyncSummary>() {
            for (_, error) in summary.process.errored.iter().rev() {
                push(error);
            }
            for error in summary.post_commit_errors.iter().rev() {
                push(error);
            }
            for error in summary.publish_errors.iter().rev() {
                push(error);
            }
            if let Some(error) = summary.other.as_deref() {
                push(error);
            }
            continue;
        }
        if let Some(ClientError::Group(error)) = error.downcast_ref::<ClientError>() {
            push(error.as_ref());
        } else if let Some(SubscribeError::Group(error)) = error.downcast_ref::<SubscribeError>() {
            push(error.as_ref());
        } else if let Some(IncomingError::Group(error)) = error.downcast_ref::<IncomingError>() {
            push(error);
        } else if let Some(error) = error.downcast_ref::<GroupMessageProcessingError>() {
            match error {
                GroupMessageProcessingError::PreparedAttempt(error) => push(error.as_ref()),
                GroupMessageProcessingError::Client(error) => push(error),
                _ => {
                    if let Some(source) = error.source() {
                        push(source);
                    }
                }
            }
        } else if let Some(source) = error.source() {
            push(source);
        }
    }
    let mut failures = failures.into_iter();
    let mut result = failures.next()?;
    for failure in failures {
        result.retryable |= failure.retryable;
        result.barriers.extend(failure.barriers);
        result
            .published_intent_ids
            .extend(failure.published_intent_ids);
    }
    result.published_intent_ids.sort_unstable();
    result.published_intent_ids.dedup();
    result.intent_id = match result.published_intent_ids.as_slice() {
        [id] => Some(*id),
        _ => None,
    };
    Some(result)
}

/// Append this suffix after the existing error code and message.
pub fn encode_stream_failure(error: &(dyn Error + 'static)) -> Option<String> {
    let details = stream_failure_details(error)?;
    let json = serde_json::to_string(&details).ok()?;
    Some(format!("{STREAM_FAILURE_MARKER}{json}"))
}

/// Decode the versioned suffix. Ordinary errors have no structured details.
pub fn decode_stream_failure(message: &str) -> Option<StreamFailureDetails> {
    let (_, json) = message.rsplit_once(STREAM_FAILURE_MARKER)?;
    serde_json::from_str(json).ok()
}

#[cfg(test)]
mod tests;
