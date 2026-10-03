use super::*;
use std::sync::Arc;
use xmtp_common::ErrorCode;
use xmtp_mls::{
    groups::{GroupError, summary::SyncSummary},
    subscriptions::{
        barrier::{BarrierCause, BarrierError, BarrierFailure, BarrierTopic},
        incoming::IncomingError,
    },
};
use xmtp_proto::types::{Cursor, Topic};

fn barrier(target: Option<u64>, reason: BarrierFailure) -> BarrierError {
    BarrierError::Incomplete {
        reason,
        unfinished: vec![BarrierTopic {
            topic: Topic::new_welcome_message([0xab; 32].into()),
            scope_generation: Some(u64::MAX),
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

fn stream_failure(mut error: XmtpError) -> StreamFailureDetails {
    error
        .details_mut()
        .stream_failure
        .take()
        .expect("core processing error must keep its structured details")
}

// verifies: PROC-018
#[xmtp_common::test(unwrap_try = true)]
fn group_read_write_and_builder_keep_published_failure_details() {
    let core = || GroupError::PublishedButUnconfirmed {
        intent_id: 42,
        cause: Some(Box::new(barrier(Some(0), BarrierFailure::Cancelled))),
    };
    for error in [
        XmtpError::from_group(core()),
        XmtpError::from_group_write(core()),
        XmtpError::from_core(xmtp_mls::builder::ClientBuilderError::GroupError(Box::new(
            core(),
        ))),
    ] {
        let XmtpError::PublishedButUnconfirmed(details) = error else {
            panic!("published failure lost its recovery action");
        };
        assert_eq!(details.code, "PublishedButUnconfirmed");
        assert!(matches!(details.category, ErrorCategory::Conversation));
        assert!(!details.retryable);
        let failure = details.stream_failure?;
        assert_eq!(failure.kind, StreamFailureKind::PublishedButUnconfirmed);
        assert_eq!(failure.code, "GroupError::PublishedButUnconfirmed");
        assert_eq!(failure.message, "The published intent is not confirmed");
        assert_eq!(failure.intent_id, Some(42));
        assert_eq!(failure.published_intent_ids, vec![42]);
        assert!(failure.summary.is_none());
        assert_eq!(failure.barriers.len(), 1);
        assert_eq!(failure.barriers[0].reason, StreamBarrierReason::Cancelled);
        let topic = &failure.barriers[0].unfinished[0];
        assert_eq!(
            topic.topic,
            Topic::new_welcome_message([0xab; 32].into()).cloned_vec()
        );
        assert_eq!(topic.target, Some(0));
        assert_eq!(topic.scope_generation, Some(u64::MAX));
        assert_eq!(topic.received, u64::MAX);
        assert_eq!(topic.processed, u64::MAX - 1);
        assert_eq!(
            topic.unresolved_welcomes,
            vec![9_007_199_254_740_993, u64::MAX]
        );
        assert!(!topic.inactive);
        assert_eq!(
            topic.cause,
            Some(StreamBarrierCause {
                kind: StreamBarrierCauseKind::Receiver,
                code: Some("unsupported_topic".into()),
                message: "The receiver failed".into(),
                retryable: false,
            })
        );
    }
    let no_barrier = stream_failure(XmtpError::from_group(GroupError::PublishedButUnconfirmed {
        intent_id: 17,
        cause: None,
    }));
    assert_eq!(no_barrier.intent_id, Some(17));
    assert_eq!(no_barrier.published_intent_ids, vec![17]);
    assert!(no_barrier.barriers.is_empty());
    assert!(XmtpError::closed().details_mut().stream_failure.is_none());
}

// verifies: PROC-018
#[xmtp_common::test(unwrap_try = true)]
fn projection_keeps_all_summary_branches_and_published_intents() {
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
    let error = xmtp_mls::client::ClientError::Group(Box::new(GroupError::Sync(Box::new(summary))));
    let failure = stream_failure(XmtpError::from_core(error));
    assert_eq!(failure.intent_id, None);
    assert_eq!(failure.published_intent_ids, vec![17, 18]);
    assert_eq!(failure.barriers.len(), 3);
    assert_eq!(
        failure
            .barriers
            .iter()
            .map(|barrier| barrier.reason)
            .collect::<Vec<_>>(),
        vec![
            StreamBarrierReason::Deadline,
            StreamBarrierReason::Blocked,
            StreamBarrierReason::Cancelled,
        ]
    );
    assert_eq!(
        failure
            .barriers
            .iter()
            .map(|barrier| barrier.unfinished[0].target)
            .collect::<Vec<_>>(),
        vec![None, Some(1), Some(2)]
    );
}

// verifies: PROC-018
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
fn catch_up_projection_keeps_committed_counts_and_all_barriers() {
    use xmtp_mls::subscriptions::catch_up::{CatchUpError, CatchUpSummary};
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
    let failure = stream_failure(XmtpError::from_core(error));
    assert_eq!(failure.kind, StreamFailureKind::CatchUp);
    assert_eq!(failure.code, "CatchUpError::Incomplete");
    assert_eq!(failure.message, "Catch-up did not complete");
    assert!(failure.retryable);
    assert_eq!(
        failure.summary,
        Some(StreamFailureSummary {
            messages: u64::MAX,
            conversations: 9_007_199_254_740_993,
            failed: 2,
            completed: false,
        })
    );
    assert_eq!(failure.barriers.len(), 2);
    assert_eq!(failure.barriers[0].unfinished[0].target, None);
    assert_eq!(failure.barriers[1].unfinished[0].target, Some(0));
}

// verifies: PROC-018
#[xmtp_common::test(unwrap_try = true)]
fn barrier_projection_keeps_every_cause_and_topic() {
    let storage = xmtp_db::StorageError::PreTransitionDatabase;
    let storage_code = storage.error_code().to_string();
    let causes = vec![
        (
            BarrierCause::TargetPending,
            StreamBarrierCauseKind::TargetPending,
            None,
            "Target capture is pending",
            true,
        ),
        (
            BarrierCause::ReceiptPending,
            StreamBarrierCauseKind::ReceiptPending,
            None,
            "Receipt is pending",
            true,
        ),
        (
            BarrierCause::ProcessingPending,
            StreamBarrierCauseKind::ProcessingPending,
            None,
            "Processing is pending",
            true,
        ),
        (
            BarrierCause::Blocked("blocked-code".into()),
            StreamBarrierCauseKind::Blocked,
            Some("blocked-code".to_owned()),
            "Processing is blocked",
            false,
        ),
        (
            BarrierCause::Storage(Arc::new(storage)),
            StreamBarrierCauseKind::Storage,
            Some(storage_code),
            "Storage failed",
            false,
        ),
        (
            BarrierCause::Receiver(Arc::new(IncomingError::UnsupportedTopic)),
            StreamBarrierCauseKind::Receiver,
            Some("unsupported_topic".to_owned()),
            "The receiver failed",
            false,
        ),
        (
            BarrierCause::InvalidTopic,
            StreamBarrierCauseKind::InvalidTopic,
            None,
            "The topic is invalid",
            false,
        ),
    ];
    let expected: Vec<_> = causes
        .iter()
        .map(|(_, kind, code, message, retryable)| StreamBarrierCause {
            kind: *kind,
            code: code.clone(),
            message: (*message).into(),
            retryable: *retryable,
        })
        .collect();
    let unfinished = causes
        .into_iter()
        .enumerate()
        .map(|(index, (cause, ..))| BarrierTopic {
            topic: Topic::new_welcome_message([index as u8; 32].into()),
            scope_generation: (index != 0).then_some(u64::MAX - index as u64),
            target: (index != 0).then_some(Cursor(u64::MAX)),
            received: Cursor(u64::MAX - 1),
            processed: Cursor(u64::MAX - 2),
            unresolved_welcomes: vec![Cursor(9_007_199_254_740_993), Cursor(u64::MAX)],
            inactive: index % 2 == 0,
            cause: Some(cause),
        })
        .collect();
    let failure = stream_failure(XmtpError::from_group(GroupError::StreamBarrier(
        BarrierError::Incomplete {
            reason: BarrierFailure::Blocked,
            unfinished,
        },
    )));
    assert_eq!(failure.kind, StreamFailureKind::Barrier);
    assert_eq!(failure.barriers.len(), 1);
    assert_eq!(failure.barriers[0].unfinished.len(), expected.len());
    for (index, (topic, cause)) in failure.barriers[0]
        .unfinished
        .iter()
        .zip(expected)
        .enumerate()
    {
        assert_eq!(
            topic.topic,
            Topic::new_welcome_message([index as u8; 32].into()).cloned_vec()
        );
        assert_eq!(topic.target, (index != 0).then_some(u64::MAX));
        assert_eq!(
            topic.scope_generation,
            (index != 0).then_some(u64::MAX - index as u64)
        );
        assert_eq!(topic.received, u64::MAX - 1);
        assert_eq!(topic.processed, u64::MAX - 2);
        assert_eq!(
            topic.unresolved_welcomes,
            vec![9_007_199_254_740_993, u64::MAX]
        );
        assert_eq!(topic.inactive, index % 2 == 0);
        assert_eq!(topic.cause.as_ref(), Some(&cause));
    }
}

#[xmtp_common::test(unwrap_try = true)]
fn invalid_wire_values_do_not_drop_one_topic_or_make_a_cursor() {
    let core = GroupError::StreamBarrier(barrier(Some(0), BarrierFailure::Deadline));
    let valid = wire::stream_failure_details(&core)?;
    let mut invalid = valid.clone();
    invalid.barriers[0].unfinished[0].topic = "not-hex".into();
    assert!(StreamFailureDetails::from_wire(invalid).is_err());
    let mut invalid = valid.clone();
    invalid.barriers[0].unfinished[0].received = "18446744073709551616".into();
    assert!(StreamFailureDetails::from_wire(invalid).is_err());
    let mut invalid = valid.clone();
    invalid.barriers[0].unfinished[0].scope_generation = Some("18446744073709551616".into());
    assert!(StreamFailureDetails::from_wire(invalid).is_err());
    let mut invalid = valid;
    invalid.barriers[0].unfinished[0]
        .unresolved_welcomes
        .push("-1".into());
    assert!(StreamFailureDetails::from_wire(invalid).is_err());
}

// verifies: PROC-018
#[xmtp_common::test(unwrap_try = true)]
fn failure_before_admission_has_no_scope_generation() {
    let mut error = barrier(None, BarrierFailure::Deadline);
    let BarrierError::Incomplete { unfinished, .. } = &mut error;
    unfinished[0].scope_generation = None;
    let failure = stream_failure(XmtpError::from_group(GroupError::StreamBarrier(error)));
    let topic = &failure.barriers[0].unfinished[0];
    assert_eq!(topic.scope_generation, None);
    assert_eq!(topic.target, None);
    assert_eq!(topic.received, u64::MAX);
}
