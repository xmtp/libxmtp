use crate::context::XmtpSharedContext;
use crate::groups::MlsGroup;
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
    #[error("storage error")]
    Generic,
}

impl NeedsDbReconnect for PendingSelfRemoveWorkerError {
    fn needs_db_reconnect(&self) -> bool {
        match self {
            Self::Storage(s) => s.db_needs_connection(),
            Self::Generic => false,
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

    async fn react_to_group_has_pending_leave_request(
        &mut self,
        group_id: &[u8],
    ) -> Result<(), PendingSelfRemoveWorkerError> {
        tracing::info!("Processing pending leave requests for group");
        // Check if the group has pending leave request
        let stored_group = self
            .context
            .db()
            .find_group(group_id)
            .map_err(|e| PendingSelfRemoveWorkerError::Storage(e.into()))?;
        tracing::info!(
            "Processing pending leave requests for group {:?}",
            stored_group
        );

        let stored_group = match stored_group {
            Some(group) => group,
            None => {
                tracing::warn!("Group not found: {}", hex::encode(group_id));
                return Ok(());
            }
        };

        if stored_group.has_pending_leave_request != Some(true) {
            return Ok(());
        }
        tracing::info!("stored group has pending leave request:");

        // Load the group with validation
        let (group, _stored_group) = MlsGroup::new_cached(self.context.clone(), group_id)
            .map_err(|e| PendingSelfRemoveWorkerError::Storage(e))?;

        // Check if the current inbox ID is in the admin list or super admin list
        let current_inbox_id = self.context.inbox_id().to_string();
        let admin_list = group
            .admin_list()
            .map_err(|e| PendingSelfRemoveWorkerError::Generic)?;
        let super_admin_list = group
            .super_admin_list()
            .map_err(|e| PendingSelfRemoveWorkerError::Generic)?;

        if !admin_list.contains(&current_inbox_id) && !super_admin_list.contains(&current_inbox_id)
        {
            tracing::debug!(
                "Current inbox ID {} is not in admin or super admin list, skipping pending leave request processing",
                current_inbox_id
            );
            return Ok(());
        }
        tracing::info!(
            "stored group has pending leave request and current inbox ID is in admin or super admin list:"
        );

        // Find the pending leave list, and remove the user inbox that exists in the list
        let metadata = group
            .mutable_metadata()
            .map_err(|e| PendingSelfRemoveWorkerError::Generic)?;

        let pending_remove_list = metadata.pending_remove_list.clone();

        if pending_remove_list.is_empty() {
            // No pending removals to process, clear the pending leave request status
            self.context
                .db()
                .set_group_has_pending_leave_request_status(group_id, Some(false))
                .map_err(|e| PendingSelfRemoveWorkerError::Storage(e.into()))?;
            return Ok(());
        }

        // Remove users from the group by their inbox IDs
        for inbox_id in pending_remove_list {
            match group
                .remove_members_by_inbox_id(&[inbox_id.clone().as_str()])
                .await
            {
                Ok(_) => {
                    tracing::info!("Successfully removed inbox_id {} from group", inbox_id);
                }
                Err(e) => {
                    tracing::error!("Failed to remove inbox_id {} from group: {}", inbox_id, e);
                    // Continue processing other removals even if one fails
                }
            }
        }

        // After processing all pending removals, clear the pending leave request status
        self.context
            .db()
            .set_group_has_pending_leave_request_status(group_id, Some(false))
            .map_err(|e| PendingSelfRemoveWorkerError::Storage(e.into()))?;
        tracing::info!("Completed processing pending leave requests for group");
        Ok(())
    }

    /// Iterate on the list of groups and delete expired messages
    async fn remove_pending_remove_users(&mut self) -> Result<(), PendingSelfRemoveWorkerError> {
        let db = self.context.db();
        match db.get_groups_have_pending_leave_request() {
            Ok(groups) => {
                if groups.len() > 0 {
                    tracing::info!("has pending remove for {:?} groups", groups);

                    self.react_to_group_has_pending_leave_request(&groups[0])
                        .await?;
                    tracing::info!("has pending remove for {:?} groups", groups);
                }
            }
            Err(e) => {
                tracing::error!("Failed to delete expired messages, error: {:?}", e);
            }
        }
        Ok(())
    }
}
