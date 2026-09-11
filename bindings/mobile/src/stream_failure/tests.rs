use super::*;
use crate::FfiError;
use xmtp_mls::{
    client::ClientError,
    groups::GroupError,
    subscriptions::{
        barrier::{BarrierCause, BarrierError, BarrierFailure, BarrierTopic},
        catch_up::{CatchUpError, CatchUpSummary},
    },
};
use xmtp_proto::types::{Cursor, Topic};

fn barrier(target: Option<u64>) -> BarrierError {
    BarrierError::Incomplete {
        reason: BarrierFailure::Deadline,
        unfinished: vec![BarrierTopic {
            topic: Topic::new_welcome_message([7; 32].into()),
            target: target.map(Cursor),
            received: Cursor(u64::MAX),
            processed: Cursor(9_007_199_254_740_993),
            unresolved_welcomes: vec![Cursor(u64::MAX)],
            inactive: false,
            cause: Some(BarrierCause::TargetPending),
        }],
    }
}

#[xmtp_common::test(unwrap_try = true)]
fn flat_group_error_keeps_missing_target_and_typed_cursors() {
    let error: FfiError = GroupError::StreamBarrier(barrier(None)).into();
    let message = error.to_string();
    assert!(message.starts_with("[BarrierError::Incomplete]"));
    let details = get_stream_failure_details(message)?;
    let topic = &details.barriers[0].unfinished[0];
    assert_eq!(details.kind, FfiStreamFailureKind::Barrier);
    assert_eq!(topic.target, None);
    assert_eq!(topic.received, u64::MAX);
    assert_eq!(topic.processed, 9_007_199_254_740_993);
    assert_eq!(topic.unresolved_welcomes, vec![u64::MAX]);
    assert_eq!(
        topic.topic,
        Topic::new_welcome_message([7; 32].into()).cloned_vec()
    );
    assert_eq!(
        topic.cause.as_ref()?.kind,
        FfiStreamBarrierCauseKind::TargetPending
    );
}

#[xmtp_common::test(unwrap_try = true)]
fn flat_client_error_keeps_published_failure_and_zero_target() {
    let error: FfiError = ClientError::Group(Box::new(GroupError::PublishedButUnconfirmed {
        intent_id: 42,
        cause: Some(Box::new(barrier(Some(0)))),
    }))
    .into();
    let details = get_stream_failure_details(error.to_string())?;
    assert_eq!(details.kind, FfiStreamFailureKind::PublishedButUnconfirmed);
    assert_eq!(details.intent_id, Some(42));
    assert_eq!(details.published_intent_ids, vec![42]);
    assert_eq!(details.barriers[0].unfinished[0].target, Some(0));
}

#[xmtp_common::test(unwrap_try = true)]
fn flat_catch_up_error_keeps_all_barriers_and_partial_counts() {
    let error: FfiError = CatchUpError::Incomplete {
        summary: CatchUpSummary {
            messages: u64::MAX,
            conversations: 2,
            failed: 3,
            completed: false,
        },
        causes: vec![barrier(None), barrier(Some(0))],
    }
    .into();
    let details = get_stream_failure_details(error.to_string())?;
    let summary = details.summary?;
    assert_eq!(details.kind, FfiStreamFailureKind::CatchUp);
    assert_eq!(summary.messages, u64::MAX);
    assert_eq!(summary.conversations, 2);
    assert_eq!(summary.failed, 3);
    assert!(!summary.completed);
    assert_eq!(details.barriers.len(), 2);
}

#[xmtp_common::test(unwrap_try = true)]
fn malformed_detail_values_and_ordinary_errors_return_none() {
    assert!(get_stream_failure_details(FfiError::generic("ordinary error").to_string()).is_none());
    let error: FfiError = GroupError::StreamBarrier(barrier(None)).into();
    let message = error
        .to_string()
        .replace(&u64::MAX.to_string(), "18446744073709551616");
    assert!(get_stream_failure_details(message).is_none());
}
