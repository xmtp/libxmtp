use futures::StreamExt;
use prost::Message;
use std::{collections::HashMap, sync::Arc, time::Duration};
use thiserror::Error;
use tokio::sync::OnceCell;
use xmtp_api::ApiError;
use xmtp_db::{
    remote_commit_log::{self, CommitResult, RemoteCommitLog},
    DbConnection, StorageError, Store, XmtpDb,
};
use xmtp_proto::xmtp::mls::message_contents::CommitResult as ProtoCommitResult;
use xmtp_proto::{
    api_client::trait_impls::XmtpApi,
    mls_v1::{PagingInfo, QueryCommitLogRequest, QueryCommitLogResponse},
    xmtp::{message_api::v1::SortDirection, mls::message_contents::PlaintextCommitLogEntry},
};

use crate::{
    context::{XmtpContextProvider, XmtpMlsLocalContext, XmtpSharedContext},
    worker::{BoxedWorker, NeedsDbReconnect, Worker, WorkerFactory, WorkerKind, WorkerResult},
};

/// Interval at which the CommitLogWorker runs to publish commit log entries.
pub const INTERVAL_DURATION: Duration = Duration::from_secs(5);

#[derive(Clone)]
pub struct Factory<ApiClient, Db> {
    context: Arc<XmtpMlsLocalContext<ApiClient, Db>>,
}

impl<ApiClient, Db> WorkerFactory for Factory<ApiClient, Db>
where
    ApiClient: XmtpApi + 'static,
    Db: XmtpDb + 'static,
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
impl<ApiClient, Db> Worker for CommitLogWorker<ApiClient, Db>
where
    ApiClient: XmtpApi + 'static,
    Db: XmtpDb + 'static + Send,
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
        C: XmtpSharedContext,
        <C as XmtpSharedContext>::Db: 'static,
        <C as XmtpSharedContext>::ApiClient: 'static,
    {
        let context = context.context_ref().clone();
        Factory { context }
    }
}

pub struct CommitLogWorker<ApiClient, Db> {
    context: Arc<XmtpMlsLocalContext<ApiClient, Db>>,
    #[allow(dead_code)]
    init: OnceCell<()>,
}

