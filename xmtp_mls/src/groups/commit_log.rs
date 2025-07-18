use futures::StreamExt;
use std::{collections::HashMap, sync::Arc, time::Duration};
use thiserror::Error;
use tokio::sync::OnceCell;
use xmtp_api::ApiError;
use xmtp_db::{DbConnection, StorageError, XmtpDb};
use xmtp_proto::{
    api_client::trait_impls::XmtpApi, xmtp::mls::message_contents::PlaintextCommitLogEntry,
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
}

impl NeedsDbReconnect for CommitLogError {
    fn needs_db_reconnect(&self) -> bool {
        match self {
            Self::Storage(s) => s.db_needs_connection(),
            Self::Api(_api_error) => false,
            Self::Connection(_connection_error) => true, // TODO(cam): verify this is correct
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

impl<ApiClient, Db> CommitLogWorker<ApiClient, Db>
where
    ApiClient: XmtpApi + 'static,
    Db: XmtpDb + 'static,
{
    async fn run(&mut self) -> Result<(), CommitLogError> {
        let mut intervals = xmtp_common::time::interval_stream(INTERVAL_DURATION);
        while (intervals.next().await).is_some() {
            self.publish_commit_logs_to_remote().await?;
        }
        Ok(())
    }

    /// Test-only version that runs without infinite loop
    #[cfg(test)]
    pub async fn run_test(&mut self, iterations: Option<usize>) -> Result<(), CommitLogError> {
        match iterations {
            Some(n) => {
                // Run exactly n times
                for _ in 0..n {
                    self.publish_commit_logs_to_remote().await?;
                }
            }
            None => {
                // Run once
                self.publish_commit_logs_to_remote().await?;
            }
        }
        Ok(())
    }

    async fn publish_commit_logs_to_remote(&mut self) -> Result<(), CommitLogError> {
        let provider = self.context.mls_provider();
        let conn = provider.db();

        // Step 1 is to get the list of all group_id for dms and for groups where we are a super admin
        let conversation_ids_for_remote_log = conn.get_conversation_ids_for_remote_log()?;

        // Step 2 is to map the cursor positions we should publish from, for each conversation
        let conversation_cursor_map =
            self.map_conversation_to_commit_log_cursor(conn, conversation_ids_for_remote_log);

        // Step 3 is to publish any new local commit logs and to update relevant cursors
        let api = self.context.api();
        for (conversation_id, published_commit_log_cursor) in conversation_cursor_map {
            if let Some(published_commit_log_cursor) = published_commit_log_cursor {
                // Local commit log entries are returned sorted in ascending order of `commit_sequence_id`
                let plaintext_commit_log_entries: Vec<PlaintextCommitLogEntry> = conn
                    .get_group_logs_after_cursor(&conversation_id, published_commit_log_cursor)?
                    .iter()
                    .map(PlaintextCommitLogEntry::from)
                    .collect();
                // Publish commit log entries to the API
                match api.publish_commit_log(&plaintext_commit_log_entries).await {
                    Ok(_) => {
                        if let Some(last_entry) = plaintext_commit_log_entries.last() {
                            // If publish is successful, update the cursor to the last entry's `commit_sequence_id`
                            conn.update_cursor(
                                &conversation_id,
                                xmtp_db::refresh_state::EntityKind::PublishedCommitLog,
                                last_entry.commit_sequence_id as i64,
                            )?;
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
        Ok(())
    }

    // Check if for each `conversation_id` whether its `PublishedCommitLog` cursor is lower than the local commit log sequence id.
    //  If so - map to the `PublishedCommitLog` cursor in `cursor_map`, otherwise map to None
    fn map_conversation_to_commit_log_cursor(
        &self,
        conn: &DbConnection<<Db as XmtpDb>::Connection>,
        conversation_ids: Vec<Vec<u8>>,
    ) -> HashMap<Vec<u8>, Option<i64>> {
        let mut cursor_map: HashMap<Vec<u8>, Option<i64>> = HashMap::new();
        for conversation_id in conversation_ids {
            let local_commit_log_cursor = conn
                .get_local_commit_log_cursor(&conversation_id)
                .ok()
                .flatten()
                .unwrap_or(0);
            let published_commit_log_cursor = conn
                .get_last_cursor_for_id(
                    &conversation_id,
                    xmtp_db::refresh_state::EntityKind::PublishedCommitLog,
                )
                .unwrap_or(0);

            if local_commit_log_cursor > published_commit_log_cursor {
                // We have new commits that have not been published to remote commit log yet
                cursor_map.insert(conversation_id, Some(published_commit_log_cursor));
            } else {
                cursor_map.insert(conversation_id, None); // Remote log is up to date with local commit log
            }
        }
        cursor_map
    }
}
