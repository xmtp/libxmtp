use futures::StreamExt;
use prost::Message;
use std::time::Duration;
use thiserror::Error;
use xmtp_api::ApiError;
use xmtp_db::{
    prelude::*,
    remote_commit_log::{self, CommitResult, RemoteCommitLog},
    DbQuery, StorageError, Store,
};
use xmtp_proto::xmtp::mls::message_contents::CommitResult as ProtoCommitResult;
use xmtp_proto::{
    mls_v1::{PagingInfo, QueryCommitLogRequest, QueryCommitLogResponse},
    xmtp::{message_api::v1::SortDirection, mls::message_contents::PlaintextCommitLogEntry},
};

use crate::{
    context::XmtpSharedContext,
    worker::{BoxedWorker, NeedsDbReconnect, Worker, WorkerFactory, WorkerKind, WorkerResult},
};

/// Interval at which the CommitLogWorker runs to publish commit log entries.
pub const INTERVAL_DURATION: Duration = Duration::from_secs(60 * 5); // 5 minutes

#[derive(Clone)]
pub struct Factory<Context> {
    context: Context,
}

impl<Context> WorkerFactory for Factory<Context>
where
    Context: XmtpSharedContext + Send + Sync + 'static,
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
    #[error("generic api error: {0}")]
    Api(#[from] ApiError),
    #[error("connection error: {0}")]
    Connection(#[from] xmtp_db::ConnectionError),
    #[error("prost decode error: {0}")]
    Prost(#[from] prost::DecodeError),
}

impl NeedsDbReconnect for CommitLogError {
    fn needs_db_reconnect(&self) -> bool {
        match self {
            Self::Storage(s) => s.db_needs_connection(),
            Self::Api(_api_error) => false,
            Self::Connection(_connection_error) => true, // TODO(cam): verify this is correct
            Self::Prost(_prost_error) => false,
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
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
        Self: Sized,
        C: XmtpSharedContext + Send + Sync + 'static,
    {
        Factory { context }
    }
}

pub struct CommitLogWorker<Context> {
    context: Context,
}

impl<Context> CommitLogWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    pub fn new(context: Context) -> Self {
        Self { context }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct ConversationCursorInfo {
    pub conversation_id: Vec<u8>,
    pub num_entries_published: usize,
    pub last_entry_published_sequence_id: i64,
    pub last_entry_published_rowid: i64,
}

#[derive(Debug, PartialEq, Clone)]
pub struct SaveRemoteCommitLogResult {
    pub conversation_id: Vec<u8>,
    pub num_entries_saved: usize,
    pub last_entry_saved_commit_sequence_id: i64,
    pub last_entry_saved_remote_log_sequence_id: i64,
}

pub struct UpdateCursorsResult {
    pub conversation_id: Vec<u8>,
    pub last_entry_saved_commit_sequence_id: i64,
    pub last_entry_saved_remote_log_sequence_id: i64,
}

#[cfg(test)]
pub struct TestResult {
    pub save_remote_commit_log_results: Option<Vec<SaveRemoteCommitLogResult>>,
    pub publish_commit_log_results: Option<Vec<ConversationCursorInfo>>,
}

impl<Context> CommitLogWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    async fn run(&mut self) -> Result<(), CommitLogError> {
        let mut intervals = xmtp_common::time::interval_stream(INTERVAL_DURATION);
        while (intervals.next().await).is_some() {
            self.publish_commit_logs_to_remote().await?;
            self.save_remote_commit_log().await?;
        }
        Ok(())
    }

    /// Test-only version that runs without infinite loop
    #[cfg(test)]
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

    #[cfg(test)]
    async fn test_helper(
        &mut self,
        commit_log_test_function: &CommitLogTestFunction,
    ) -> Result<TestResult, CommitLogError> {
        let mut test_result = TestResult {
            save_remote_commit_log_results: None,
            publish_commit_log_results: None,
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
            CommitLogTestFunction::All => {
                let publish_commit_log_results = self.publish_commit_logs_to_remote().await?;
                test_result.publish_commit_log_results = Some(publish_commit_log_results);
                let save_remote_commit_log_results = self.save_remote_commit_log().await?;
                test_result.save_remote_commit_log_results = Some(save_remote_commit_log_results);
            }
        }
        Ok(test_result)
    }

    async fn publish_commit_logs_to_remote(
        &mut self,
    ) -> Result<Vec<ConversationCursorInfo>, CommitLogError> {
        let conn = &self.context.db();
        // Step 1 is to get the list of all group_id for dms and for groups where we are a super admin
        let conversation_ids_for_remote_log_publish =
            conn.get_conversation_ids_for_remote_log_publish()?;

        // Step 2 is to prepare commit log entries for publishing along with the updated cursor for each conversation on publication success
        let (conversation_cursor_info, all_plaintext_entries) =
            self.prepare_publish_commit_log_info(conn, &conversation_ids_for_remote_log_publish)?;

        // Step 3 is to publish commit log entries to the API and update cursors
        let api = self.context.api();
        match api.publish_commit_log(&all_plaintext_entries).await {
            Ok(_) => {
                // Publishing was successful, let's update every group's cursor
                for conversation_cursor_info in &conversation_cursor_info {
                    conn.update_cursor(
                        &conversation_cursor_info.conversation_id,
                        xmtp_db::refresh_state::EntityKind::CommitLogUpload,
                        conversation_cursor_info.last_entry_published_rowid,
                    )?;
                }
            }
            Err(e) => {
                // In this case we do not update the cursor, so next worker iteration will try again
                tracing::error!(
                    "Failed to publish commit log entries to remote commit log, error: {:?}",
                    e
                );
            }
        }
        Ok(conversation_cursor_info)
    }

    // Check each `conversation_id` for new commit log entries. Return a combined list of all entries for batch publishing,
    // along with the new cursor for each conversation on publication success
    fn prepare_publish_commit_log_info(
        &self,
        conn: &impl DbQuery,
        conversation_ids: &[Vec<u8>],
    ) -> Result<(Vec<ConversationCursorInfo>, Vec<PlaintextCommitLogEntry>), CommitLogError> {
        let mut conversation_cursor_info: Vec<ConversationCursorInfo> = Vec::new();
        let mut all_plaintext_entries = Vec::new();
        for conversation_id in conversation_ids {
            // Step 1: Check each conversation cursors to see if we have new commits that have not been published to remote commit log yet
            let local_commit_log_cursor = conn
                .get_local_commit_log_cursor(conversation_id)
                .ok()
                .flatten()
                .unwrap_or(0);
            let published_commit_log_cursor = conn
                .get_last_cursor_for_id(
                    conversation_id,
                    xmtp_db::refresh_state::EntityKind::CommitLogUpload,
                )
                .unwrap_or(0);

            if local_commit_log_cursor as i64 <= published_commit_log_cursor {
                // We have no new commits to publish for this conversation
                continue;
            }

            // Step 2: collect all the commit log entries for this conversation
            // Local commit log entries are returned sorted in ascending order of `rowid`
            // All local commit log will have rowid > 0 since sqlite rowid starts at 1 https://www.sqlite.org/autoinc.html
            let (plaintext_commit_log_entries, rowids): (Vec<PlaintextCommitLogEntry>, Vec<i32>) =
                conn.get_group_logs_for_publishing(conversation_id, published_commit_log_cursor)?
                    .iter()
                    .map(|log| (PlaintextCommitLogEntry::from(log), log.rowid))
                    .unzip();

            // Step 3: Compile the conversation cursor info and all the commit log entries for this conversation
            if let Some(max_rowid) = rowids.into_iter().last() {
                conversation_cursor_info.push(ConversationCursorInfo {
                    conversation_id: conversation_id.clone(),
                    num_entries_published: plaintext_commit_log_entries.len(),
                    last_entry_published_sequence_id: plaintext_commit_log_entries
                        .last()
                        .map(|e| e.commit_sequence_id as i64)
                        .unwrap_or(0),
                    last_entry_published_rowid: max_rowid as i64,
                });
                all_plaintext_entries.extend(plaintext_commit_log_entries);
            }
        }
        Ok((conversation_cursor_info, all_plaintext_entries))
    }

    async fn save_remote_commit_log(
        &mut self,
    ) -> Result<Vec<SaveRemoteCommitLogResult>, CommitLogError> {
        let conn = &self.context.db();
        // This should be all groups we are in, and all dms are in except sync groups
        let conversation_ids_for_remote_log_download =
            conn.get_conversation_ids_for_remote_log_download()?;

        // Step 1 is to collect a list of remote log cursors for all conversations and convert them into query log requests
        let remote_log_cursors =
            conn.get_remote_log_cursors(conversation_ids_for_remote_log_download.as_slice())?;
        // For now we will rely on next iteration of the worker to download the next batch of commit log entries
        // if there is more than MAX_PAGE_SIZE entries to download per group
        let query_log_requests: Vec<QueryCommitLogRequest> = remote_log_cursors
            .iter()
            .map(|(conversation_id, cursor)| QueryCommitLogRequest {
                group_id: conversation_id.clone(),
                paging_info: Some(PagingInfo {
                    direction: SortDirection::Ascending as i32,
                    id_cursor: *cursor as u64,
                    limit: remote_commit_log::MAX_PAGE_SIZE,
                }),
            })
            .collect();

        // Step 2 execute the api call to query remote commit log entries
        let api = self.context.api();
        let query_commit_log_responses = api.query_commit_log(query_log_requests).await?;

        // Step 3 save the remote commit log entries to the local commit log
        let mut save_remote_commit_log_results = Vec::new();
        for response in query_commit_log_responses {
            let num_entries = response.commit_log_entries.len();
            let group_id = response.group_id.clone();
            let update_cursors_result =
                self.save_remote_commit_log_entries_and_update_cursors(conn, response)?;
            save_remote_commit_log_results.push(SaveRemoteCommitLogResult {
                conversation_id: group_id,
                num_entries_saved: num_entries,
                last_entry_saved_commit_sequence_id: update_cursors_result
                    .last_entry_saved_commit_sequence_id,
                last_entry_saved_remote_log_sequence_id: update_cursors_result
                    .last_entry_saved_remote_log_sequence_id,
            });
        }

        Ok(save_remote_commit_log_results)
    }

    fn save_remote_commit_log_entries_and_update_cursors(
        &self,
        conn: &impl DbQuery,
        commit_log_response: QueryCommitLogResponse,
    ) -> Result<UpdateCursorsResult, CommitLogError> {
        let group_id = commit_log_response.group_id;
        let mut latest_download_cursor = 0;
        let mut latest_sequence_id = 0;
        for entry in commit_log_response.commit_log_entries {
            // TODO(cam): we will have to decrypt here
            let log_entry =
                PlaintextCommitLogEntry::decode(entry.serialized_commit_log_entry.as_slice())?;
            RemoteCommitLog {
                log_sequence_id: entry.sequence_id as i64,
                group_id: log_entry.group_id,
                commit_sequence_id: log_entry.commit_sequence_id as i64,
                commit_result: CommitResult::from(
                    ProtoCommitResult::try_from(log_entry.commit_result)
                        .unwrap_or(ProtoCommitResult::Unspecified),
                ),
                applied_epoch_number: Some(log_entry.applied_epoch_number as i64),
                applied_epoch_authenticator: Some(log_entry.applied_epoch_authenticator),
            }
            .store(conn)?;
            if entry.sequence_id > latest_download_cursor {
                latest_download_cursor = entry.sequence_id;
            }
            if log_entry.commit_sequence_id > latest_sequence_id {
                latest_sequence_id = log_entry.commit_sequence_id;
            }
        }
        conn.update_cursor(
            &group_id,
            xmtp_db::refresh_state::EntityKind::CommitLogDownload,
            latest_download_cursor as i64,
        )?;
        Ok(UpdateCursorsResult {
            conversation_id: group_id,
            last_entry_saved_commit_sequence_id: latest_sequence_id as i64,
            last_entry_saved_remote_log_sequence_id: latest_download_cursor as i64,
        })
    }
}

#[cfg(test)]
pub enum CommitLogTestFunction {
    PublishCommitLogsToRemote,
    SaveRemoteCommitLog,
    All,
}
