//! The facade's stream failure reading through [`ClientError::Group`], whose
//! source skips the transparent group error.

use crate::client::ClientError;
use crate::subscriptions::barrier::{BarrierCause, BarrierError, BarrierFailure, BarrierTopic};
use crate::subscriptions::incoming::IncomingError;
use std::sync::Arc;
use xmtp_mls_engine::groups::{
    GroupError, GroupMessageProcessingError,
    summary::{ProcessSummary, SyncSummary},
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

#[xmtp_common::test(unwrap_try = true)]
fn traverses_all_sync_summary_branches_and_transparent_wrappers() {
    let mut summary = SyncSummary::other(GroupError::StreamBarrier(barrier(
        None,
        BarrierFailure::Deadline,
    )));
    summary.add_publish_err(GroupError::PublishedButUnconfirmed {
        intent_id: 17,
        cause: Some(Box::new(barrier(Some(1), BarrierFailure::Blocked))),
        rate_limit_delay: None,
    });
    summary.add_post_commit_err(GroupError::PublishedButUnconfirmed {
        intent_id: 18,
        cause: Some(Box::new(barrier(Some(2), BarrierFailure::Cancelled))),
        rate_limit_delay: None,
    });
    summary.add_process(ProcessSummary {
        errored: vec![(
            Cursor(3),
            GroupMessageProcessingError::PreparedAttempt(Box::new(GroupError::StreamBarrier(
                barrier(Some(3), BarrierFailure::Blocked),
            ))),
        )],
        ..Default::default()
    });
    let error = ClientError::Group(Box::new(GroupError::Sync(Box::new(summary))));
    let details = crate::client::stream_failure_details(&error)?;
    let targets: Vec<_> = details
        .barriers
        .iter()
        .map(|barrier| barrier.unfinished[0].target.as_deref())
        .collect();
    assert_eq!(targets, vec![None, Some("1"), Some("2"), Some("3")]);
    assert_eq!(details.published_intent_ids, vec![17, 18]);
    assert_eq!(details.intent_id, None);
}
