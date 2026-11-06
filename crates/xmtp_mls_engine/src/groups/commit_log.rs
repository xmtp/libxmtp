use crate::groups::MlsGroup;
use crate::groups::mls_sync::retry_due;
use crate::groups::oneshot::Oneshot;
use crate::groups::summary::SyncSummary;
use futures::StreamExt;
use std::collections::HashSet;
use std::sync::Arc;
use std::{collections::HashMap, time::Duration};
use thiserror::Error;
use xmtp_api::ApiError;
use xmtp_common::RetryableError;
use xmtp_common::hex::NormalizeHex;
use xmtp_common::snippet::Snippet;
use xmtp_common::time::now_ns;
use xmtp_db::TransactionOutcome::Continue;
use xmtp_db::consent_record::ConsentState;
use xmtp_db::group::StoredGroupForRespondingReadds;
use xmtp_db::remote_commit_log::RemoteCommitLog;
use xmtp_db::remote_commit_log::RemoteCommitLogOrder;
use xmtp_db::{
    DbQuery, StorageError, Store, TransactionOutcome,
    group::StoredGroupForReaddRequest,
    local_commit_log::{CommitType, LocalCommitLogOrder},
    prelude::*,
    readd_status::QueryReaddStatus,
    remote_commit_log::{CommitResult, NewRemoteCommitLog},
};
use xmtp_mls_core::config::ForkRecoveryPolicy;
use xmtp_mls_core::events::dm_event_context;
use xmtp_mls_group_keys::commit_log::{open_entry, seal_entry, sign_entry_author};
use xmtp_mls_group_keys::group_keys::{GroupKeys, SealError};
use xmtp_mls_group_state::app_data::group_root::group_keys_from_extensions;
use xmtp_mls_validation::ForkActors;
use xmtp_proto::ShortHex;
use xmtp_proto::backend_v1::SealedCommitLogEntry;
use xmtp_proto::types::Cursor;
use xmtp_proto::types::{CommitLogEntry as DecodedCommitLogEntry, Topic, TopicCursor};
use xmtp_proto::xmtp::mls::message_contents::CommitResult as ProtoCommitResult;
use xmtp_proto::xmtp::mls::message_contents::PlaintextCommitLogEntry;

use crate::groups::{GroupError, context::XmtpSharedContext};
use xmtp_mls_core::worker::{
    BoxedWorker, NeedsDbReconnect, NewWorker, Worker, WorkerFactory, WorkerKind, WorkerResult,
};
use xmtp_proto::xmtp::mls::message_contents::{
    OneshotMessage, ReaddRequest, oneshot_message::MessageType,
};

use xmtp_events::{ClientEvent, EventWriter, GroupRef};
use xmtp_proto::types::GroupId;

mod error;
mod fork;
mod publish;
mod readd;
mod remote;
#[cfg(any(test, feature = "test-utils"))]
mod test_support;

pub use error::CommitLogError;
pub use remote::SaveRemoteCommitLogResult;
#[cfg(test)]
pub(crate) use remote::should_skip_remote_commit_log_entry;
#[cfg(any(test, feature = "test-utils"))]
pub use test_support::{CommitLogTestFunction, TestResult};

/// Interval at which the CommitLogWorker runs to publish commit log entries.
pub const DEFAULT_INTERVAL_DURATION: Duration = Duration::from_secs(60 * 5); // 5 minutes

#[derive(Clone)]
pub struct Factory<Context> {
    context: Context,
    /// Kept by the factory, so a storage reconnect, which builds a new
    /// worker, keeps every rate-limit hold.
    holds: Arc<parking_lot::Mutex<Holds>>,
}

impl<Context> Factory<Context>
where
    Context: XmtpSharedContext + 'static,
{
    pub(crate) fn new(context: Context) -> Self {
        Self {
            context,
            holds: Arc::default(),
        }
    }

    /// A worker that shares this factory's rate-limit holds.
    pub(crate) fn worker(&self) -> CommitLogWorker<Context> {
        CommitLogWorker::with_holds(self.context.clone(), self.holds.clone())
    }
}

impl<Context> WorkerFactory for Factory<Context>
where
    Context: XmtpSharedContext + 'static,
{
    fn kind(&self) -> WorkerKind {
        WorkerKind::CommitLog
    }

    fn create(&self) -> BoxedWorker {
        Box::new(self.worker())
    }
}

