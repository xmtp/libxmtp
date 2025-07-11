use futures::StreamExt;
use std::{sync::Arc, time::Duration};
use thiserror::Error;
use tokio::sync::OnceCell;
use xmtp_api::ApiError;
use xmtp_db::{local_commit_log::LocalCommitLog, StorageError, XmtpDb};
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

    async fn publish_commit_logs_to_remote(&mut self) -> Result<(), CommitLogError> {
        let provider = self.context.mls_provider();
        let conn = provider.db();


        // Step 1 is to get the list of all group_id for dms and for groups where we are a super admin
        let conversation_ids_for_remote_log = conn.get_conversation_ids_for_remote_log()?;
        
        // Step 2 is to check if for each conv id for remote log whether it's refresh_state cursor is lower than the local commit log sequence id
        for conversation_id in conversation_ids_for_remote_log {
        }

        // Step 3 is to publish any new local commit logs and to update relevant cursors

        // Step 3 is to publish the local commit log entries to the API
        let _commit_log_entries: Vec<LocalCommitLog> = vec![]; //conn.get_group_logs()?;

        // Publish commit log entries to the API
        let api = self.context.api();
        let plaintext_commit_log_entries: Vec<PlaintextCommitLogEntry> = vec![]; // TODO: convert commit_log_entries to plaintext_commit_log_entries
        api.publish_commit_log(plaintext_commit_log_entries).await?;
        Ok(())
    }
}
