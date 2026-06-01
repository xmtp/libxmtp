use crate::context::XmtpSharedContext;
use crate::groups::{GroupError, MlsGroup};
use crate::mls_store::{MlsStore, MlsStoreError};
use crate::worker::{BoxedWorker, NeedsDbReconnect, Worker, WorkerFactory};
use crate::worker::{WorkerKind, WorkerResult};
use futures::{StreamExt, TryFutureExt};
use std::time::Duration;
use thiserror::Error;
use xmtp_db::{StorageError, prelude::*};
use xmtp_proto::types::GroupId;

/// Interval at which the PendingSelfRemoveWorker runs to remove the members want requested SelfRemove.
pub const INTERVAL_DURATION: Duration = Duration::from_secs(2);

#[derive(Debug, Error)]
pub enum PendingSelfRemoveWorkerError {
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("failed to get groups with pending leave requests: {0}")]
    GetPendingLeaveGroups(StorageError),
    #[error("failed to load MLS group from store: {0}")]
    LoadGroup(#[from] MlsStoreError),
    #[error("group error: {0}")]
    GroupError(#[from] GroupError),
}

impl NeedsDbReconnect for PendingSelfRemoveWorkerError {
    fn needs_db_reconnect(&self) -> bool {
        match self {
            Self::Storage(s) | Self::GetPendingLeaveGroups(s) => s.db_needs_connection(),
            Self::LoadGroup(s) => s.needs_db_reconnect(),
            // A dropped pool can hide in a GroupError (member-removal path); forward.
            Self::GroupError(e) => e.needs_db_reconnect(),
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
    Context: XmtpSharedContext + 'static,
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

#[xmtp_common::async_trait]
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
        C: XmtpSharedContext + 'static,
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

    #[tracing::instrument(
        skip_all,
        fields(group_id = %mls_group.group_id),
        name = "process_pending_leave_requests"
    )]
    async fn react_to_group_has_pending_leave_request(
        &mut self,
        mls_group: &MlsGroup<Context>,
    ) -> Result<(), PendingSelfRemoveWorkerError> {
        mls_group.remove_members_pending_removal().await?;
        mls_group.cleanup_pending_removal_list().await?;
        Ok(())
    }

    /// Iterate on the list of groups and delete expired messages
    async fn remove_pending_remove_users(&mut self) -> Result<(), PendingSelfRemoveWorkerError> {
        let db = self.context.db();
        // Errors propagate to the supervisor (the sole logger); the worker restarts and
        // retries the pending removals on its next turn.
        let groups = db
            .get_groups_have_pending_leave_request()
            .map_err(|e| PendingSelfRemoveWorkerError::GetPendingLeaveGroups(e.into()))?;
        for group_id in groups {
            let Ok(group_id) = GroupId::try_from(group_id) else {
                tracing::warn!("skipping malformed group_id in pending leave request list");
                continue;
            };
            let mls_group = self.mls_store.group(&group_id)?;
            self.react_to_group_has_pending_leave_request(&mls_group)
                .await?;
        }
        Ok(())
    }
}