#[xmtp_common::async_trait]
impl<Context> Worker for CommitLogWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    fn kind(&self) -> WorkerKind {
        WorkerKind::CommitLog
    }

    async fn run_tasks(&mut self) -> WorkerResult<()> {
        self.run().await.map_err(|e| Box::new(e) as Box<_>)
    }
}

impl<Context> NewWorker<Context> for CommitLogWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    fn factory(context: Context) -> impl WorkerFactory + 'static {
        Factory::new(context)
    }
}

/// Conversations held by a rate limit, and when each delay ends (RATE-041).
#[derive(Default)]
pub(crate) struct Holds {
    /// Conversations whose commit-log publish was refused. Other
    /// conversations still publish.
    publish: HashMap<GroupId, xmtp_common::time::Instant>,
    /// Conversations whose readd answer was refused. Other conversations
    /// still answer.
    readd: HashMap<GroupId, xmtp_common::time::Instant>,
}

pub struct CommitLogWorker<Context> {
    context: Context,
    /// The conversations whose commit-log publish failed in the last turn.
    failed_publishes: Vec<GroupId>,
    /// Rate-limit holds, shared with the factory that built this worker.
    holds: Arc<parking_lot::Mutex<Holds>>,
    #[cfg(test)]
    seal_fault: Option<SealFault>,
}

/// Hold `id` in `held` until `delay`, a rate-limit delay, has passed. A
/// shorter later delay never ends a pending one.
// implements: RATE-041
fn hold(
    held: &mut HashMap<GroupId, xmtp_common::time::Instant>,
    id: GroupId,
    delay: Option<Duration>,
) {
    let Some(delay) = delay else {
        return;
    };
    let until = xmtp_common::time::saturating_after(xmtp_common::time::Instant::now(), delay);
    let held = held.entry(id).or_insert(until);
    *held = (*held).max(until);
}

/// A test-only fault at the result of each seal, by conversation.
#[cfg(test)]
pub(crate) type SealFault = std::sync::Arc<
    dyn Fn(
            &GroupId,
            Result<SealedCommitLogEntry, SealError>,
        ) -> Result<SealedCommitLogEntry, SealError>
        + Send
        + Sync,
>;

impl<Context> CommitLogWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    pub fn new(context: Context) -> Self {
        Self::with_holds(context, Arc::default())
    }

    fn with_holds(context: Context, holds: Arc<parking_lot::Mutex<Holds>>) -> Self {
        Self {
            context,
            failed_publishes: Vec::new(),
            holds,
            #[cfg(test)]
            seal_fault: None,
        }
    }

    /// The conversations whose commit-log publish failed in the last turn.
    /// A failed publish ends the turn, not the worker, so `tick` still
    /// returns `Ok`; a health check reads the failures here.
    pub fn failed_publishes(&self) -> &[GroupId] {
        &self.failed_publishes
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct ConversationCursorInfo {
    pub conversation_id: Vec<u8>,
    pub num_entries_published: usize,
    pub last_entry_published_sequence_id: i64,
    pub last_entry_published_rowid: i64,
    /// The local rowid of each published entry, in publish order.
    pub entry_rowids: Vec<i64>,
}

impl ConversationCursorInfo {
    /// The publish cursor to save once the first `stored` entries of this
    /// conversation's publish are stored, or `None` when none is. A fully
    /// stored publish moves to the conversation's last row, past skipped
    /// rows. A partly stored one moves to the row of its last stored entry,
    /// so that the next turn starts from its first unstored entry.
    // implements: FORK-021
    fn stored_cursor(&self, stored: usize) -> Option<i64> {
        if stored >= self.entry_rowids.len() {
            return Some(self.last_entry_published_rowid);
        }
        stored.checked_sub(1).map(|last| self.entry_rowids[last])
    }
}

// CommitLogWorker implementation
impl<Context> CommitLogWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    async fn run(&mut self) -> Result<(), CommitLogError> {
        // Precedence: fork-recovery override (clamped to >=2s) > WorkerConfig >
        // const default. Fork recovery wins because its cadence is tied to
        // recovery-protocol timing, not general worker tuning.
        let (mut worker_interval, jitter) = self
            .context
            .worker_interval(WorkerKind::CommitLog, DEFAULT_INTERVAL_DURATION);
        if let Some(interval) = self.context.fork_recovery_opts().worker_interval_ns {
            worker_interval = Duration::from_nanos(interval).max(Duration::from_secs(2));
        }
        let mut intervals = xmtp_common::time::jittered_interval_stream(worker_interval, jitter);
        while (intervals.next().await).is_some() {
            self.tick().await?;
        }
        Ok(())
    }

    #[tracing::instrument(skip_all, fields(worker = ?self.kind(), operation = "worker_turn"))]
    pub async fn tick(&mut self) -> Result<(), CommitLogError> {
        self.save_remote_commit_log().await?;
        self.update_forked_state().await?;
        self.publish_commit_logs_to_remote().await?;
        self.send_outgoing_readd_requests().await?;
        self.handle_incoming_pending_readds().await?;
        Ok(())
    }
}

