use crate::builder::ForkRecoveryPolicy;
use crate::groups::MlsGroup;
use crate::groups::oneshot::Oneshot;
use crate::groups::summary::SyncSummary;
use futures::StreamExt;
use std::collections::HashSet;
use std::{collections::HashMap, time::Duration};
use thiserror::Error;
use xmtp_api::ApiError;
use xmtp_common::RetryableError;
use xmtp_common::hex::NormalizeHex;
use xmtp_common::snippet::Snippet;
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
use xmtp_mls_common::app_data::group_root::group_keys_from_extensions;
use xmtp_mls_common::commit_log::{open_entry, seal_entry, sign_entry_author};
use xmtp_mls_common::group_keys::{GroupKeys, SealError};
use xmtp_mls_validation::ForkActors;
use xmtp_proto::backend_v1::SealedCommitLogEntry;
use xmtp_proto::types::Cursor;
use xmtp_proto::types::{CommitLogEntry as DecodedCommitLogEntry, Topic, TopicCursor};
use xmtp_proto::xmtp::mls::message_contents::CommitResult as ProtoCommitResult;
use xmtp_proto::xmtp::mls::message_contents::PlaintextCommitLogEntry;

use crate::{
    context::XmtpSharedContext,
    groups::GroupError,
    worker::{BoxedWorker, NeedsDbReconnect, Worker, WorkerFactory, WorkerKind, WorkerResult},
};
use xmtp_proto::xmtp::mls::message_contents::{
    OneshotMessage, ReaddRequest, oneshot_message::MessageType,
};

use xmtp_events::{ClientEvent, EventWriter, GroupRef};
use xmtp_proto::types::GroupId;
/// Interval at which the CommitLogWorker runs to publish commit log entries.
pub const DEFAULT_INTERVAL_DURATION: Duration = Duration::from_secs(60 * 5); // 5 minutes

#[derive(Clone)]
pub struct Factory<Context> {
    context: Context,
}

impl<Context> WorkerFactory for Factory<Context>
where
    Context: XmtpSharedContext + 'static,
{
    fn kind(&self) -> WorkerKind {
        WorkerKind::CommitLog
    }

    fn create(
        &self,
        metrics: Option<crate::worker::DynMetrics>,
    ) -> (BoxedWorker, Option<crate::worker::DynMetrics>) {
        (
            Box::new(CommitLogWorker::new(self.context.clone())) as Box<_>,
            metrics,
        )
    }
}

