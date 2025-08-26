use crate::context::XmtpSharedContext;
use crate::worker::{BoxedWorker, NeedsDbReconnect, Worker, WorkerFactory};
use crate::worker::{WorkerKind, WorkerResult};
use futures::{StreamExt, TryFutureExt};
use std::time::Duration;
use thiserror::Error;
use tokio::sync::OnceCell;
use xmtp_db::{StorageError, prelude::*};

/// Interval at which the DisappearingMessagesCleanerWorker runs to delete expired messages.
pub const INTERVAL_DURATION: Duration = Duration::from_secs(1);

#[derive(Debug, Error)]
pub enum PendingSelfRemoveWorkerError {
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
}

impl NeedsDbReconnect for PendingSelfRemoveWorkerError {
    fn needs_db_reconnect(&self) -> bool {
        match self {
            Self::Storage(s) => s.db_needs_connection(),
        }
    }
}

pub struct PendingSelfRemoveWorker<Context> {
    context: Context,
    #[allow(dead_code)]
    init: OnceCell<()>,
}

struct Factory<Context> {
    context: Context,
}

impl<Context> WorkerFactory for Factory<Context>
where
    Context: XmtpSharedContext + Send + Sync + 'static,
{
    fn create(
        &self,
        metrics: Option<crate::worker::DynMetrics>,
    ) -> (BoxedWorker, Option<crate::worker::DynMetrics>) {
        let worker = Box::new(PendingSelfRemoveWorker::new(self.context.clone())) as Box<_>;
        (worker, metrics)
    }

    fn kind(&self) -> WorkerKind {
        WorkerKind::PendingSelfRemove
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<Context> Worker for PendingSelfRemoveWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    fn kind(&self) -> WorkerKind {
        WorkerKind::PendingSelfRemove
    }

    async fn run_tasks(&mut self) -> WorkerResult<()> {
        self.run().map_err(|e| Box::new(e) as Box<_>).await
    }

    fn factory<C>(context: C) -> impl WorkerFactory + 'static
    where
        Self: Sized,
        C: XmtpSharedContext + Send + Sync + 'static,
    {
        Factory { context }
    }
}

impl<Context> PendingSelfRemoveWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    pub fn new(context: Context) -> Self {
        Self {
            context,
            init: OnceCell::new(),
        }
    }
}

impl<Context> PendingSelfRemoveWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    async fn run(&mut self) -> Result<(), PendingSelfRemoveWorkerError> {
        tracing::info!("pending self remove worker started");
        let mut intervals = xmtp_common::time::interval_stream(INTERVAL_DURATION);
        while (intervals.next().await).is_some() {
            self.remove_pending_remove_users().await?;
        }
        Ok(())
    }

    /// Iterate on the list of groups and delete expired messages
    async fn remove_pending_remove_users(&mut self) -> Result<(), PendingSelfRemoveWorkerError> {
        let db = self.context.db();
        match db.get_groups_have_pending_leave_request() {
            Ok(groups) => {
                tracing::info!("has pending remove for {:?} groups", groups);
            }
            Err(e) => {
                tracing::error!("Failed to delete expired messages, error: {:?}", e);
            }
        }
        Ok(())
    }
}