#[cfg(test)]
mod stored_cursor_tests {
    use super::*;

    /// A conversation with `count` published entries on rows 10, 20, ...,
    /// whose last row is a skipped one after them.
    fn conversation(id: u8, count: usize) -> ConversationCursorInfo {
        let entry_rowids: Vec<i64> = (1..=count as i64).map(|n| n * 10).collect();
        ConversationCursorInfo {
            conversation_id: vec![id],
            num_entries_published: count,
            last_entry_published_sequence_id: count as i64,
            last_entry_published_rowid: entry_rowids.last().copied().unwrap_or(0) + 5,
            entry_rowids,
        }
    }

    /// One conversation's backlog spans two requests and the second fails:
    /// the cursor moves to the last entry of the first request, so the next
    /// turn starts from entry 1001, not entry 1. A fully stored backlog moves
    /// past its last row, and a publish that stored nothing moves nothing.
    // verifies: FORK-021
    #[xmtp_common::test(unwrap_try = true)]
    fn a_partly_stored_backlog_resumes_from_its_first_unstored_entry() {
        let info = conversation(1, 1500);
        assert_eq!(info.stored_cursor(1000), Some(10_000));
        assert_eq!(info.stored_cursor(1), Some(10));
        assert_eq!(info.stored_cursor(1500), Some(15_005));
        assert_eq!(info.stored_cursor(0), None);
    }

