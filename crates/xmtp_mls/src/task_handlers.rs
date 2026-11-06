//! The client's durable task handlers: one exhaustive dispatch over the
//! `Task` oneof, run by the core task runner.

use crate::{
    context::XmtpSharedContext,
    groups::MlsGroup,
    worker::{
        NeedsDbReconnect,
        device_sync::{DeviceSyncClient, DeviceSyncError},
        key_package_maintenance as kp,
        metrics::WorkerMetrics,
        tasks::{TaskHandlers, TaskOutcome, apply_pull_in},
    },
};
use prost::Message;
use std::sync::Arc;
use xmtp_common::RetryableError;
use xmtp_configuration::{KEY_PACKAGE_ROTATION_INTERVAL_NS, SYNC_UPDATE_INSTALLATIONS_INTERVAL_NS};
use xmtp_db::group::ConversationType;
use xmtp_db::prelude::{QueryIdentity, QueryKeyPackageHistory};
use xmtp_db::tasks::{QueryTasks, Task as DbTask};
use xmtp_events::EventEnvelope;
use xmtp_proto::xmtp::mls::database::{Task as TaskProto, task::Task as TaskKind};

/// How far out to push a task whose kind this build does not understand. Long
/// enough that an old client sharing the database does not spin on it, short
/// enough that a newer client picks it up soon after it starts.
pub(crate) const UNKNOWN_TASK_DEFER_NS: i64 = 60 * 60 * 1_000_000_000;

