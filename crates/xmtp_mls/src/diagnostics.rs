//! Test-only observations for convergence and liveness checks.
//!
//! IDs and public MLS state use hex so reports can cross a process boundary.
//! No secret MLS state or message contents leave these observations.

use crate::{
    context::XmtpSharedContext,
    groups::{GroupError, MlsGroup},
    subscriptions::barrier,
};
use openmls::prelude::BasicCredential;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicU64, Ordering},
};
use tls_codec::Serialize as _;
use xmtp_common::{
    ErrorCode, RetryableError,
    time::{Duration, Instant},
};
use xmtp_db::{
    ConnectionExt, NotFound, TransactionOutcome, XmtpMlsStorageProvider,
    consent_record::ConsentState, group::GroupMembershipState, prelude::*,
};
use xmtp_proto::types::GroupId;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct MemberSnapshot {
    pub inbox_id: String,
    pub installation_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommitSnapshot {
    pub sequence_id: i64,
    pub epoch: i64,
    pub authenticator: String,
    pub last_authenticator: String,
    pub result: i32,
    pub commit_type: Option<String>,
    pub removed_this_installation: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GroupSnapshot {
    pub group_id: String,
    /// Zero with an empty authenticator when a Restored archive stub has no MLS state.
    pub epoch: u64,
    pub epoch_authenticator: String,
    pub members: Vec<MemberSnapshot>,
    /// Serialized public context extensions, including all metadata and app data.
    pub metadata: String,
    pub membership_state: GroupMembershipState,
    pub active: bool,
    pub maybe_forked: bool,
    pub is_commit_log_forked: Option<bool>,
    pub cursor: u64,
    pub commits: Vec<CommitSnapshot>,
    /// Older local records excluded by the per-snapshot commit limit.
    #[serde(default)]
    pub omitted_commit_count: u64,
}

/// Maximum commit records sent in one group snapshot.
const SNAPSHOT_COMMIT_LIMIT: i64 = 256;

fn recent_commit_records(
    db: &impl DbQuery,
    group_id: &GroupId,
) -> Result<(Vec<xmtp_db::local_commit_log::LocalCommitLog>, u64), xmtp_db::ConnectionError> {
    use xmtp_db::diesel::prelude::*;
    use xmtp_db::schema::local_commit_log::dsl;

    ConnectionExt::raw_query(db, |connection| {
        let total = dsl::local_commit_log
            .filter(dsl::group_id.eq(group_id))
            .count()
            .get_result::<i64>(connection)?;
        let mut records = dsl::local_commit_log
            .filter(dsl::group_id.eq(group_id))
            .order(dsl::rowid.desc())
            .limit(SNAPSHOT_COMMIT_LIMIT)
            .load::<xmtp_db::local_commit_log::LocalCommitLog>(connection)?;
        // Row order preserves a re-add's sequence-zero Welcome among later commits.
        records.reverse();
        let omitted = total.saturating_sub(records.len() as i64) as u64;
        Ok((records, omitted))
    })
}

impl<C: XmtpSharedContext> MlsGroup<C> {
    /// Read every field under one database writer, including cross-process writers.
    pub fn diagnostic_snapshot(&self) -> Result<GroupSnapshot, GroupError> {
        crate::state_tx::state_write(self.context.mls_storage(), |tx| {
            let storage = tx.storage();
            let db = storage.db();
            let stored = db
                .find_group(&self.group_id)?
                .ok_or(NotFound::GroupById(self.group_id))?;
            let (records, omitted_commit_count) = recent_commit_records(&db, &self.group_id)?;
            let commits = records
                .into_iter()
                .map(|entry| CommitSnapshot {
                    sequence_id: entry.commit_sequence_id,
                    epoch: entry.applied_epoch_number,
                    authenticator: hex::encode(entry.applied_epoch_authenticator),
                    last_authenticator: hex::encode(entry.last_epoch_authenticator),
                    result: entry.commit_result as i32,
                    removed_this_installation: entry.commit_type.as_deref()
                        == Some("RemovedFromGroup"),
                    commit_type: entry.commit_type,
                })
                .collect();
            let group = openmls::group::MlsGroup::load(&storage, &self.group_id.to_openmls())?;
            // An archive stub can owe membership without having MLS state yet.
            let Some(group) = group else {
                if stored.membership_state != GroupMembershipState::Restored {
                    return Err(GroupError::Storage(
                        NotFound::MlsGroup(self.group_id).into(),
                    ));
                }
                return Ok(TransactionOutcome::Continue(GroupSnapshot {
                    group_id: hex::encode(self.group_id),
                    epoch: 0,
                    epoch_authenticator: String::new(),
                    members: Vec::new(),
                    metadata: String::new(),
                    membership_state: stored.membership_state,
                    active: false,
                    maybe_forked: stored.maybe_forked,
                    is_commit_log_forked: stored.is_commit_log_forked,
                    cursor: db
                        .topic_progress(&xmtp_db::incoming_envelope::StreamTopic::group(
                            self.group_id,
                        ))?
                        .processed
                        .0,
                    commits,
                    omitted_commit_count,
                }));
            };
            let mut members = group
                .members()
                .map(|member| {
                    let credential = BasicCredential::try_from(member.credential)?;
                    Ok(MemberSnapshot {
                        inbox_id: crate::identity::parse_credential(credential.identity())?,
                        installation_id: hex::encode(member.signature_key),
                    })
                })
                .collect::<Result<Vec<_>, GroupError>>()?;
            members.sort();
            Ok::<_, GroupError>(TransactionOutcome::Continue(GroupSnapshot {
                group_id: hex::encode(self.group_id),
                epoch: group.epoch().as_u64(),
                epoch_authenticator: hex::encode(group.epoch_authenticator().as_slice()),
                members,
                metadata: hex::encode(group.extensions().tls_serialize_detached()?),
                membership_state: stored.membership_state,
                active: stored.membership_state != GroupMembershipState::Restored
                    && group.is_active(),
                maybe_forked: stored.maybe_forked,
                is_commit_log_forked: stored.is_commit_log_forked,
                cursor: db
                    .topic_progress(&xmtp_db::incoming_envelope::StreamTopic::group(
                        self.group_id,
                    ))?
                    .processed
                    .0,
                commits,
                omitted_commit_count,
            }))
        })
        .map(TransactionOutcome::into_continued)
    }
}

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
    use crate::{tester, utils::test::MlsGroupExt};

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