impl<ApiClient, Db> CommitLogWorker<ApiClient, Db>
where
    ApiClient: XmtpApi + 'static,
    Db: XmtpDb + 'static,
{
    pub fn new(context: Arc<XmtpMlsLocalContext<ApiClient, Db>>) -> Self {
        Self {
            context,
            init: OnceCell::new(),
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct PublishCommitLogsResult {
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
    pub publish_commit_log_results: Option<Vec<PublishCommitLogsResult>>,
}

impl<ApiClient, Db> CommitLogWorker<ApiClient, Db>
where
    ApiClient: XmtpApi + 'static,
    Db: XmtpDb + 'static,
{
    async fn run(&mut self) -> Result<(), CommitLogError> {
        let mut intervals = xmtp_common::time::interval_stream(INTERVAL_DURATION);
        while (intervals.next().await).is_some() {
            let provider = self.context.mls_provider();
            let conn = provider.db();
            self.publish_commit_logs_to_remote(conn).await?;
            self.save_remote_commit_log(conn).await?;
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
        let provider = self.context.mls_provider();
        let conn = provider.db();
        let mut test_result = TestResult {
            save_remote_commit_log_results: None,
            publish_commit_log_results: None,
        };
        match commit_log_test_function {
            CommitLogTestFunction::PublishCommitLogsToRemote => {
                let publish_commit_log_results = self.publish_commit_logs_to_remote(conn).await?;
                test_result.publish_commit_log_results = Some(publish_commit_log_results);
            }
            CommitLogTestFunction::SaveRemoteCommitLog => {
                let save_remote_commit_log_results = self.save_remote_commit_log(conn).await?;
                test_result.save_remote_commit_log_results = Some(save_remote_commit_log_results);
            }
            CommitLogTestFunction::All => {
                let publish_commit_log_results = self.publish_commit_logs_to_remote(conn).await?;
                test_result.publish_commit_log_results = Some(publish_commit_log_results);
                let save_remote_commit_log_results = self.save_remote_commit_log(conn).await?;
                test_result.save_remote_commit_log_results = Some(save_remote_commit_log_results);
            }
        }
        Ok(test_result)
    }

    async fn publish_commit_logs_to_remote(
        &mut self,
        conn: &DbConnection<<Db as XmtpDb>::Connection>,
    ) -> Result<Vec<PublishCommitLogsResult>, CommitLogError> {
        // Step 1 is to get the list of all group_id for dms and for groups where we are a super admin
        let conversation_ids_for_remote_log_publish =
            conn.get_conversation_ids_for_remote_log_publish()?;

        // Step 2 is to map the cursor positions we should publish from, for each conversation
        let conversation_cursor_map = self
            .map_conversation_to_commit_log_cursor(conn, &conversation_ids_for_remote_log_publish);

        // Step 3 is to publish any new local commit logs and to update relevant cursors
        let api = self.context.api();
        let mut commit_log_results = Vec::new();
        for (conversation_id, published_commit_log_cursor) in conversation_cursor_map {
            if let Some(published_commit_log_cursor) = published_commit_log_cursor {
                // Local commit log entries are returned sorted in ascending order of `commit_sequence_id`
                // All local commit log will have rowid > 0 since sqlite rowid starts at 1 https://www.sqlite.org/autoinc.html
                let (plaintext_commit_log_entries, rowids): (
                    Vec<PlaintextCommitLogEntry>,
                    Vec<i32>,
                ) = conn
                    .get_group_logs_for_publishing(&conversation_id, published_commit_log_cursor)?
                    .iter()
                    .map(|log| (PlaintextCommitLogEntry::from(log), log.rowid))
                    .unzip();

                let max_rowid = rowids.into_iter().max().unwrap_or_else(|| {
                    tracing::warn!(
                        "No rowids found for conversation {:?}, using 0 as cursor",
                        conversation_id
                    );
                    0
                });
                // Publish commit log entries to the API
                match api.publish_commit_log(&plaintext_commit_log_entries).await {
                    Ok(_) => {
                        if let Some(last_entry) = plaintext_commit_log_entries.last() {
                            // If publish is successful, update the cursor to the last entry's `commit_sequence_id`
                            conn.update_cursor(
                                &conversation_id,
                                xmtp_db::refresh_state::EntityKind::CommitLogUpload,
                                max_rowid as i64,
                            )?;
                            commit_log_results.push(PublishCommitLogsResult {
                                conversation_id,
                                num_entries_published: plaintext_commit_log_entries.len(),
                                last_entry_published_sequence_id: last_entry.commit_sequence_id
                                    as i64,
                                last_entry_published_rowid: max_rowid as i64,
                            });
                        } else {
                            tracing::error!(
                                "No last entry found for conversation id: {:?}",
                                conversation_id
                            );
                        }
                    }
                    Err(e) => {
                        // In this case we do not update the cursor, so next worker iteration will try again
                        tracing::error!("Failed to publish commit log entries to remote commit log for conversation id: {:?}, error: {:?}", conversation_id, e);
                    }
                }
            }
        }
        Ok(commit_log_results)
    }

    // Check if for each `conversation_id` whether its `PublishedCommitLog` cursor is lower than the local commit log sequence id.
    //  If so - map to the `PublishedCommitLog` cursor in `cursor_map`, otherwise map to None
    fn map_conversation_to_commit_log_cursor(
        &self,
        conn: &DbConnection<<Db as XmtpDb>::Connection>,
        conversation_ids: &[Vec<u8>],
    ) -> HashMap<Vec<u8>, Option<i64>> {
        let mut cursor_map: HashMap<Vec<u8>, Option<i64>> = HashMap::new();
        for conversation_id in conversation_ids {
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

            if local_commit_log_cursor as i64 > published_commit_log_cursor {
                // We have new commits that have not been published to remote commit log yet
                cursor_map.insert(conversation_id.to_vec(), Some(published_commit_log_cursor));
            } else {
                cursor_map.insert(conversation_id.to_vec(), None); // Remote log is up to date with local commit log
            }
        }
        cursor_map
    }

    async fn save_remote_commit_log(
        &mut self,
        conn: &DbConnection<<Db as XmtpDb>::Connection>,
    ) -> Result<Vec<SaveRemoteCommitLogResult>, CommitLogError> {
        // This should be all groups we are in, and all dms are in except sync groups
        let conversation_ids_for_remote_log_download =
            conn.get_conversation_ids_for_remote_log_download()?;

        // Step 1 is to collect a list of remote log cursors for all conversations and convert them into query log requests
        let remote_log_cursors =
            conn.get_remote_log_cursors(conversation_ids_for_remote_log_download.as_slice())?;
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
        conn: &DbConnection<<Db as XmtpDb>::Connection>,
        commit_log_response: QueryCommitLogResponse,
    ) -> Result<UpdateCursorsResult, CommitLogError> {
        let group_id = commit_log_response.group_id;
        let mut latest_download_cursor = 0;
        let mut latest_sequence_id = 0;
        for entry in commit_log_response.commit_log_entries {
            // TODO(cam): we will have to decrypt here
            let log_entry =
                PlaintextCommitLogEntry::decode(entry.encrypted_commit_log_entry.as_slice())?;
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