#[derive(thiserror::Error, Debug)]
pub enum TaskWorkerError {
    #[error("generic storage error: {0}")]
    Storage(#[from] xmtp_db::StorageError),
    #[error("group error: {0}")]
    Group(#[from] crate::groups::GroupError),
    #[error("device sync error: {0}")]
    DeviceSync(#[from] DeviceSyncError),
    #[error("failed to load MLS group from store: {0}")]
    LoadGroup(#[from] crate::mls_store::MlsStoreError),
    #[error("invalid task data for {id}: {error}")]
    InvalidTaskData { id: i64, error: prost::DecodeError },
    #[error("invalid hash for {id}, expected: {expected}, got: {got}")]
    InvalidHash {
        id: i64,
        expected: String,
        got: String,
    },
    #[error(transparent)]
    Conversion(#[from] xmtp_proto::ConversionError),
    #[error("identity error: {0}")]
    Identity(#[from] crate::identity::IdentityError),
    #[error("key package maintenance error: {0}")]
    KeyPackageMaintenance(
        #[from] crate::worker::key_package_maintenance::KeyPackageMaintenanceError,
    ),
    #[error("notification task failed")]
    Notification(#[from] crate::client::notifications::NotificationError),
}

impl RetryableError for TaskWorkerError {
    #[deny(clippy::wildcard_enum_match_arm)]
    fn is_retryable(&self) -> bool {
        match self {
            Self::Storage(e) => e.is_retryable(),
            Self::Group(e) => e.is_retryable(),
            Self::DeviceSync(e) => e.is_retryable(),
            Self::LoadGroup(e) => e.is_retryable(),
            Self::Identity(e) => e.is_retryable(),
            Self::KeyPackageMaintenance(e) => e.is_retryable(),
            Self::Notification(e) => e.is_retryable(),
            Self::Conversion(_) | Self::InvalidTaskData { .. } | Self::InvalidHash { .. } => false,
        }
    }

    /// The delay of a rate-limit rejection that failed this task turn.
    fn rate_limit_delay(&self) -> Option<xmtp_common::time::Duration> {
        match self {
            Self::Group(e) => e.rate_limit_delay(),
            Self::DeviceSync(e) => e.rate_limit_delay(),
            Self::LoadGroup(e) => e.rate_limit_delay(),
            Self::Identity(e) => e.rate_limit_delay(),
            Self::KeyPackageMaintenance(e) => e.rate_limit_delay(),
            Self::Notification(e) => e.rate_limit_delay(),
            Self::Storage(_)
            | Self::Conversion(_)
            | Self::InvalidTaskData { .. }
            | Self::InvalidHash { .. } => None,
        }
    }
}

impl NeedsDbReconnect for TaskWorkerError {
    fn restart_delay(&self) -> Option<xmtp_common::time::Duration> {
        self.rate_limit_delay()
    }

    #[deny(clippy::wildcard_enum_match_arm)]
    fn needs_db_reconnect(&self) -> bool {
        match self {
            TaskWorkerError::Storage(s) => s.db_needs_connection(),
            TaskWorkerError::LoadGroup(e) => e.needs_db_reconnect(),
            // Forward through GroupError's own classifier so a dropped pool hiding
            // in a `Db`/`MlsStore` (not just `Storage`) variant still restarts the
            // worker instead of being retried on a dead connection.
            TaskWorkerError::Group(e) => e.needs_db_reconnect(),
            TaskWorkerError::DeviceSync(e) => e.needs_db_reconnect(),
            TaskWorkerError::InvalidTaskData { .. } => false,
            TaskWorkerError::InvalidHash { .. } => false,
            TaskWorkerError::Conversion(_) => false,
            TaskWorkerError::Identity(e) => e.needs_db_reconnect(),
            TaskWorkerError::KeyPackageMaintenance(e) => e.needs_db_reconnect(),
            TaskWorkerError::Notification(
                crate::client::notifications::NotificationError::Storage(e),
            ) => e.db_needs_connection(),
            TaskWorkerError::Notification(
                crate::client::notifications::NotificationError::Group(e),
            ) => e.needs_db_reconnect(),
            TaskWorkerError::Notification(_) => false,
        }
    }
}

/// Runs every durable task kind this build knows, and keeps the notification
/// task's deadline current as the runner sees changes.
#[derive(Default)]
pub(crate) struct MlsTaskHandlers;

#[xmtp_common::async_trait]
impl<Context> TaskHandlers<Context> for MlsTaskHandlers
where
    Context: XmtpSharedContext + 'static,
{
    type Error = TaskWorkerError;

    // A new `Task` kind fails to compile here until it has a handler.
    #[deny(clippy::wildcard_enum_match_arm)]
    async fn dispatch(
        &self,
        context: &Context,
        task: &DbTask,
    ) -> Result<TaskOutcome, TaskWorkerError> {
        #[cfg(any(test, feature = "test-hooks"))]
        if let Some((work, rejection)) = test_hooks::rate_limit_override(&task.data_hash) {
            xmtp_common::time::sleep(work).await;
            return Err(TaskWorkerError::Identity(
                crate::identity::IdentityError::ApiClient(xmtp_api::ApiError::Api(
                    xmtp_proto::api::NetworkError::new(xmtp_api_grpc::error::GrpcError::Status(
                        rejection,
                    )),
                )),
            ));
        }
        let data_hash = xmtp_common::sha256_bytes(&task.data);
        if task.data_hash != data_hash {
            let expected = hex::encode(&data_hash);
            let got = hex::encode(&task.data_hash);
            tracing::warn!(
                "Task {} data hash mismatch. Expected {expected}, got {got}",
                task.id,
            );
        }
        let task_proto = match TaskProto::decode(task.data.as_slice()) {
            Ok(task_proto) => task_proto,
            Err(e) => {
                context.db().delete_task(task.id)?;
                tracing::warn!("Task {} data decode error: {}", task.id, e);
                return Ok(TaskOutcome::Done);
            }
        };
        match task_proto.task {
            Some(TaskKind::NotificationSync(_)) => {
                return Ok(crate::worker::notifications::run(context).await?);
            }
            Some(TaskKind::ProcessPendingSelfRemove(pending)) => {
                process_pending_self_remove(task, pending, context).await?;
            }
            Some(TaskKind::PullInDeadline(p)) => {
                apply_pull_in(context, task, p)?;
            }
            Some(TaskKind::KpRotation(_)) => {
                let now = xmtp_common::time::now_ns();
                kp::rotate_if_needed(context).await?;
                // Unconditional (no-op when nothing is marked): a backoff retry
                // after rotate-succeeded/nudge-failed must still re-nudge.
                kp::nudge_deletion(context)?;
                // Advance to the LIVE rotation column (rotate reset it to +30d; a
                // welcome nudge may have re-lowered it). Do NOT hardcode +30d.
                let next = context
                    .db()
                    .next_key_package_rotation_ns()?
                    .unwrap_or(now + KEY_PACKAGE_ROTATION_INTERVAL_NS);
                return Ok(TaskOutcome::RescheduleAt(next));
            }
            Some(TaskKind::KpDeletion(_)) => {
                kp::sweep_expired(context)?;
                let now = xmtp_common::time::now_ns();
                // Nothing pending: park one rotation interval out (deletions only
                // arise from rotations; same constant as the KpRotation arm — the
                // exact value isn't load-bearing, nudges pull the task in sooner).
                let next = context
                    .db()
                    .min_key_package_delete_at_ns()?
                    .unwrap_or(now + KEY_PACKAGE_ROTATION_INTERVAL_NS);
                return Ok(TaskOutcome::RescheduleAt(next));
            }
            Some(TaskKind::AddMissingInstallations(add)) => {
                return run_add_missing_installations(task, add, context).await;
            }
            // A kind this build cannot run: `KpLiveness` has no handler yet,
            // and `None` is a newer build's kind (prost drops an unknown
            // `oneof task` field). Deleting would drop work a newer client
            // sharing this database still owes. `expires_at_ns` bounds an
            // unclaimed row.
            Some(TaskKind::KpLiveness(_)) | None => {
                tracing::warn!(
                    "Task {} has a kind this version does not handle. Deferring.",
                    task.id
                );
                return Ok(TaskOutcome::RescheduleAt(
                    xmtp_common::time::now_ns() + UNKNOWN_TASK_DEFER_NS,
                ));
            }
        }
        Ok(TaskOutcome::Done)
    }

    fn on_start(&self, context: &Context) -> Result<(), TaskWorkerError> {
        Ok(crate::worker::notifications::wake(context)?)
    }

    fn on_events(
        &self,
        context: &Context,
        _events: &[EventEnvelope<crate::core::events::InternalEvent>],
    ) -> Result<(), TaskWorkerError> {
        Ok(crate::worker::notifications::wake(context)?)
    }
}

/// Run a `ProcessPendingSelfRemove` task: load the group and remove members
/// who requested to leave (a no-op unless this client is super-admin).
async fn process_pending_self_remove<Context: XmtpSharedContext>(
    task: &DbTask,
    pending: xmtp_proto::xmtp::mls::database::ProcessPendingSelfRemove,
    context: &Context,
) -> Result<(), TaskWorkerError> {
    // A malformed group_id can never succeed — drop the task, don't retry.
    let Ok(group_id) = xmtp_proto::types::GroupId::try_from(pending.group_id.as_slice()) else {
        tracing::warn!(
            "Task {} has a malformed group_id for ProcessPendingSelfRemove. Deleting.",
            task.id
        );
        context.db().delete_task(task.id)?;
        return Ok(());
    };
    match MlsGroup::load_by_id(context, &group_id) {
        Ok(group) => {
            // No-op unless super-admin; idempotent, so retries are safe.
            group.process_pending_self_removals().await?;
            Ok(())
        }
        Err(crate::mls_store::MlsStoreError::NotFound(_)) => {
            tracing::debug!(
                "Task {} targets a group that no longer exists. Deleting.",
                task.id
            );
            context.db().delete_task(task.id)?;
            Ok(())
        }
        // A DB/connection error is transient — let it retry.
        Err(e) => Err(e.into()),
    }
}

/// Run an `AddMissingInstallations` task: load the group and reconcile its
/// membership with the inbox's latest identity state. Idempotent — the
/// underlying group method no-ops when membership is already current, so
/// retries and duplicate rows are safe. The selected sync group's task
/// runs again after the retry interval while an own installation it can
/// retry has no leaf there.
async fn run_add_missing_installations<Context: XmtpSharedContext>(
    task: &DbTask,
    add: xmtp_proto::xmtp::mls::database::AddMissingInstallations,
    context: &Context,
) -> Result<TaskOutcome, TaskWorkerError> {
    // A malformed group_id can never succeed — drop the task, don't retry.
    let Ok(group_id) = xmtp_proto::types::GroupId::try_from(add.group_id.as_slice()) else {
        tracing::warn!(
            "Task {} has a malformed group_id for AddMissingInstallations. Deleting.",
            task.id
        );
        context.db().delete_task(task.id)?;
        return Ok(TaskOutcome::Done);
    };
    match MlsGroup::load_by_id(context, &group_id) {
        // implements: SYNC-034, GMOD-055
        Ok(group) if group.conversation_type == ConversationType::Sync => {
            let metrics = Arc::new(WorkerMetrics::new(context.installation_id()));
            let sync = DeviceSyncClient::new(context.clone(), metrics);
            let retried = sync.retry_own_installations(&group).await;
            #[cfg(any(test, feature = "test-hooks"))]
            let retried = match test_hooks::take_sync_retry_failure() {
                true => Err(DeviceSyncError::Conversion),
                false => retried,
            };
            // A failed attempt also waits the retry interval, unless the
            // runner must honor a rate limit or reconnect.
            // implements: RATE-041
            match retried {
                Ok(false) => Ok(TaskOutcome::Done),
                Err(e)
                    if xmtp_common::RetryableError::rate_limit_delay(&e).is_some()
                        || e.needs_db_reconnect() =>
                {
                    Err(e.into())
                }
                result => {
                    if let Err(e) = result {
                        tracing::warn!(error = %e, "sync group retry failed");
                    }
                    Ok(TaskOutcome::RescheduleAt(
                        xmtp_common::time::now_ns() + SYNC_UPDATE_INSTALLATIONS_INTERVAL_NS,
                    ))
                }
            }
        }
        Ok(group) => {
            group.add_missing_installations().await?;
            Ok(TaskOutcome::Done)
        }
        Err(crate::mls_store::MlsStoreError::NotFound(_)) => {
            tracing::debug!(
                "Task {} targets a group that no longer exists. Deleting.",
                task.id
            );
            context.db().delete_task(task.id)?;
            Ok(TaskOutcome::Done)
        }
        // A DB/connection error is transient — let it retry.
        Err(e) => Err(e.into()),
    }
}

/// Client task-dispatch controls for tests. The dispatch reads them; tests
/// set them. Each assumes process-per-test isolation.
#[cfg(any(test, feature = "test-hooks"))]
#[doc(hidden)]
pub mod test_hooks {
    use std::sync::Mutex;
    use xmtp_common::time::Duration;

    type RateLimitOverride = (Vec<u8>, Duration, tonic::Status);

    static RATE_LIMIT_OVERRIDE: Mutex<Option<RateLimitOverride>> = Mutex::new(None);
    static FAIL_SYNC_RETRY: Mutex<bool> = Mutex::new(false);

    /// While set to `(target_data_hash, work, rejection)`, the client's task
    /// dispatch works for `work`, then fails with the backend `rejection`, for
    /// the matching task. `None` clears it.
    pub fn set_rate_limit_override(value: Option<(Vec<u8>, Duration, tonic::Status)>) {
        *RATE_LIMIT_OVERRIDE.lock().expect("task test hooks") = value;
    }

    /// The `(work, rejection)` that `set_rate_limit_override` set for the
    /// task with `data_hash`.
    pub fn rate_limit_override(data_hash: &[u8]) -> Option<(Duration, tonic::Status)> {
        RATE_LIMIT_OVERRIDE
            .lock()
            .expect("task test hooks")
            .as_ref()
            .filter(|(hash, _, _)| hash.as_slice() == data_hash)
            .map(|(_, work, rejection)| (*work, rejection.clone()))
    }

    /// Makes the next sync-group retry fail without a rate limit, once.
    pub fn fail_next_sync_retry() {
        *FAIL_SYNC_RETRY.lock().expect("task test hooks") = true;
    }

    /// Whether `fail_next_sync_retry` armed a failure; consumes it.
    pub fn take_sync_retry_failure() -> bool {
        std::mem::take(&mut *FAIL_SYNC_RETRY.lock().expect("task test hooks"))
    }
}

#[cfg(test)]
mod tests;
