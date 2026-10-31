//! Test-only observations for convergence and liveness checks.
//!
//! IDs and public MLS state use hex so reports can cross a process boundary.
//! No secret MLS state, group key, or message contents leave these
//! observations.

use crate::{context::XmtpSharedContext, subscriptions::barrier};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicU64, Ordering},
};
use xmtp_common::{
    ErrorCode, RetryableError,
    time::{Duration, Instant},
};
use xmtp_db::consent_record::ConsentState;
use xmtp_proto::types::GroupId;

pub use crate::groups::diagnostics::{CommitSnapshot, GroupSnapshot, MemberSnapshot};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BarrierCause {
    TargetPending,
    ReceiptPending,
    ProcessingPending,
    Blocked { code: String },
    Storage { code: String, retryable: bool },
    Receiver { code: String, retryable: bool },
    InvalidTopic,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BarrierTopicSnapshot {
    pub topic: String,
    pub target: Option<u64>,
    pub received: u64,
    pub processed: u64,
    pub unresolved_welcomes: Vec<u64>,
    pub inactive: bool,
    pub cause: Option<BarrierCause>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum BarrierFailure {
    Blocked,
    Deadline,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckpointSnapshot {
    pub group_ids: Vec<String>,
    pub topics: Vec<BarrierTopicSnapshot>,
    pub failure: Option<BarrierFailure>,
}

/// Drive fixed targets with every consent state and preserve complete obligations too.
pub async fn checkpoint<C: XmtpSharedContext>(
    context: &C,
    groups: &[GroupId],
    budget: Duration,
) -> CheckpointSnapshot {
    let (groups, snapshot, failure) = barrier::receive_with_welcomes_snapshot_until(
        context,
        groups.to_vec(),
        Some(vec![
            ConsentState::Allowed,
            ConsentState::Unknown,
            ConsentState::Denied,
        ]),
        Instant::now() + budget,
    )
    .await;
    CheckpointSnapshot {
        group_ids: groups.into_iter().map(hex::encode).collect(),
        topics: snapshot
            .topics
            .into_iter()
            .map(|topic| BarrierTopicSnapshot {
                topic: hex::encode(topic.topic),
                target: topic.target.map(|c| c.0),
                received: topic.received.0,
                processed: topic.processed.0,
                unresolved_welcomes: topic.unresolved_welcomes.into_iter().map(|c| c.0).collect(),
                inactive: topic.inactive,
                cause: topic.cause.map(|cause| match cause {
                    barrier::BarrierCause::TargetPending => BarrierCause::TargetPending,
                    barrier::BarrierCause::ReceiptPending => BarrierCause::ReceiptPending,
                    barrier::BarrierCause::ProcessingPending => BarrierCause::ProcessingPending,
                    barrier::BarrierCause::Blocked(code) => BarrierCause::Blocked { code },
                    barrier::BarrierCause::Storage(error) => BarrierCause::Storage {
                        code: error.error_code().into(),
                        retryable: error.is_retryable(),
                    },
                    barrier::BarrierCause::Receiver(error) => BarrierCause::Receiver {
                        code: error.code().into(),
                        retryable: error.is_retryable(),
                    },
                    barrier::BarrierCause::InvalidTopic => BarrierCause::InvalidTopic,
                }),
            })
            .collect(),
        failure: failure.map(|failure| match failure {
            barrier::BarrierFailure::Blocked => BarrierFailure::Blocked,
            barrier::BarrierFailure::Deadline => BarrierFailure::Deadline,
            barrier::BarrierFailure::Cancelled => BarrierFailure::Cancelled,
        }),
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct RetryBudgets {
    pub barrier_ms: u64,
    pub blocked_welcome_ms: u64,
    pub permanent_receiver_ms: u64,
}

impl Default for RetryBudgets {
    fn default() -> Self {
        use xmtp_configuration::*;
        Self {
            barrier_ms: STREAM_BARRIER_TIMEOUT.as_millis() as u64,
            blocked_welcome_ms: STREAM_BLOCKED_WELCOME_RESCAN_INTERVAL.as_millis() as u64,
            permanent_receiver_ms: STREAM_PERMANENT_RETRY_MAX.as_millis() as u64,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum WelcomeRetryCause {
    IdentityDependency,
    GroupPrefix,
    Pointee,
    UnsupportedWelcome,
    Blocked,
    Retry,
}

static OWN_COMMIT_EPOCH_CONFLICTS: AtomicU64 = AtomicU64::new(0);
static WELCOME_RETRIES: [AtomicU64; 6] = [const { AtomicU64::new(0) }; 6];
const WELCOME_CAUSES: [WelcomeRetryCause; 6] = [
    WelcomeRetryCause::IdentityDependency,
    WelcomeRetryCause::GroupPrefix,
    WelcomeRetryCause::Pointee,
    WelcomeRetryCause::UnsupportedWelcome,
    WelcomeRetryCause::Blocked,
    WelcomeRetryCause::Retry,
];

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContentionSnapshot {
    pub own_commit_epoch_conflicts: u64,
    pub welcome_retries: BTreeMap<WelcomeRetryCause, u64>,
}

/// Cumulative observations for this process. A process restart resets them.
pub fn contention_snapshot() -> ContentionSnapshot {
    ContentionSnapshot {
        own_commit_epoch_conflicts: OWN_COMMIT_EPOCH_CONFLICTS.load(Ordering::Relaxed),
        welcome_retries: WELCOME_CAUSES
            .into_iter()
            .zip(&WELCOME_RETRIES)
            .map(|(cause, counter)| (cause, counter.load(Ordering::Relaxed)))
            .collect(),
    }
}

pub(crate) fn record_own_commit_epoch_conflict() {
    OWN_COMMIT_EPOCH_CONFLICTS.fetch_add(1, Ordering::Relaxed);
}

/// Called only after the retry row was stored successfully.
pub(crate) fn record_welcome_retry(code: &str) {
    let cause = match code {
        "identity_dependency" => WelcomeRetryCause::IdentityDependency,
        "group_prefix" => WelcomeRetryCause::GroupPrefix,
        "welcome_pointee" => WelcomeRetryCause::Pointee,
        "unsupported_welcome" => WelcomeRetryCause::UnsupportedWelcome,
        "welcome_blocked" => WelcomeRetryCause::Blocked,
        _ => WelcomeRetryCause::Retry,
    };
    WELCOME_RETRIES[cause as usize].fetch_add(1, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::groups::GroupError;
    use crate::groups::diagnostics::SNAPSHOT_COMMIT_LIMIT;
    use crate::{tester, utils::test::MlsGroupExt};
    use xmtp_db::{group::GroupMembershipState, prelude::*};

    #[xmtp_common::test(unwrap_try = true)]
    async fn commit_snapshot_limit_preserves_latest_readd_boundary() {
        use xmtp_db::{local_commit_log::NewLocalCommitLog, remote_commit_log::CommitResult};

        tester!(alix, disable_workers);
        let group = alix.create_group(None, None)?;
        let before = group.diagnostic_snapshot()?;
        let added = SNAPSHOT_COMMIT_LIMIT + 1;
        for index in 1..=added {
            let welcome = index == added;
            NewLocalCommitLog {
                group_id: group.group_id,
                commit_sequence_id: if welcome { 0 } else { index },
                last_epoch_authenticator: Vec::new(),
                commit_result: CommitResult::Success,
                applied_epoch_number: index,
                applied_epoch_authenticator: Vec::new(),
                error_message: None,
                sender_inbox_id: None,
                sender_installation_id: None,
                commit_type: Some(if welcome { "Welcome" } else { "KeyUpdate" }.into()),
            }
            .store(&alix.context.db())?;
        }
        let snapshot = group.diagnostic_snapshot()?;
        assert_eq!(snapshot.commits.len(), SNAPSHOT_COMMIT_LIMIT as usize);
        assert_eq!(
            snapshot.omitted_commit_count,
            before.commits.len() as u64 + 1
        );
        assert_eq!(snapshot.commits.first()?.sequence_id, 2);
        assert_eq!(snapshot.commits.last()?.sequence_id, 0);
        assert_eq!(
            snapshot.commits.last()?.commit_type.as_deref(),
            Some("Welcome")
        );
        assert_eq!(snapshot.epoch_authenticator, before.epoch_authenticator);
    }

    /// A snapshot can leave the process, so it never carries the group root
    /// that the AppData dictionary holds, or a key derived from it.
    // verifies: GMOD-042
    #[xmtp_common::test(unwrap_try = true)]
    async fn snapshot_holds_no_group_root() {
        use xmtp_mls_common::app_data::group_root::read_group_root;

        tester!(alix, disable_workers);
        let group = alix.create_group(None, None)?;
        let root = group
            .with_group_snapshot(|mls_group| Ok(read_group_root(mls_group.extensions())))???;
        let report = serde_json::to_vec(&group.diagnostic_snapshot()?)?;
        for (secret, encoded) in crate::utils::test::group_secret_encodings(&root) {
            assert!(
                !report.windows(encoded.len()).any(|w| w == encoded),
                "the snapshot holds the {secret}"
            );
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn snapshot_preserves_leaf_membership_and_restored_state() {
        tester!(alix, disable_workers);
        tester!(bo, disable_workers);
        let group = alix.create_group(None, None)?;
        group.invite(&bo).await?;
        // A Welcome can be processed before sync samples its initial group list.
        // Wait for the named group; sync's return value lists only new discoveries.
        let bo_group = xmtp_common::wait_for_ok(|| async {
            bo.sync_welcomes()
                .await
                .and_then(|_| bo.group(&group.group_id).map_err(GroupError::from))
        })
        .await?;
        let first = group.diagnostic_snapshot()?;
        let peer = bo_group.diagnostic_snapshot()?;
        assert_eq!(first.epoch, peer.epoch);
        assert_eq!(first.epoch_authenticator, peer.epoch_authenticator);
        assert_eq!(first.members, peer.members);
        assert_eq!(first.metadata, peer.metadata);
        assert!(
            first
                .members
                .iter()
                .any(|member| member.installation_id == hex::encode(bo.context.installation_id()))
        );
        assert!(first.active);
        assert!(!first.commits.is_empty());
        bo.context
            .db()
            .update_group_membership(bo_group.group_id, GroupMembershipState::Restored)?;
        let restored = bo_group.diagnostic_snapshot()?;
        assert_eq!(restored.membership_state, GroupMembershipState::Restored);
        assert!(!restored.active);
        assert_eq!(restored.epoch_authenticator, first.epoch_authenticator);
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn checkpoint_includes_denied_welcome_discoveries() {
        tester!(alix, disable_workers);
        tester!(bo, disable_workers);
        let group = alix.create_group(None, None)?;
        group.invite(&bo).await?;
        // A Welcome can be processed before sync samples its initial group list.
        // Wait for the named group; sync's return value lists only new discoveries.
        let bo_group = xmtp_common::wait_for_ok(|| async {
            bo.sync_welcomes()
                .await
                .and_then(|_| bo.group(&group.group_id).map_err(GroupError::from))
        })
        .await?;
        bo_group.update_consent_state(ConsentState::Denied)?;
        group.send_msg(b"denied group checkpoint").await;
        let snapshot = checkpoint(
            &bo.context,
            &[],
            Duration::from_millis(RetryBudgets::default().barrier_ms),
        )
        .await;
        assert!(snapshot.failure.is_none(), "{snapshot:?}");
        assert!(snapshot.group_ids.contains(&hex::encode(group.group_id)));
        assert!(snapshot.topics.iter().all(|topic| topic.cause.is_none()));
        assert!(
            snapshot
                .topics
                .iter()
                .any(|topic| topic.target.is_some_and(|target| target > 0))
        );
        assert_eq!(
            group.diagnostic_snapshot()?.epoch_authenticator,
            bo_group.diagnostic_snapshot()?.epoch_authenticator
        );
    }
}
