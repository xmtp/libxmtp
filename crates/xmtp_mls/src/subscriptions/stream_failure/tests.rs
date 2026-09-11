use super::*;
use std::sync::Arc;
use xmtp_proto::types::{Cursor, Topic};

fn barrier(target: Option<u64>, reason: BarrierFailure) -> BarrierError {
    BarrierError::Incomplete {
        reason,
        unfinished: vec![BarrierTopic {
            topic: Topic::new_welcome_message([0xab; 32].into()),
            target: target.map(Cursor),
            received: Cursor(u64::MAX),
            processed: Cursor(u64::MAX - 1),
            unresolved_welcomes: vec![Cursor(9_007_199_254_740_993), Cursor(u64::MAX)],
            inactive: false,
            cause: Some(BarrierCause::Receiver(Arc::new(
                IncomingError::UnsupportedTopic,
            ))),
        }],
    }
}

#[xmtp_common::test(unwrap_try = true)]
fn preserves_missing_targets_and_full_width_cursors() {
    let error = GroupError::StreamBarrier(barrier(None, BarrierFailure::Deadline));
    let suffix = encode_stream_failure(&error)?;
    let json: serde_json::Value =
        serde_json::from_str(suffix.strip_prefix(STREAM_FAILURE_MARKER)?)?;
    let topic = &json["barriers"][0]["unfinished"][0];
    assert!(topic["target"].is_null());
    assert_eq!(topic["received"], u64::MAX.to_string());
    assert_eq!(topic["processed"], (u64::MAX - 1).to_string());
    assert_eq!(topic["unresolvedWelcomes"][0], "9007199254740993");
    assert_eq!(topic["unresolvedWelcomes"][1], u64::MAX.to_string());
    let details = decode_stream_failure(&format!("[code] message{suffix}"))?;
    assert_eq!(details.barriers[0].unfinished[0].target, None);
    assert_eq!(
        details.barriers[0].unfinished[0]
            .cause
            .as_ref()?
            .code
            .as_deref(),
        Some("unsupported_topic")
    );
}

#[xmtp_common::test(unwrap_try = true)]
fn published_failure_keeps_zero_target_and_intent_identity() {
    let error = GroupError::PublishedButUnconfirmed {
        intent_id: 42,
        cause: Some(Box::new(barrier(Some(0), BarrierFailure::Cancelled))),
    };
    let details = decode_stream_failure(&encode_stream_failure(&error)?)?;
    assert_eq!(details.kind, StreamFailureKind::PublishedButUnconfirmed);
    assert_eq!(details.intent_id, Some(42));
    assert_eq!(details.published_intent_ids, vec![42]);
    assert_eq!(details.barriers[0].reason, StreamBarrierReason::Cancelled);
    assert_eq!(
        details.barriers[0].unfinished[0].target.as_deref(),
        Some("0")
    );
}

#[xmtp_common::test(unwrap_try = true)]
fn published_failure_without_barrier_keeps_intent_identity() {
    let error = GroupError::PublishedButUnconfirmed {
        intent_id: 17,
        cause: None,
    };
    let details = stream_failure_details(&error)?;
    assert_eq!(details.intent_id, Some(17));
    assert!(details.barriers.is_empty());
}

#[xmtp_common::test(unwrap_try = true)]
fn traverses_all_sync_summary_branches_and_transparent_wrappers() {
    let mut summary = SyncSummary::other(GroupError::StreamBarrier(barrier(
        None,
        BarrierFailure::Deadline,
    )));
    summary.add_publish_err(GroupError::PublishedButUnconfirmed {
        intent_id: 17,
        cause: Some(Box::new(barrier(Some(1), BarrierFailure::Blocked))),
    });
    summary.add_post_commit_err(GroupError::PublishedButUnconfirmed {
        intent_id: 18,
        cause: Some(Box::new(barrier(Some(2), BarrierFailure::Cancelled))),
    });
    summary.process.errored.push((
        Cursor(3),
        GroupMessageProcessingError::PreparedAttempt(Box::new(GroupError::StreamBarrier(barrier(
            Some(3),
            BarrierFailure::Blocked,
        )))),
    ));
    let error = ClientError::Group(Box::new(GroupError::Sync(Box::new(summary))));
    let details = stream_failure_details(&error)?;
    let targets: Vec<_> = details
        .barriers
        .iter()
        .map(|barrier| barrier.unfinished[0].target.as_deref())
        .collect();
    assert_eq!(targets, vec![None, Some("1"), Some("2"), Some("3")]);
    assert_eq!(details.published_intent_ids, vec![17, 18]);
    assert_eq!(details.intent_id, None);
}

#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
fn catch_up_failure_keeps_partial_summary_and_all_barriers() {
    use crate::subscriptions::catch_up::CatchUpSummary;
    let error = CatchUpError::Incomplete {
        summary: CatchUpSummary {
            messages: u64::MAX,
            conversations: 9_007_199_254_740_993,
            failed: 2,
            completed: false,
        },
        causes: vec![
            barrier(None, BarrierFailure::Deadline),
            barrier(Some(0), BarrierFailure::Blocked),
        ],
    };
    let details = decode_stream_failure(&encode_stream_failure(&error)?)?;
    let summary = details.summary?;
    assert_eq!(details.kind, StreamFailureKind::CatchUp);
    assert_eq!(summary.messages, u64::MAX.to_string());
    assert_eq!(summary.conversations, "9007199254740993");
    assert_eq!(summary.failed, "2");
    assert!(!summary.completed);
    assert_eq!(details.barriers.len(), 2);
}

#[xmtp_common::test(unwrap_try = true)]
fn details_do_not_include_storage_error_data() {
    let mut error = barrier(None, BarrierFailure::Blocked);
    let BarrierError::Incomplete { unfinished, .. } = &mut error;
    unfinished[0].cause = Some(BarrierCause::Storage(Arc::new(
        xmtp_db::StorageError::NotFound(xmtp_db::NotFound::MessageById(
            b"private-message-data".to_vec(),
        )),
    )));
    let details = stream_failure_details(&error)?;
    let cause = details.barriers[0].unfinished[0].cause.as_ref()?;
    assert_eq!(cause.code.as_deref(), Some("StorageError::NotFound"));
    assert_eq!(cause.message, "Storage failed");
}

#[xmtp_common::test(unwrap_try = true)]
fn ordinary_and_invalid_errors_have_no_details() {
    assert!(encode_stream_failure(&GroupError::GroupInactive).is_none());
    assert!(decode_stream_failure("[GenericError::Generic] ordinary error").is_none());
    assert!(decode_stream_failure(&format!("{STREAM_FAILURE_MARKER}invalid-json")).is_none());
}