    /// A conversation whose rows are all skipped publishes nothing and moves
    /// past them.
    #[xmtp_common::test(unwrap_try = true)]
    fn a_conversation_with_only_skipped_rows_moves_past_them() {
        assert_eq!(conversation(2, 0).stored_cursor(0), Some(5));
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod publication_snapshot_tests {
    use super::*;
    use crate::groups::UpdateAdminListType::{AddSuper, RemoveSuper};
    use crate::groups::test_support::{EnginePeer, EngineTestPeer, EngineTestPeerOps};
    use crate::prelude::*;

    use std::sync::{Arc, Mutex, mpsc};
    use xmtp_db::mock::MockDbQuery;

    // verifies: FORK-085
    #[xmtp_common::test(unwrap_try = true)]
    async fn publication_excludes_a_demotion_applied_after_its_authority_snapshot() {
        let alix = EngineTestPeer::new().await;
        let bo = EngineTestPeer::new().await;
        let a = alix
            .create_group_with_members(&[bo.inbox_id()], None, None)
            .await?;
        bo.sync_welcomes().await?;
        let b = bo.group(&a.group_id)?;
        a.update_admin_list(AddSuper, bo.inbox_id().to_string())
            .await?;
        b.sync().await?;
        let mut worker = CommitLogWorker::new(alix.context.clone());
        worker.publish_commit_logs_to_remote().await?;

        a.update_group_name("pending before demotion".into())
            .await?;
        b.sync().await?;
        let pending = alix.context.db().get_latest_log_for_group(&a.group_id)??;
        b.update_admin_list(RemoveSuper, alix.inbox_id().to_string())
            .await?;
        let demotion = bo.context.db().get_latest_log_for_group(&a.group_id)??;

        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        let gate = Arc::new(Mutex::new(Some((ready_tx, resume_rx))));
        let pause = Arc::new(move || {
            let barrier = gate.lock().unwrap().take();
            if let Some((ready, resume)) = barrier {
                ready.send(()).unwrap();
                resume.recv_timeout(Duration::from_secs(30)).unwrap();
            }
        });
        let context = alix.context.clone();
        let group_id = a.group_id;
        let preparation = std::thread::spawn(move || {
            let mut conn = MockDbQuery::new();
            // Forward every read to the real database. Pause at the first read
            // outside the state transaction, even if the maximum-row read is
            // incorrectly moved out of that transaction in a future change.
            let db = context.db();
            let pause_cursor = pause.clone();
            conn.expect_get_local_commit_log_cursor()
                .returning(move |id| {
                    pause_cursor();
                    db.get_local_commit_log_cursor(id)
                });
            let db = context.db();
            conn.expect_get_latest_chain_start_rowid()
                .returning(move |id| {
                    pause();
                    db.get_latest_chain_start_rowid(id)
                });
            let db = context.db();
            conn.expect_get_last_cursor()
                .returning(move |id, kind| db.get_last_cursor(id, kind));
            let db = context.db();
            conn.expect_get_local_commit_log_after_cursor()
                .returning(move |id, cursor, order| {
                    db.get_local_commit_log_after_cursor(id, cursor, order)
                });
            CommitLogWorker::new(context)
                .prepare_conversation_publish(&conn, &group_id)
                .unwrap()
                .unwrap()
        });

        xmtp_common::time::timeout(Duration::from_secs(30), ready_rx).await??;
        a.sync().await?;
        assert!(
            !alix
                .context
                .db()
                .find_group(&a.group_id)??
                .should_publish_commit_log
        );
        assert_eq!(
            alix.context
                .db()
                .get_latest_log_for_group(&a.group_id)??
                .commit_sequence_id,
            demotion.commit_sequence_id
        );
        resume_tx.send(())?;
        let prepared = preparation.join().unwrap();

        let mut stored = Vec::new();
        alix.context
            .api()
            .publish_commit_log(prepared.entries, &mut stored)
            .await?;
        let keys = worker.group_keys(&a.group_id)??;
        let topic = Topic::new_commit_log(keys.commit_log_id());
        let remote = alix
            .context
            .api()
            .query_commit_log([(topic, Cursor(0))].into())
            .await?;
        let entries = remote
            .iter()
            .map(|entry| {
                open_entry(
                    &keys,
                    a.group_id.as_slice(),
                    entry.cursor.0,
                    entry.sealed.as_ref().unwrap(),
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        assert!(
            entries
                .iter()
                .any(|entry| entry.commit_sequence_id == pending.commit_sequence_id as u64)
        );
        assert!(
            entries
                .iter()
                .all(|entry| entry.commit_sequence_id != demotion.commit_sequence_id as u64)
        );
        assert_eq!(
            prepared.cursor_info.stored_cursor(stored.len()),
            Some(i64::from(pending.rowid))
        );
        assert!(
            worker
                .prepare_conversation_publish(&alix.context.db(), &a.group_id)?
                .is_none()
        );
    }
}

// These fixtures use native pool errors. Browser connection lifecycle tests
// cover closed persistent handles in the WASM database module.
#[cfg(all(test, not(target_arch = "wasm32")))]
mod disconnect_propagation_tests {
    //! Pins that a dropped-pool signal survives this owner's error types, so
    //! `needs_db_reconnect()` stays `true` for the worker that surfaces them.
    use super::CommitLogError;
    use crate::groups::GroupError;
    use xmtp_db::{ConnectionError, PlatformStorageError, StorageError};
    use xmtp_mls_core::reconnect::NeedsDbReconnect;

    /// A `StorageError` that signals the connection pool was dropped.
    fn disconnect_storage() -> StorageError {
        StorageError::Platform(PlatformStorageError::PoolNeedsConnection)
    }

    /// A `ConnectionError` that signals the connection pool was dropped.
    fn disconnect_connection() -> ConnectionError {
        ConnectionError::Platform(PlatformStorageError::PoolNeedsConnection)
    }

    #[xmtp_common::test]
    fn commit_log_error_forwards_disconnect() {
        assert!(CommitLogError::Connection(disconnect_connection()).needs_db_reconnect());
        assert!(
            CommitLogError::GroupError(GroupError::Storage(disconnect_storage()))
                .needs_db_reconnect()
        );
        assert!(
            CommitLogError::SyncError(crate::groups::summary::SyncSummary::other(
                GroupError::Storage(disconnect_storage()),
            ))
            .needs_db_reconnect()
        );
        // A transient (non-disconnect) connection error must not stop the worker.
        assert!(
            !CommitLogError::Connection(ConnectionError::DisconnectInTransaction)
                .needs_db_reconnect()
        );
    }
}
