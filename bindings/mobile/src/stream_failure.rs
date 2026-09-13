//! Typed details for processing failures carried by flat UniFFI errors.

use xmtp_mls::subscriptions::stream_failure as wire;

use crate::{FfiCatchUpSummary, GenericError};

/// The operation whose processing obligations remain unfinished.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum FfiStreamFailureKind {
    Barrier,
    PublishedButUnconfirmed,
    CatchUp,
}

/// Why the barrier stopped. Its pending work remains durable.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum FfiStreamBarrierReason {
    Blocked,
    Deadline,
    Cancelled,
}

/// Stable categories for one unfinished topic's cause.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum FfiStreamBarrierCauseKind {
    TargetPending,
    ReceiptPending,
    ProcessingPending,
    Blocked,
    Storage,
    Receiver,
    InvalidTopic,
}

/// Cause codes and fixed display text, without raw nested error data.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct FfiStreamBarrierCause {
    pub kind: FfiStreamBarrierCauseKind,
    pub code: Option<String>,
    pub message: String,
    pub retryable: bool,
}

/// One fixed topic obligation. Every cursor keeps its exact unsigned 64-bit value.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct FfiStreamBarrierTopic {
    /// Complete encoded topic bytes, including its kind.
    pub topic: Vec<u8>,
    /// None means target capture failed. Some(0) is a captured empty target.
    pub target: Option<u64>,
    /// Durable receipt cursor F, not application delivery progress.
    pub received: u64,
    /// Resolved processing cursor P, independent of local acknowledgement D.
    pub processed: u64,
    /// Exact Welcome sequence IDs at or below the target that remain unresolved.
    pub unresolved_welcomes: Vec<u64>,
    pub inactive: bool,
    pub cause: Option<FfiStreamBarrierCause>,
}

/// One failed barrier and all its unfinished topic obligations.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct FfiStreamBarrierFailure {
    pub reason: FfiStreamBarrierReason,
    pub unfinished: Vec<FfiStreamBarrierTopic>,
}

/// Typed details decoded from an unchanged flat UniFFI error.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct FfiStreamFailureDetails {
    pub kind: FfiStreamFailureKind,
    pub code: String,
    pub message: String,
    pub retryable: bool,
    /// The sole published intent, if exactly one appears in the error.
    pub intent_id: Option<i32>,
    /// All published intent IDs, including sibling sync-summary errors.
    pub published_intent_ids: Vec<i32>,
    /// Committed partial catch-up counts, when available.
    pub summary: Option<FfiCatchUpSummary>,
    /// All failed barriers. Reading details does not acknowledge any message.
    pub barriers: Vec<FfiStreamBarrierFailure>,
}

impl From<wire::StreamBarrierCause> for FfiStreamBarrierCause {
    fn from(cause: wire::StreamBarrierCause) -> Self {
        use wire::StreamBarrierCauseKind as Kind;
        Self {
            kind: match cause.kind {
                Kind::TargetPending => FfiStreamBarrierCauseKind::TargetPending,
                Kind::ReceiptPending => FfiStreamBarrierCauseKind::ReceiptPending,
                Kind::ProcessingPending => FfiStreamBarrierCauseKind::ProcessingPending,
                Kind::Blocked => FfiStreamBarrierCauseKind::Blocked,
                Kind::Storage => FfiStreamBarrierCauseKind::Storage,
                Kind::Receiver => FfiStreamBarrierCauseKind::Receiver,
                Kind::InvalidTopic => FfiStreamBarrierCauseKind::InvalidTopic,
            },
            code: cause.code,
            message: cause.message,
            retryable: cause.retryable,
        }
    }
}

impl FfiStreamBarrierTopic {
    fn from_wire(topic: wire::StreamBarrierTopic) -> Option<Self> {
        Some(Self {
            topic: hex::decode(topic.topic).ok()?,
            target: topic.target.map(|cursor| cursor.parse()).transpose().ok()?,
            received: topic.received.parse().ok()?,
            processed: topic.processed.parse().ok()?,
            unresolved_welcomes: topic
                .unresolved_welcomes
                .into_iter()
                .map(|cursor| cursor.parse().ok())
                .collect::<Option<Vec<_>>>()?,
            inactive: topic.inactive,
            cause: topic.cause.map(Into::into),
        })
    }
}

impl FfiStreamBarrierFailure {
    fn from_wire(failure: wire::StreamBarrierFailure) -> Option<Self> {
        Some(Self {
            reason: match failure.reason {
                wire::StreamBarrierReason::Blocked => FfiStreamBarrierReason::Blocked,
                wire::StreamBarrierReason::Deadline => FfiStreamBarrierReason::Deadline,
                wire::StreamBarrierReason::Cancelled => FfiStreamBarrierReason::Cancelled,
            },
            unfinished: failure
                .unfinished
                .into_iter()
                .map(FfiStreamBarrierTopic::from_wire)
                .collect::<Option<Vec<_>>>()?,
        })
    }
}

impl FfiStreamFailureDetails {
    fn from_wire(details: wire::StreamFailureDetails) -> Option<Self> {
        Some(Self {
            kind: match details.kind {
                wire::StreamFailureKind::Barrier => FfiStreamFailureKind::Barrier,
                wire::StreamFailureKind::PublishedButUnconfirmed => {
                    FfiStreamFailureKind::PublishedButUnconfirmed
                }
                wire::StreamFailureKind::CatchUp => FfiStreamFailureKind::CatchUp,
            },
            code: details.code,
            message: details.message,
            retryable: details.retryable,
            intent_id: details.intent_id,
            published_intent_ids: details.published_intent_ids,
            summary: match details.summary {
                Some(summary) => Some(FfiCatchUpSummary {
                    messages: summary.messages.parse().ok()?,
                    conversations: summary.conversations.parse().ok()?,
                    failed: summary.failed.parse().ok()?,
                    completed: summary.completed,
                }),
                None => None,
            },
            barriers: details
                .barriers
                .into_iter()
                .map(FfiStreamBarrierFailure::from_wire)
                .collect::<Option<Vec<_>>>()?,
        })
    }
}

/// Read typed processing details from an error message. Other errors return None.
#[uniffi::export]
pub fn get_stream_failure_details(error_message: String) -> Option<FfiStreamFailureDetails> {
    wire::decode_stream_failure(&error_message).and_then(FfiStreamFailureDetails::from_wire)
}

pub(crate) fn encode_error(error: &GenericError) -> Option<String> {
    // Transparent wrappers omit their direct child from Error::source().
    match error {
        GenericError::GroupError(error) => wire::encode_stream_failure(error),
        GenericError::CatchUp(error) => wire::encode_stream_failure(error),
        GenericError::Client(error) => wire::encode_stream_failure(error),
        GenericError::Subscription(error) => wire::encode_stream_failure(error),
        _ => wire::encode_stream_failure(error),
    }
}

#[cfg(test)]
mod tests;