#[derive(Debug, Error)]
pub enum CommitLogError {
    #[error("generic storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("diesel error: {0}")]
    Diesel(#[from] xmtp_db::diesel::result::Error),
    #[error("generic api error: {0}")]
    Api(#[from] ApiError),
    #[error("connection error: {0}")]
    Connection(#[from] xmtp_db::ConnectionError),
    #[error("prost decode error: {0}")]
    Prost(#[from] prost::DecodeError),
    #[error("keystore error: {0}")]
    KeystoreError(#[from] xmtp_db::sql_key_store::SqlKeyStoreError),
    #[error("group error: {0}")]
    GroupError(#[from] GroupError),
    #[error("crypto error: {0}")]
    CryptoError(#[from] openmls_traits::types::CryptoError),
    #[error("try from slice error: {0}")]
    TryFromSliceError(#[from] std::array::TryFromSliceError),
    #[error("conversion error: {0}")]
    Conversion(#[from] xmtp_proto::ConversionError),
    #[error("Group did not pass readd validation: {0}")]
    GroupReaddValidationError(String),
    #[error("sync error: {0}")]
    SyncError(#[from] SyncSummary),
    #[error("failed to send readd request for group {group_id}: {source}")]
    FailedToSendReadd {
        group_id: GroupId,
        source: Box<CommitLogError>,
    },
    #[error("{count} readd request(s) failed: {errors:?}", count = errors.len())]
    FailedReadds { errors: Vec<CommitLogError> },
    #[error("no latest commit sequence id found for forked group {group_id}")]
    MissingLatestCommitSequenceId { group_id: GroupId },
}

impl RetryableError for CommitLogError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::Storage(storage_error) => storage_error.is_retryable(),
            Self::Diesel(diesel_error) => diesel_error.is_retryable(),
            Self::Api(api_error) => api_error.is_retryable(),
            Self::Connection(connection_error) => connection_error.is_retryable(),
            Self::Prost(_prost_error) => false,
            Self::KeystoreError(keystore_error) => keystore_error.is_retryable(),
            Self::GroupError(group_error) => group_error.is_retryable(),
            Self::CryptoError(_crypto_error) => false,
            Self::TryFromSliceError(_try_from_slice_error) => false,
            Self::Conversion(_) => false,
            Self::GroupReaddValidationError(_group_readd_validation_error) => false,
            Self::SyncError(sync_error) => sync_error.is_retryable(),
            Self::FailedToSendReadd { source, .. } => source.is_retryable(),
            Self::FailedReadds { errors } => errors.iter().any(|e| e.is_retryable()),
            Self::MissingLatestCommitSequenceId { .. } => false,
        }
    }
}

impl NeedsDbReconnect for CommitLogError {
    fn needs_db_reconnect(&self) -> bool {
        match self {
            Self::Storage(s) => s.db_needs_connection(),
            // Only a `PoolNeedsConnection` connection error means the pool is gone.
            Self::Connection(c) => c.db_needs_connection(),
            // A dropped pool can be wrapped inside a GroupError; forward.
            Self::GroupError(e) => e.needs_db_reconnect(),
            // A readd send failure wraps an inner CommitLogError; forward.
            Self::FailedToSendReadd { source, .. } => source.needs_db_reconnect(),
            Self::FailedReadds { errors } => errors.iter().any(|e| e.needs_db_reconnect()),
            // Remaining variants can't carry a dropped-pool signal.
            Self::Diesel(_)
            | Self::Api(_)
            | Self::Prost(_)
            | Self::KeystoreError(_)
            | Self::CryptoError(_)
            | Self::TryFromSliceError(_)
            | Self::Conversion(_)
            | Self::GroupReaddValidationError(_)
            | Self::SyncError(_)
            | Self::MissingLatestCommitSequenceId { .. } => false,
        }
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

    fn factory<C>(context: C) -> impl WorkerFactory + 'static
    where
        C: XmtpSharedContext + 'static,
    {
        Factory { context }
    }
}

pub struct CommitLogWorker<Context> {
    context: Context,
    /// The conversations whose commit-log publish failed in the last turn.
    failed_publishes: Vec<GroupId>,
    #[cfg(test)]
    seal_fault: Option<SealFault>,
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
        Self {
            context,
            failed_publishes: Vec::new(),
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

/// One conversation's sealed entries and the cursor that publishing them
/// reaches.
struct PendingPublish {
    cursor_info: ConversationCursorInfo,
    entries: Vec<SealedCommitLogEntry>,
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

#[derive(Debug, PartialEq, Clone)]
pub struct SaveRemoteCommitLogResult {
    pub conversation_id: Vec<u8>,
    pub num_entries_saved: usize,
}

// Test related types
#[cfg(test)]
pub enum CommitLogTestFunction {
    PublishCommitLogsToRemote,
    SaveRemoteCommitLog,
    CheckForkedState,
    All,
}

#[cfg(test)]
pub struct TestResult {
    pub save_remote_commit_log_results: Option<HashMap<Vec<u8>, usize>>,
    pub publish_commit_log_results: Option<Vec<ConversationCursorInfo>>,
    pub is_forked: Option<HashMap<Vec<u8>, Option<bool>>>,
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

    /// Publish every eligible conversation's new entries, each conversation
    /// in its own publish call, so a refused or unsendable entry holds back
    /// only its own conversation. A failed publish is logged, recorded in
    /// [`Self::failed_publishes`], and moves the conversation's cursor past
    /// the stored prefix only; the conversation is tried again at the next
    /// tick. A refusal or a local parse failure holds back only its own
    /// conversation. A transport failure ends the publishing part of the
    /// turn, since every later conversation would wait through the same
    /// retries. No publish failure stops the worker.
    // implements: FORK-021
    async fn publish_commit_logs_to_remote(
        &mut self,
    ) -> Result<Vec<ConversationCursorInfo>, CommitLogError> {
        self.failed_publishes.clear();
        let conn = &self.context.db();
        // DMs, and groups where we are a super admin
        let conversations = conn.get_conversation_ids_for_remote_log_publish()?;
        let api = self.context.api();
        let mut published = Vec::new();
        for conversation in &conversations {
            let Some(PendingPublish {
                cursor_info,
                entries,
            }) = self.prepare_conversation_publish(conn, &conversation.id)?
            else {
                continue;
            };
            let mut stored = Vec::new();
            let result = if entries.is_empty() {
                Ok(())
            } else {
                tracing::info!(
                    group_id = %conversation.id.as_slice().snippet(),
                    entries = entries.len(),
                    "Publishing commit log entries to remote commit log"
                );
                // Entries go out in order, one request at a time, and
                // publishing stops at the first failed request.
                api.publish_commit_log(entries, &mut stored).await
            };
            // Move the cursor past the stored entries. After a failure the
            // next turn starts at the first entry not stored.
            if let Some(rowid) = cursor_info.stored_cursor(stored.len()) {
                conn.update_cursor(
                    conversation.id,
                    xmtp_db::refresh_state::EntityKind::CommitLogUpload,
                    Cursor(rowid as u64),
                )?;
            }
            match result {
                Ok(()) => published.push(cursor_info),
                Err(error) => {
                    tracing::warn!(
                        group_id = %conversation.id.as_slice().snippet(),
                        stored = stored.len(),
                        %error,
                        "Commit log publish failed; the conversation retries at the next turn"
                    );
                    self.failed_publishes.push(conversation.id);
                    if error.is_retryable() {
                        break;
                    }
                }
            }
        }
        Ok(published)
    }

    /// Seal a conversation's entries recorded after its upload cursor, in
    /// ascending row order, with the cursor that publishing them reaches.
    /// `None` when it has no new entries, or when they cannot be sealed this
    /// turn.
    // implements: FORK-021, FORK-081
    fn prepare_conversation_publish(
        &self,
        conn: &impl DbQuery,
        conversation_id: &GroupId,
    ) -> Result<Option<PendingPublish>, CommitLogError> {
        // Propagate read errors (incl. a dropped pool) to the supervisor; a
        // missing cursor legitimately defaults to 0.
        let local_commit_log_cursor = conn
            .get_local_commit_log_cursor(conversation_id)?
            .unwrap_or(0);
        let membership_floor = conn
            .get_latest_chain_start_rowid(conversation_id)?
            .unwrap_or(0) as u64;
        let published_commit_log_cursor = conn
            .get_last_cursor(
                conversation_id,
                xmtp_db::refresh_state::EntityKind::CommitLogUpload,
            )?
            .0
            .max(membership_floor);
        if local_commit_log_cursor <= published_commit_log_cursor as i32 {
            // We have no new commits to publish for this conversation
            return Ok(None);
        }

        // Local commit log entries are returned sorted in ascending order of `rowid`
        // All local commit log will have rowid > 0 since sqlite rowid starts at 1 https://www.sqlite.org/autoinc.html
        let logs = conn.get_local_commit_log_after_cursor(
            conversation_id,
            published_commit_log_cursor as i64,
            LocalCommitLogOrder::AscendingByRowid,
        )?;
        let Some(max_rowid) = logs.last().map(|log| log.rowid) else {
            return Ok(None);
        };
        // A commit that removed us is recorded locally (RemovedFromGroup,
        // with the pre-commit epoch/authenticator) for debuggability, but
        // must never be published: it does not attest the new epoch and
        // could shadow the real consensus entry published by the
        // remaining members. In practice a removed member is not a
        // publisher (only super admins and DM participants publish, super
        // admins cannot be removed, and DMs have no removals) — this
        // filter makes that property structural. The publish cursor still
        // advances past skipped rows.
        let removed_from_group_marker = CommitType::RemovedFromGroup.to_string();
        let (entry_rowids, plaintext_commit_log_entries): (Vec<i64>, Vec<PlaintextCommitLogEntry>) =
            logs.iter()
                .filter(|log| {
                    log.commit_type.as_deref() != Some(removed_from_group_marker.as_str())
                })
                .map(|log| (log.rowid as i64, PlaintextCommitLogEntry::from(log)))
                .unzip();
        let entries = if plaintext_commit_log_entries.is_empty() {
            Vec::new()
        } else {
            let Some(entries) =
                self.seal_group_logs(conversation_id, &plaintext_commit_log_entries)?
            else {
                return Ok(None);
            };
            entries
        };
        Ok(Some(PendingPublish {
            cursor_info: ConversationCursorInfo {
                conversation_id: conversation_id.to_vec(),
                num_entries_published: plaintext_commit_log_entries.len(),
                last_entry_published_sequence_id: plaintext_commit_log_entries
                    .last()
                    .map(|e| e.commit_sequence_id as i64)
                    .unwrap_or(0),
                last_entry_published_rowid: max_rowid as i64,
                entry_rowids,
            },
            entries,
        }))
    }

    /// Seal entries under the keys of the conversation's group root.
    /// `None` when the conversation has no root or its group state does not
    /// load, or when a seal fails its self-check. Nothing of that
    /// conversation is then published this turn, and the failure is logged
    /// without any entry or key bytes.
    fn seal_group_logs(
        &self,
        conversation_id: &GroupId,
        plaintext_commit_log_entries: &[PlaintextCommitLogEntry],
    ) -> Result<Option<Vec<SealedCommitLogEntry>>, CommitLogError> {
        let Some(keys) = self.group_keys(conversation_id)? else {
            return Ok(None);
        };
        let mut sealed = Vec::with_capacity(plaintext_commit_log_entries.len());
        for entry in plaintext_commit_log_entries {
            let mut entry = entry.clone();
            let sealed_entry = sign_entry_author(
                &keys,
                &self.context.identity().installation_keys,
                &mut entry,
            )
            .and_then(|()| seal_entry(&keys, &entry));
            match self.apply_seal_fault(conversation_id, sealed_entry) {
                Ok(entry) => sealed.push(entry),
                Err(error) => {
                    tracing::error!(
                        group_id = %conversation_id.as_slice().snippet(),
                        %error,
                        "Sealing a commit log entry failed; the conversation publishes nothing this turn"
                    );
                    return Ok(None);
                }
            }
        }
        Ok(Some(sealed))
    }

    #[cfg(not(test))]
    fn apply_seal_fault(
        &self,
        _conversation_id: &GroupId,
        sealed: Result<SealedCommitLogEntry, SealError>,
    ) -> Result<SealedCommitLogEntry, SealError> {
        sealed
    }

    #[cfg(test)]
    fn apply_seal_fault(
        &self,
        conversation_id: &GroupId,
        sealed: Result<SealedCommitLogEntry, SealError>,
    ) -> Result<SealedCommitLogEntry, SealError> {
        match &self.seal_fault {
            Some(fault) => fault(conversation_id, sealed),
            None => sealed,
        }
    }

    /// The keys derived from a conversation's group root, read from its
    /// group context each time and never stored. `None` for a group without
    /// a root, such as a restored placeholder, and for a group whose state
    /// does not load. Either holds back only that conversation. A dropped
    /// database pool is not one conversation's failure, so it is returned.
    fn group_keys(&self, conversation_id: &GroupId) -> Result<Option<GroupKeys>, CommitLogError> {
        let group_context = MlsGroup::new_cached(self.context.clone(), conversation_id)
            .map_err(GroupError::from)
            .and_then(|(group, _)| group.load_group_context());
        let group_context = match group_context {
            Ok(group_context) => group_context,
            Err(error) if error.needs_db_reconnect() => return Err(error.into()),
            Err(error) => {
                tracing::warn!(
                    group_id = %conversation_id.as_slice().snippet(),
                    %error,
                    "A conversation's group state did not load; it skips the commit log this turn"
                );
                return Ok(None);
            }
        };
        match group_keys_from_extensions(group_context.extensions()) {
            Ok(keys) => Ok(Some(keys)),
            Err(error) => {
                tracing::warn!(
                    group_id = %conversation_id.as_slice().snippet(),
                    %error,
                    "A conversation without a group root has no commit log"
                );
                Ok(None)
            }
        }
    }

    // Returns a map of conversation_id to the number of entries saved
    // implements: FORK-030
    async fn save_remote_commit_log(&mut self) -> Result<HashMap<Vec<u8>, usize>, CommitLogError> {
        let conn = &self.context.db();
        // All allowed DMs and groups other than sync and restored groups.
        // Each names its commit-log id, stored at creation and join. A group
        // without one has no root, and so no log to read.
        let conversations: HashMap<Vec<u8>, GroupId> = conn
            .get_conversation_ids_for_remote_log_download()?
            .into_iter()
            .filter_map(|conversation| Some((conversation.commit_log_public_key?, conversation.id)))
            .collect();

        // The download cursor stays keyed by group id; that link is local only.
        let remote_log_cursors = conn.get_remote_log_cursors(
            conversations
                .values()
                .map(|group_id| group_id.as_slice())
                .collect::<Vec<_>>()
                .as_slice(),
        )?;
        let query_log_requests: TopicCursor = conversations
            .iter()
            .map(|(commit_log_id, group_id)| {
                let cursor = remote_log_cursors
                    .get(group_id.as_slice())
                    .copied()
                    .unwrap_or_default();
                (Topic::new_commit_log(commit_log_id), cursor)
            })
            .collect();

        // Skip API call if there are no requests to make
        if query_log_requests.is_empty() {
            tracing::debug!("No commit log requests to query");
            return Ok(HashMap::new());
        }

        let mut keys_by_group = HashMap::new();
        let mut cursors = query_log_requests;
        for group_id in conversations.values() {
            if let Some(keys) = self.group_keys(group_id)? {
                keys_by_group.insert(*group_id, keys);
            }
        }
        cursors.retain(|topic, _| {
            conversations
                .get(topic.identifier())
                .is_some_and(|group| keys_by_group.contains_key(group))
        });
        let policy = self.context.incoming_runtime().policy();
        let limits = xmtp_proto::types::IncomingBatchLimits {
            max_rows: policy.max_fetched_rows as usize,
            max_bytes: usize::try_from(policy.max_fetched_bytes).unwrap_or(usize::MAX),
        };
        let mut save_remote_commit_log_results = HashMap::new();
        self.context
            .api()
            .fold_ordered_pages(cursors, limits, |batches, cursors| {
                for batch in batches {
                    let Some(group_id) = conversations.get(batch.topic.identifier()) else {
                        continue;
                    };
                    let Some(keys) = keys_by_group.get(group_id) else {
                        continue;
                    };
                    let entries = batch
                        .envelopes
                        .into_iter()
                        .map(xmtp_api_backend::envelope::decode_commit_log_entry)
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(ApiError::from)?;
                    let (num_entries, held) = self
                        .save_remote_commit_log_entries_and_update_cursors(
                            conn, group_id, keys, entries,
                        )?;
                    *save_remote_commit_log_results
                        .entry(group_id.to_vec())
                        .or_insert(0) += num_entries;
                    if held {
                        // The durable cursor still precedes the held entry. Do not
                        // fetch or evaluate any later page from this topic this turn.
                        cursors.remove(&batch.topic);
                    }
                }
                Ok::<_, CommitLogError>(())
            })
            .await?;

        Ok(save_remote_commit_log_results)
    }

    /// Open, check, and store each envelope read from a conversation's
    /// commit-log topic, in order, then move the download cursor past the
    /// last one. A discarded envelope is read past and never evaluated again.
    fn save_remote_commit_log_entries_and_update_cursors(
        &self,
        conn: &impl DbQuery,
        group_id: &GroupId,
        keys: &GroupKeys,
        entries: Vec<DecodedCommitLogEntry>,
    ) -> Result<(usize, bool), CommitLogError> {
        // implements: FORK-078
        // Read the durable processed position before loading the tree. Commits
        // and their positions share a transaction, so this tree cannot precede
        // the position against which the hold is checked.
        let processed = conn
            .latest_cursor_for_id(
                group_id,
                &[xmtp_db::refresh_state::EntityKind::ApplicationMessage],
            )?
            .0;
        let (active, publishers, boundaries) =
            crate::state_tx::state_write(self.context.mls_storage(), |tx| {
                tx.with_group(*group_id, |g, storage| {
                    Ok::<_, GroupError>((
                        g.is_active(),
                        ForkActors::from_group(g)?.publisher_keys(g)?,
                        storage.db().get_commit_log_publisher_boundaries(group_id)?,
                    ))
                })
                .map(Continue)
            })?
            .into_continued();
        let mut evaluated_to = None;
        let mut held = false;
        let mut num_entries_saved = 0;
        // From the stored remote commit log, fetch the following info:
        // 1. The latest applied epoch authenticator
        // 2. The latest applied epoch number
        // 3. The latest stored sequence id
        let mut latest_saved_remote_log = conn.get_latest_remote_log_for_group(group_id)?;
        for decoded in &entries {
            let Some(log_entry) = open_remote_entry(keys, group_id, decoded) else {
                evaluated_to = Some(decoded.cursor);
                continue;
            };
            if active && log_entry.commit_sequence_id > processed {
                held = true;
                break;
            }
            evaluated_to = Some(decoded.cursor);
            if !publishers.contains(&log_entry.author_installation_key) {
                continue;
            }
            let chain_holds = latest_saved_remote_log.as_ref().is_some_and(|latest| {
                let boundary = boundaries
                    .get(&latest.author_installation_key)
                    .copied()
                    .unwrap_or(0);
                publishers.contains(&latest.author_installation_key)
                    && !(latest.commit_sequence_id < boundary
                        && log_entry.commit_sequence_id >= boundary as u64)
            });
            if should_skip_remote_commit_log_entry(
                latest_saved_remote_log.as_ref(),
                &log_entry,
                chain_holds,
            ) {
                continue;
            }

            let commit_result = CommitResult::from(
                ProtoCommitResult::try_from(log_entry.commit_result)
                    .unwrap_or(ProtoCommitResult::Unspecified),
            );
            num_entries_saved += 1;
            NewRemoteCommitLog {
                log_sequence_id: decoded.cursor.0 as i64,
                group_id: *group_id,
                commit_sequence_id: log_entry.commit_sequence_id as i64,
                commit_result,
                applied_epoch_number: log_entry.applied_epoch_number as i64,
                applied_epoch_authenticator: log_entry.applied_epoch_authenticator.clone(),
                author_installation_key: log_entry.author_installation_key.clone(),
            }
            .store(conn)?;

            latest_saved_remote_log = Some(RemoteCommitLog {
                rowid: 0,
                log_sequence_id: decoded.cursor.0 as i64,
                group_id: *group_id,
                commit_sequence_id: log_entry.commit_sequence_id as i64,
                commit_result,
                applied_epoch_number: log_entry.applied_epoch_number as i64,
                applied_epoch_authenticator: log_entry.applied_epoch_authenticator,
                author_installation_key: log_entry.author_installation_key,
            });
        }
        if let Some(cursor) = evaluated_to {
            conn.update_cursor(
                group_id,
                xmtp_db::refresh_state::EntityKind::CommitLogDownload,
                cursor,
            )?;
        }

        Ok((num_entries_saved, held))
    }

    // Updates fork status for conversations in the database
    pub async fn update_forked_state(&mut self) -> Result<(), CommitLogError> {
        let conversation_ids_for_forked_state_check =
            self.context.db().get_conversation_ids_for_fork_check()?;

        for conversation_id in conversation_ids_for_forked_state_check {
            let conversation_id = GroupId::try_from(conversation_id)?;
            crate::state_tx::state_write_with_events(
                self.context.mls_storage(),
                self.context.events(),
                |tx, events| {
                    let storage = tx.storage();
                    let db = storage.db();
                    let was_forked = db.get_group_commit_log_forked_status(&conversation_id)?;
                    let is_forked = self.check_conversation_fork_state(&db, &conversation_id)?;
                    // Persist the fork status to the database
                    db.set_group_commit_log_forked_status(&conversation_id, is_forked)?;
                    if was_forked != Some(true) && is_forked == Some(true) {
                        events.emit(
                            Some(ClientEvent::ConversationForkDetected(GroupRef {
                                group_id: conversation_id.to_vec(),
                            })),
                            None,
                        );
                    }
                    Ok::<_, CommitLogError>(Continue(()))
                },
            )
            .map(TransactionOutcome::into_continued)?;
            tokio::task::yield_now().await;
        }

        Ok(())
    }

    /// Explicit request recipients; the one-shot sender also includes its own inbox.
    // implements: FORK-050
    async fn permitted_readders(&self, group_id: &GroupId) -> Result<Vec<String>, CommitLogError> {
        let (group, _) = MlsGroup::new_cached(self.context.clone(), group_id)?;
        Ok(group.with_group_snapshot(|g| {
            Ok::<_, GroupError>(ForkActors::from_group(g)?.request_targets(self.context.inbox_id()))
        })?)
    }

    async fn request_readd(
        &mut self,
        group_info: StoredGroupForReaddRequest,
    ) -> Result<(), CommitLogError> {
        let conn = self.context.db();
        let group_id = group_info.group_id;

        // Check if a readd request has already been sent for this group
        if conn.is_awaiting_readd(&group_id, self.context.installation_id().as_slice())? {
            tracing::debug!(
                group_id = %group_id,
                "Skipping readd request for group because it has already been requested"
            );
            return Ok(());
        }

        tracing::debug!(group_id = %group_id, "Sending readd request");

        // Send the one-shot request to the permitted readders.
        let latest_commit_sequence_id = group_info
            .latest_commit_sequence_id
            .ok_or(CommitLogError::MissingLatestCommitSequenceId { group_id })?;
        let oneshot_message = OneshotMessage {
            message_type: Some(MessageType::ReaddRequest(ReaddRequest {
                group_id: group_id.to_vec(),
                latest_commit_sequence_id: latest_commit_sequence_id as u64,
            })),
        };
        let readders = self.permitted_readders(&group_id).await?;
        tracing::debug!(
            group_id = %group_id,
            "Sending readd request to {:?}",
            readders
        );
        Oneshot::send_message(self.context.clone(), readders, oneshot_message).await?;

        tracing::debug!(group_id = %group_id, "Sent readd request",);

        // Mark readd as requested
        conn.update_requested_at_sequence_id(
            &group_id,
            self.context.installation_id().as_slice(),
            latest_commit_sequence_id,
        )?;

        tracing::debug!(
            group_id = %group_id,
            sequence_id = latest_commit_sequence_id,
            "Updated requested readd sequence id",
        );

        Ok(())
    }

    /// Send readd requests for all forked conversations
    async fn send_outgoing_readd_requests(&mut self) -> Result<(), CommitLogError> {
        if self.context.fork_recovery_opts().enable_recovery_requests == ForkRecoveryPolicy::None {
            return Ok(());
        }
        let conn = self.context.db();

        // Fetch all forked groups with their latest epoch
        let mut forked_groups = conn.get_conversation_ids_for_requesting_readds()?;
        if self.context.fork_recovery_opts().enable_recovery_requests
            == ForkRecoveryPolicy::AllowlistedGroups
        {
            let groups_to_request_recovery = self
                .context
                .fork_recovery_opts()
                .groups_to_request_recovery
                .iter()
                .map(|group_id| group_id.normalize_hex())
                .collect::<HashSet<String>>();
            tracing::info!(
                "Forked groups: {:?}, allowlisted groups for sending recovery requests: {:?}",
                forked_groups
                    .iter()
                    .map(|group_info| group_info.group_id.to_string())
                    .collect::<Vec<String>>(),
                groups_to_request_recovery
            );
            forked_groups.retain(|group_info| {
                groups_to_request_recovery
                    .contains(&group_info.group_id.to_string().normalize_hex())
            });
        }

        // Process groups in order, collecting per-group failures so one transient
        // error doesn't block the rest; a dropped pool still bubbles immediately.
        let mut failures = Vec::new();
        for group_info in forked_groups {
            let group_id = group_info.group_id;
            if let Err(source) = self.request_readd(group_info).await {
                if source.needs_db_reconnect() {
                    return Err(source);
                }
                failures.push(CommitLogError::FailedToSendReadd {
                    group_id,
                    source: Box::new(source),
                });
            }
        }

        if !failures.is_empty() {
            return Err(CommitLogError::FailedReadds { errors: failures });
        }

        Ok(())
    }

    async fn handle_incoming_pending_readds(&self) -> Result<(), CommitLogError> {
        if self.context.fork_recovery_opts().disable_recovery_responses {
            return Ok(());
        }
        let conn = self.context.db();
        let groups_for_readd = conn.get_conversation_ids_for_responding_readds()?;

        // Runs every worker tick (~2s); only speak when there's actually work.
        if groups_for_readd.is_empty() {
            return Ok(());
        }
        tracing::debug!(
            "Processing readd requests for {} groups",
            groups_for_readd.len()
        );

        for group in groups_for_readd {
            match self.validate_pending_readds(&conn, &group).await {
                Ok(validated_installations) => {
                    if validated_installations.is_empty() {
                        continue;
                    }
                    let mls_group = MlsGroup::new(
                        self.context.clone(),
                        group.group_id,
                        group.dm_id.clone(),
                        group.conversation_type,
                        group.created_at_ns,
                    );
                    mls_group
                        .readd_installations(
                            validated_installations.into_iter().collect::<Vec<_>>(),
                        )
                        .await?;
                }
                Err(e) => {
                    // Permanently-bad group: drop its readd statuses and move on.
                    // Retryable failures (incl. a dropped pool) propagate instead.
                    if e.is_retryable() {
                        return Err(e);
                    }
                    tracing::warn!(
                        group_id = %group.group_id,
                        "Deleting readd statuses for group because it failed validation: {}",
                        e
                    );
                    conn.delete_other_readd_statuses(
                        &group.group_id,
                        self.context.installation_id().as_slice(),
                    )?;
                    continue;
                }
            }
        }

        Ok(())
    }

    // implements: FORK-080, FORK-061
    async fn validate_pending_readds(
        &self,
        conn: &impl DbQuery,
        group: &StoredGroupForRespondingReadds,
    ) -> Result<HashSet<Vec<u8>>, CommitLogError> {
        let (mls_group, _) = MlsGroup::new_cached(self.context.clone(), &group.group_id)?;
        tracing::debug!(
            group_id = %mls_group.group_id,
            "Processing readd requests for group"
        );

        mls_group.sync_with_conn().await?;

        if mls_group.consent_state()? != ConsentState::Allowed {
            return Err(CommitLogError::GroupReaddValidationError(
                "Group is not consented".to_string(),
            ));
        }
        if !mls_group.is_active()? {
            return Err(CommitLogError::GroupReaddValidationError(
                "Group is not active".to_string(),
            ));
        }
        let actors =
            mls_group.with_group_snapshot(|g| Ok::<_, GroupError>(ForkActors::from_group(g)?))?;
        if !actors.permits(self.context.inbox_id()) {
            return Err(CommitLogError::GroupReaddValidationError(
                "No longer a permitted readder of group".to_string(),
            ));
        }

        let fork_state = self.check_conversation_fork_state(conn, &mls_group.group_id)?;
        if let Some(true) = fork_state {
            return Err(CommitLogError::GroupReaddValidationError(
                "Group is forked".to_string(),
            ));
        } else if fork_state.is_none() {
            tracing::info!(
                group_id = %mls_group.group_id,
                "Local commit log ahead of remote, skipping group"
            );
            return Ok(HashSet::new());
        }

        let readd_statuses = conn.get_readds_awaiting_response(
            &mls_group.group_id,
            self.context.installation_id().as_slice(),
        )?;
        let mut unverified = readd_statuses
            .iter()
            .map(|readd_status| readd_status.installation_id.clone())
            .collect::<HashSet<_>>();

        let (unverified, verified) = mls_group.with_group_snapshot(|openmls_group| {
            let mut verified = HashSet::new();
            for member in openmls_group.members() {
                if unverified.contains(&member.signature_key) {
                    unverified.remove(&member.signature_key);
                    verified.insert(member.signature_key);
                }
            }
            Ok::<_, GroupError>((unverified, verified))
        })?;
        tracing::debug!(
            group_id = %mls_group.group_id,
            "{} readd requests were for non-members, while {} were for members",
            unverified.len(),
            verified.len()
        );
        conn.delete_readd_statuses(&mls_group.group_id, unverified)?;

        let processed = conn
            .latest_cursor_for_id(
                mls_group.group_id,
                &[xmtp_db::refresh_state::EntityKind::ApplicationMessage],
            )?
            .0;
        // Unsigned wire claims above i64::MAX are stored as negative values.
        // Keep them pending instead of treating them as an earlier position.
        Ok(readd_statuses
            .into_iter()
            .filter(|status| {
                verified.contains(&status.installation_id)
                    && status.requested_at_sequence_id.is_some_and(|requested| {
                        u64::try_from(requested).is_ok_and(|requested| requested <= processed)
                    })
            })
            .map(|status| status.installation_id)
            .collect())
    }

    fn check_conversation_fork_state(
        &self,
        conn: &impl DbQuery,
        conversation_id: &GroupId,
    ) -> Result<Option<bool>, CommitLogError> {
        // Get cursors for this conversation
        let fork_check_local_cursor = conn.get_last_cursor(
            conversation_id,
            xmtp_db::refresh_state::EntityKind::CommitLogForkCheckLocal,
        )?;
        let fork_check_remote_cursor = conn.get_last_cursor(
            conversation_id,
            xmtp_db::refresh_state::EntityKind::CommitLogForkCheckRemote,
        )?;

        // Chain-start anchor: rows with `commit_sequence_id == 0` (Welcome /
        // GroupCreation / BackupRestore) mark the beginning of this member's
        // current membership session — e.g. rejoining via a new welcome after
        // having been removed. History before the latest chain start is not
        // attestable by this member and must not be compared against remote
        // consensus. Those rows are filtered out of
        // `get_local_commit_log_after_cursor`, so the anchor is looked up
        // separately and applied as a floor on the local fork-check cursor.
        let mut local_cursor = fork_check_local_cursor.0 as i64;
        let mut crossed_chain_start = false;
        if let Some(anchor_rowid) = conn.get_latest_chain_start_rowid(conversation_id)?
            && anchor_rowid as i64 > local_cursor
        {
            local_cursor = anchor_rowid as i64;
            crossed_chain_start = true;
            conn.update_cursor(
                conversation_id,
                xmtp_db::refresh_state::EntityKind::CommitLogForkCheckLocal,
                Cursor(anchor_rowid as u64),
            )?;
        }

        // Get local and remote commit logs
        let local_logs = conn.get_local_commit_log_after_cursor(
            conversation_id,
            local_cursor,
            LocalCommitLogOrder::DescendingByRowid,
        )?;
        let remote_logs = conn.get_remote_commit_log_after_cursor(
            conversation_id,
            fork_check_remote_cursor.0 as i64,
            RemoteCommitLogOrder::DescendingByRowid,
        )?;

        // If there are no new commits to check, preserve the existing fork status
        if local_logs.is_empty() {
            if crossed_chain_start {
                // A new chain start (e.g. a welcome from being re-added)
                // invalidates any previously computed status: nothing after
                // it has been verified against remote consensus yet.
                return Ok(None);
            }
            return Ok(conn.get_group_commit_log_forked_status(conversation_id)?);
        }

        let mut is_remote_log_up_to_date = true;
        // Check each local log against remote logs for matching commit_sequence_id
        for local_log in &local_logs {
            // Terminal removal marker: a commit that removed us merges only
            // the public diff — we cannot derive the new epoch's secrets, so
            // a Success row whose applied authenticator equals its last
            // authenticator means "this commit removed us", not a fork. The
            // remaining members publish the real post-commit authenticator,
            // which can never match ours, so this row must be excluded from
            // comparison. (Covers both new RemovedFromGroup rows and legacy
            // rows that recorded the new epoch with the stale authenticator.)
            if local_log.commit_result == CommitResult::Success
                && local_log.applied_epoch_authenticator == local_log.last_epoch_authenticator
            {
                // Note: `update_cursor` is monotonic (it only ever moves the
                // cursor forward), so the cursor updates below for older
                // matched rows — the loop walks descending rowids — cannot
                // regress the cursor behind this terminal row.
                conn.update_cursor(
                    conversation_id,
                    xmtp_db::refresh_state::EntityKind::CommitLogForkCheckLocal,
                    Cursor(local_log.rowid as u64),
                )?;
                continue;
            }

            let Some(matching_remote_log) =
                self.find_matching_remote_log(&remote_logs, local_log.commit_sequence_id)
            else {
                is_remote_log_up_to_date = false;
                continue;
            };
            // Found a matching commit_sequence_id - check if forked
            let is_mismatched = local_log.applied_epoch_authenticator
                != matching_remote_log.applied_epoch_authenticator;

            if is_mismatched {
                // No backend sequence id: with the group id it would link the
                // conversation to its commit-log topic.
                tracing::warn!(
                    group_id = %conversation_id.as_slice().snippet(),
                    commit_sequence_id = local_log.commit_sequence_id,
                    "Detected forked state: the local and remote epoch authenticators differ"
                );
            }

            // Update cursors regardless of fork status (we found a match)
            conn.update_cursor(
                conversation_id,
                xmtp_db::refresh_state::EntityKind::CommitLogForkCheckLocal,
                Cursor(local_log.rowid as u64),
            )?;
            conn.update_cursor(
                conversation_id,
                xmtp_db::refresh_state::EntityKind::CommitLogForkCheckRemote,
                Cursor(matching_remote_log.rowid as u64),
            )?;

            if is_mismatched {
                return Ok(Some(true));
            } else if is_remote_log_up_to_date {
                return Ok(Some(false));
            } else {
                // If we haven't verified the latest commit local commit logs, we
                // don't know if we are forked or not
                return Ok(None);
            }
        }

        Ok(None)
    }

    fn find_matching_remote_log<'a>(
        &self,
        remote_logs: &'a [xmtp_db::remote_commit_log::RemoteCommitLog],
        commit_sequence_id: i64,
    ) -> Option<&'a xmtp_db::remote_commit_log::RemoteCommitLog> {
        remote_logs
            .iter()
            .find(|remote_log| remote_log.commit_sequence_id == commit_sequence_id)
    }
}

/// Open one envelope read from a conversation's commit-log topic. `None`
/// discards it; the reason is logged without entry bytes. The log names the
/// envelope by its commit-log id and sequence id only, never by group id, so
/// it does not link the conversation to its commit-log topic.
// implements: FORK-077
fn open_remote_entry(
    keys: &GroupKeys,
    group_id: &GroupId,
    decoded: &DecodedCommitLogEntry,
) -> Option<PlaintextCommitLogEntry> {
    let commit_log_id = keys.commit_log_id();
    let Some(sealed) = &decoded.sealed else {
        tracing::warn!(
            commit_log_id = %commit_log_id.as_slice().snippet(),
            sequence_id = decoded.cursor.0,
            "Discarding a commit log envelope that holds no sealed entry"
        );
        return None;
    };
    open_entry(keys, group_id.as_slice(), decoded.cursor.0, sealed)
        .inspect_err(|reason| {
            tracing::warn!(
                commit_log_id = %commit_log_id.as_slice().snippet(),
                sequence_id = decoded.cursor.0,
                %reason,
                "Discarding a commit log entry"
            );
        })
        .ok()
}

// If the last accepted author lost its publisher role or the entry crosses
// that author's publisher boundary, only rule 1 applies.
// Otherwise an opened entry is skipped if:
// 1. The commit_sequence_id of the entry is not greater than the most recently stored entry, if one exists.
// 2. The last_epoch_authenticator does not match the epoch_authenticator of the most recently stored entry with a CommitResult of COMMIT_RESULT_APPLIED, if one exists.
// 3. The entry has a CommitResult of COMMIT_RESULT_APPLIED, but the epoch number is not exactly 1 greater than the most recently stored entry with a result of COMMIT_RESULT_APPLIED, if one exists.
// 4. The entry CommitResult is not COMMIT_RESULT_APPLIED, and the epoch authenticator or epoch number does not match the most recently applied values
// implements: FORK-083
pub(crate) fn should_skip_remote_commit_log_entry(
    latest_saved_remote_log: Option<&RemoteCommitLog>,
    entry: &PlaintextCommitLogEntry,
    chain_holds: bool,
) -> bool {
    let Some(latest_saved_remote_log) = latest_saved_remote_log else {
        return false;
    };

    if !chain_holds {
        return entry.commit_sequence_id <= latest_saved_remote_log.commit_sequence_id as u64;
    }
    let is_applied = entry.commit_result == ProtoCommitResult::Applied as i32;

    entry.commit_sequence_id <= latest_saved_remote_log.commit_sequence_id as u64
        || (is_applied
            && !latest_saved_remote_log
                .applied_epoch_authenticator
                .is_empty()
            && entry.last_epoch_authenticator
                != latest_saved_remote_log.applied_epoch_authenticator)
        || (is_applied
            && entry.applied_epoch_number as i64
                != latest_saved_remote_log.applied_epoch_number + 1)
        || (!is_applied
            && (entry.applied_epoch_authenticator
                != latest_saved_remote_log.applied_epoch_authenticator
                || entry.applied_epoch_number as i64
                    != latest_saved_remote_log.applied_epoch_number))
}

// Helper that exposes private methods for testing
#[cfg(test)]
impl<Context> CommitLogWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    pub async fn _tick(&mut self) -> Result<(), CommitLogError> {
        self.tick().await
    }

    /// The worker's loop, as the supervisor runs it.
    pub async fn _run(&mut self) -> Result<(), CommitLogError> {
        self.run().await
    }

    /// Pass every seal result through `fault`.
    pub(crate) fn with_seal_fault(mut self, fault: SealFault) -> Self {
        self.seal_fault = Some(fault);
        self
    }

    // Test helper to get fork status for all groups that would be checked (for backward compatibility with tests)
    pub fn get_all_fork_statuses(&self) -> Result<HashMap<Vec<u8>, Option<bool>>, CommitLogError> {
        use xmtp_db::group::GroupQueryArgs;
        let conn = &self.context.db();
        // Get all groups (not just those with commit log keys)
        let all_groups = conn.find_groups(GroupQueryArgs::default())?;

        let mut results = HashMap::new();
        for group in all_groups {
            let fork_status = conn.get_group_commit_log_forked_status(&group.id)?;
            results.insert(group.id.to_vec(), fork_status);
        }

        Ok(results)
    }

    /// Test-only version that runs without infinite loop
    pub async fn run_test(
        &mut self,
        commit_log_test_function: CommitLogTestFunction,
        iterations: Option<usize>,
    ) -> Result<Vec<TestResult>, CommitLogError> {
        let mut test_results = Vec::new();
        match iterations {
            Some(n) => {
                // Run exactly n times
                for _ in 0..n {
                    let test_result = self.test_helper(&commit_log_test_function).await?;
                    test_results.push(test_result);
                }
            }
            None => {
                let test_result = self.test_helper(&commit_log_test_function).await?;
                test_results.push(test_result);
            }
        }
        Ok(test_results)
    }

    async fn test_helper(
        &mut self,
        commit_log_test_function: &CommitLogTestFunction,
    ) -> Result<TestResult, CommitLogError> {
        let mut test_result = TestResult {
            save_remote_commit_log_results: None,
            publish_commit_log_results: None,
            is_forked: None,
        };
        match commit_log_test_function {
            CommitLogTestFunction::PublishCommitLogsToRemote => {
                let publish_commit_log_results = self.publish_commit_logs_to_remote().await?;
                test_result.publish_commit_log_results = Some(publish_commit_log_results);
            }
            CommitLogTestFunction::SaveRemoteCommitLog => {
                let save_remote_commit_log_results = self.save_remote_commit_log().await?;
                test_result.save_remote_commit_log_results = Some(save_remote_commit_log_results);
            }
            CommitLogTestFunction::CheckForkedState => {
                self.update_forked_state().await?;
                let is_forked = self.get_all_fork_statuses()?;
                test_result.is_forked = Some(is_forked);
            }
            CommitLogTestFunction::All => {
                // Order is save; update fork status; publish
                let save_remote_commit_log_results = self.save_remote_commit_log().await?;
                test_result.save_remote_commit_log_results = Some(save_remote_commit_log_results);
                self.update_forked_state().await?;
                let is_forked = self.get_all_fork_statuses()?;
                test_result.is_forked = Some(is_forked);
                let publish_commit_log_results = self.publish_commit_logs_to_remote().await?;
                test_result.publish_commit_log_results = Some(publish_commit_log_results);
            }
        }
        Ok(test_result)
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
