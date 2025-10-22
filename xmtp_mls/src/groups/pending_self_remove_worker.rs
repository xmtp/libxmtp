use crate::context::XmtpSharedContext;
use crate::groups::{GroupError, MlsGroup};
use crate::mls_store::MlsStore;
use crate::worker::{BoxedWorker, NeedsDbReconnect, Worker, WorkerFactory};
use crate::worker::{WorkerKind, WorkerResult};
use futures::{StreamExt, TryFutureExt};
use std::time::Duration;
use thiserror::Error;
use xmtp_db::{StorageError, prelude::*};

/// Interval at which the PendingSelfRemoveWorker runs to remove the members want requested SelfRemove.
pub const INTERVAL_DURATION: Duration = Duration::from_secs(1);

#[derive(Debug, Error)]
pub enum PendingSelfRemoveWorkerError {
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("group error: {0}")]
    GroupError(#[from] GroupError),
}

impl NeedsDbReconnect for PendingSelfRemoveWorkerError {
    fn needs_db_reconnect(&self) -> bool {
        match self {
            Self::Storage(s) => s.db_needs_connection(),
            Self::GroupError(_) => false,
        }
    }
}

pub struct PendingSelfRemoveWorker<Context> {
    context: Context,
    pub(crate) mls_store: MlsStore<Context>,
}

struct Factory<Context> {
    context: Context,
}

impl<Context> WorkerFactory for Factory<Context>
where
    Context: XmtpSharedContext + Send + Sync + 'static,
{
    fn kind(&self) -> WorkerKind {
        WorkerKind::PendingSelfRemove
    }

    fn create(
        &self,
        metrics: Option<crate::worker::DynMetrics>,
    ) -> (BoxedWorker, Option<crate::worker::DynMetrics>) {
        let worker = Box::new(PendingSelfRemoveWorker::new(self.context.clone())) as Box<_>;
        (worker, metrics)
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
        Factory {
            context: context.clone(),
        }
    }
}

impl<Context> PendingSelfRemoveWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    pub fn new(context: Context) -> Self {
        Self {
            context: context.clone(),
            mls_store: MlsStore::new(context),
        }
    }
}

impl<Context> PendingSelfRemoveWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    async fn run(&mut self) -> Result<(), PendingSelfRemoveWorkerError> {
        tracing::info!("PendingSelfRemove worker started");
        let mut intervals = xmtp_common::time::interval_stream(INTERVAL_DURATION);
        while (intervals.next().await).is_some() {
            self.remove_pending_remove_users().await?;
        }
        Ok(())
    }

    async fn react_to_group_has_pending_leave_request(
        &mut self,
        mls_group: &MlsGroup<Context>,
    ) -> Result<(), PendingSelfRemoveWorkerError> {
        tracing::info!(
            group_id = hex::encode(&mls_group.group_id),
            "Processing pending leave requests for group"
        );
        mls_group.remove_members_pending_removal().await?;
        mls_group.cleanup_pending_removal_list().await?;
        tracing::info!("Completed processing pending leave requests for group");
        Ok(())
    }

    /// Iterate on the list of groups and delete expired messages
    async fn remove_pending_remove_users(&mut self) -> Result<(), PendingSelfRemoveWorkerError> {
        let db = self.context.db();
        match db.get_groups_have_pending_leave_request() {
            Ok(groups) => {
                for group_id in groups {
                    match self.mls_store.group(&group_id) {
                        Ok(mls_group) => {
                            if let Err(e) = self
                                .react_to_group_has_pending_leave_request(&mls_group)
                                .await
                            {
                                tracing::error!(
                                    group_id = hex::encode(&group_id),
                                    error = %e,
                                    "Failed to process pending leave request for group"
                                );
                            }
                        }
                        Err(e) => {
                            tracing::error!(
                                group_id = hex::encode(&group_id),
                                error = %e,
                                "Failed to load MLS group from store"
                            );
                        }
                    }
                }
            }
            Err(e) => {
                tracing::error!("Failed to get groups with pending leave requests, error: {:?}", e);
                return Err(PendingSelfRemoveWorkerError::Storage(e.into()));
            }
        }
        Ok(())
    }
}
