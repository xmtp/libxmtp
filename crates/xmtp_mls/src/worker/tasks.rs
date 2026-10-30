use crate::{
    context::XmtpSharedContext,
    subscriptions::internal::InternalEvent,
    worker::{
        NeedsDbReconnect, Worker, WorkerFactory, WorkerKind, device_sync::DeviceSyncError,
        key_package_maintenance as kp,
    },
};
use prost::Message;
use std::sync::Arc;
use xmtp_configuration::KEY_PACKAGE_ROTATION_INTERVAL_NS;
use xmtp_db::prelude::{QueryIdentity, QueryKeyPackageHistory};
use xmtp_db::tasks::{QueryTasks, Task as DbTask, TaskDataHash};
use xmtp_db::{StorageError, diesel};
use xmtp_events::Subscription;
use xmtp_proto::xmtp::mls::database::Task as TaskProto;

/// How far out to push a task whose kind this build does not understand. Long
/// enough that an old client sharing the database does not spin on it, short
/// enough that a newer client picks it up soon after it starts.
const UNKNOWN_TASK_DEFER_NS: i64 = 60 * 60 * 1_000_000_000;

/// `Done` = one-shot, row deleted. `RescheduleAt(ns)` = recurring, row kept and
/// advanced to that absolute deadline.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum TaskOutcome {
    Done,
    RescheduleAt(i64),
}

#[cfg(test)]
pub(crate) mod test_hooks {
    use std::sync::Mutex;
    /// `(target_data_hash, deadline)` → `run_task` returns `RescheduleAt(deadline)`
    /// for the matching task. Reset at test end; assumes process-per-test isolation.
    pub(crate) static RESCHEDULE_OVERRIDE: Mutex<Option<(Vec<u8>, i64)>> = Mutex::new(None);
    /// `(target_data_hash, work, delay)` → `run_task` works for `work`, then
    /// fails with a rate limit of `delay`, for the matching task.
    pub(crate) static RATE_LIMIT_OVERRIDE: Mutex<
        Option<(
            Vec<u8>,
            xmtp_common::time::Duration,
            xmtp_common::time::Duration,
        )>,
    > = Mutex::new(None);
    /// When set, the next retry schedule write fails, once.
    pub(crate) static FAIL_RESCHEDULE: Mutex<bool> = Mutex::new(false);
    pub(crate) static TASK_SCHEDULED_CONSUMED: Mutex<
        Option<(Vec<u8>, tokio::sync::oneshot::Sender<()>)>,
    > = Mutex::new(None);
}

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

impl TaskWorkerError {
    /// The delay of a rate-limit rejection that failed this task turn.
    pub(crate) fn rate_limit_delay(&self) -> Option<xmtp_common::time::Duration> {
        use xmtp_common::RetryableError;
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

    fn needs_db_reconnect(&self) -> bool {
        match self {
            TaskWorkerError::Storage(s)
            | TaskWorkerError::DeviceSync(DeviceSyncError::Storage(s)) => s.db_needs_connection(),
            TaskWorkerError::LoadGroup(e) => e.needs_db_reconnect(),
            // Forward through GroupError's own classifier so a dropped pool hiding
            // in a `Db`/`MlsStore` (not just `Storage`) variant still restarts the
            // worker instead of being retried on a dead connection.
            TaskWorkerError::Group(e) => e.needs_db_reconnect(),
            TaskWorkerError::DeviceSync(_) => false,
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

#[derive(Clone)]
pub struct TaskWorkerChannels {
    /// Serializes notification requests across inline calls and task turns.
    pub(crate) notification_request: Arc<tokio::sync::Mutex<()>>,
    notification_revision: Arc<std::sync::atomic::AtomicUsize>,
}

impl Default for TaskWorkerChannels {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskWorkerChannels {
    pub fn new() -> Self {
        Self {
            notification_request: Default::default(),
            notification_revision: Default::default(),
        }
    }

    /// Invalidate a prepared notification batch after a local change.
    pub fn mark_notification_changed(&self) {
        use std::sync::atomic::Ordering;
        self.notification_revision.fetch_add(1, Ordering::AcqRel);
    }

    pub(crate) fn notification_revision(&self) -> usize {
        self.notification_revision
            .load(std::sync::atomic::Ordering::Acquire)
    }
}

/// Durably enqueue a `PullInDeadline` for `target_data_hash`, then emit its fact.
/// The row is committed before the event; duplicates coalesce on data_hash. Lifetime is
/// bounded by `expires_at_ns` alone (pass `NEVER_EXPIRES` for critical nudges).
/// Callers must commit the target row FIRST: a pull-in never waits for its target —
/// a miss is dropped (debug-logged), since a missing target is normally a completed
/// one-shot and retrying would spin forever on never-expiring nudges.
pub(crate) fn enqueue_pull_in<Context: XmtpSharedContext>(
    context: &Context,
    target_data_hash: TaskDataHash,
    not_later_than_ns: i64,
    expires_at_ns: i64,
) -> Result<(), xmtp_db::StorageError> {
    let now = xmtp_common::time::now_ns();
    let task = xmtp_db::tasks::NewTask::builder()
        .originating_message_sequence_id(0)
        .next_attempt_at_ns(now) // the pull-in itself is due immediately
        .expires_at_ns(expires_at_ns)
        .max_attempts(i32::MAX) // lifetime bounded by expires_at_ns, not retries
        .build(xmtp_proto::xmtp::mls::database::Task {
            task: Some(xmtp_proto::xmtp::mls::database::task::Task::PullInDeadline(
                xmtp_proto::xmtp::mls::database::PullInDeadline {
                    target_data_hash: target_data_hash.to_vec(),
                    not_later_than_ns,
                },
            )),
        })?;
    context.db().create_or_ignore_task(task)?;
    use xmtp_events::EventWriter;
    context
        .events()
        .emit(None, Some(InternalEvent::TaskScheduled));
    Ok(())
}

pub struct Factory<Context> {
    context: Context,
}

impl<Context> WorkerFactory for Factory<Context>
where
    Context: XmtpSharedContext + 'static,
{
    fn kind(&self) -> WorkerKind {
        WorkerKind::TaskRunner
    }

    fn create(
        &self,
        _metrics: Option<crate::worker::DynMetrics>,
    ) -> (
        crate::worker::BoxedWorker,
        Option<crate::worker::DynMetrics>,
    ) {
        let worker = TaskWorker::new(self.context.clone());
        (Box::new(worker) as Box<_>, None)
    }
}

pub struct TaskWorker<Context> {
    context: Context,
    subscription: Option<Arc<Subscription<InternalEvent>>>,
}

#[xmtp_common::async_trait]
impl<Context> Worker for TaskWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    fn kind(&self) -> WorkerKind {
        WorkerKind::TaskRunner
    }

    fn set_subscription(&mut self, subscription: Arc<Subscription<InternalEvent>>) {
        self.subscription = Some(subscription);
    }

    async fn run_tasks(&mut self) -> Result<(), Box<dyn NeedsDbReconnect>> {
        self.run().await.map_err(|e| Box::new(e) as _)
    }

    fn factory<C>(context: C) -> impl WorkerFactory + 'static
    where
        Self: Sized,
        C: XmtpSharedContext + 'static,
    {
        Factory { context }
    }
}

impl<Context> TaskWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    pub fn new(context: Context) -> Self {
        Self {
            context,
            subscription: None,
        }
    }

    #[cfg(test)]
    fn record_task_fact(&self, event: &xmtp_events::EventEnvelope<InternalEvent>) {
        if !matches!(&event.internal, Some(InternalEvent::TaskScheduled)) {
            return;
        }
        let witness = {
            let mut hook = test_hooks::TASK_SCHEDULED_CONSUMED.lock().unwrap();
            if hook.as_ref().is_some_and(|(installation, _)| {
                installation.as_slice() == self.context.installation_id().to_vec().as_slice()
            }) {
                hook.take()
            } else {
                None
            }
        };
        if let Some((_, received)) = witness {
            let _ = received.send(());
        }
    }

    pub async fn run(&mut self) -> Result<(), TaskWorkerError> {
        if self.subscription.is_none() {
            let (filter, depth) = crate::worker::worker_event_filter(WorkerKind::TaskRunner)
                .expect("task runner has an event filter");
            self.subscription = Some(Arc::new(self.context.events().subscribe(filter, depth)));
        }
        let subscription = self
            .subscription
            .as_ref()
            .expect("subscription is initialized")
            .clone();
        crate::worker::notifications::wake(&self.context)?;
        loop {
            let drained = subscription.drain();
            #[cfg(test)]
            for event in &drained {
                self.record_task_fact(event);
            }
            if drained
                .into_iter()
                .any(|event| !matches!(event.internal, Some(InternalEvent::TaskScheduled)))
            {
                crate::worker::notifications::wake(&self.context)?;
            }
            let next_task = self.context.db().get_next_task()?;
            let next_wakeup = Self::next_wakeup(
                next_task.as_ref().map(|t| t.next_attempt_at_ns),
                xmtp_common::time::now_ns(),
            );
            tokio::select! {
                event = subscription.next() => {
                    let Some(event) = event else { break Ok(()); };
                    #[cfg(test)]
                    self.record_task_fact(&event);
                    let more_notification_changes = subscription.drain().into_iter().any(|event| {
                            !matches!(event.internal, Some(InternalEvent::TaskScheduled))
                        });
                    let notification_changed = !matches!(event.internal, Some(InternalEvent::TaskScheduled))
                        || more_notification_changes;
                    if notification_changed {
                        crate::worker::notifications::wake(&self.context)?;
                    }
                }
                () = xmtp_common::time::sleep(next_wakeup) => {
                    if let Some(task) = next_task {
                        Self::run_and_reschedule_task(task, &self.context).await?;
                    }
                }
            }
        }
    }
    #[tracing::instrument(skip_all, fields(worker = "TaskRunner", operation = "worker_turn"))]
    pub(crate) async fn run_and_reschedule_task(
        task: DbTask,
        context: &Context,
    ) -> Result<(), TaskWorkerError> {
        let now = xmtp_common::time::now_ns();
        if task.expires_at_ns < now || task.attempts >= task.max_attempts {
            context.db().delete_task(task.id)?;
            return Ok(());
        }
        if task.next_attempt_at_ns > now {
            // This will get called again — expected scheduler behavior, not a
            // warning (it was ~3k warns/day in prod).
            tracing::debug!(
                task_id = task.id,
                "Task {} called before next attempt at {}. Now: {now}",
                task.id,
                task.next_attempt_at_ns
            );
            return Ok(());
        }
        match Self::run_task(&task, context).await {
            Ok(TaskOutcome::Done) => {
                context.db().delete_task(task.id)?;
            }
            Ok(TaskOutcome::RescheduleAt(t)) => {
                // Plain advance + attempts=0. A MIN floor here would pin a just-run
                // (past-due) row and hot-loop it.
                match context.db().update_task(task.id, 0, now, t) {
                    Ok(_) => {}
                    Err(StorageError::DieselResult(diesel::result::Error::NotFound)) => {
                        tracing::debug!("Task {} vanished before reschedule; skipping", task.id);
                    }
                    Err(e) => return Err(e.into()),
                }
            }
            Err(error) => {
                let mut attempts = task.attempts + 1;
                let attempt_scaling_factor = (task.backoff_scaling_factor as f64).powi(attempts);
                let next_attempt_duration = (((task.initial_backoff_duration_ns as f64)
                    * attempt_scaling_factor) as i64)
                    .min(task.max_backoff_duration_ns);
                let retry_from =
                    if task.data_hash == crate::worker::notifications::task_hash().as_ref() {
                        xmtp_common::time::now_ns()
                    } else {
                        now
                    };
                let backoff_at_ns = retry_from.saturating_add(next_attempt_duration);
                let mut next_attempt_at_ns = backoff_at_ns;
                let mut expires_at_ns = task.expires_at_ns;
                // A rate-limited request waits at least the delay, counted from
                // when the rejection arrived, not from when the turn started.
                // Neither the wait it adds past the backoff nor the refused turn
                // counts against the task's lifetime or its attempts, so a rate
                // limit never expires or deletes the refused work. At least one
                // attempt is recorded, which marks the task as backing off, so
                // a pull-in keeps it on its schedule.
                // implements: RATE-041
                if let Some(delay) = error.rate_limit_delay() {
                    attempts = task.attempts.max(1);
                    let until = xmtp_common::time::now_ns()
                        .saturating_add(i64::try_from(delay.as_nanos()).unwrap_or(i64::MAX));
                    next_attempt_at_ns = next_attempt_at_ns.max(until);
                    expires_at_ns = expires_at_ns
                        .saturating_add(next_attempt_at_ns.saturating_sub(backoff_at_ns));
                }
                let next_attempt_duration = next_attempt_at_ns.saturating_sub(retry_from);
                tracing::warn!(%error, "Task {} retry failed. Retrying in {next_attempt_duration}ns", task.id);
                #[cfg(test)]
                let stored = if std::mem::take(&mut *test_hooks::FAIL_RESCHEDULE.lock().unwrap()) {
                    Err(StorageError::DieselResult(
                        diesel::result::Error::BrokenTransactionManager,
                    ))
                } else {
                    context.db().update_task_and_expiry(
                        task.id,
                        attempts,
                        now,
                        next_attempt_at_ns,
                        expires_at_ns,
                    )
                };
                #[cfg(not(test))]
                let stored = context.db().update_task_and_expiry(
                    task.id,
                    attempts,
                    now,
                    next_attempt_at_ns,
                    expires_at_ns,
                );
                match stored {
                    Ok(_) => {}
                    // The row was concurrently deleted (e.g. a dead-row cleanup in
                    // upsert_pending_self_remove_task crossed this retry). Nothing
                    // left to reschedule — don't abort the worker loop.
                    Err(StorageError::DieselResult(diesel::result::Error::NotFound)) => {
                        tracing::debug!("Task {} vanished before reschedule; skipping", task.id);
                    }
                    // The task stays due, so a restarted worker would resend
                    // it at once. Return the rejection instead: the worker then
                    // restarts only after the delay.
                    // implements: RATE-041
                    Err(e) if error.rate_limit_delay().is_some() => {
                        tracing::warn!(%e, "Task {} rate-limit hold not stored", task.id);
                        return Err(error);
                    }
                    Err(e) => return Err(e.into()),
                }
            }
        }
        Ok(())
    }
    async fn run_task(task: &DbTask, context: &Context) -> Result<TaskOutcome, TaskWorkerError> {
        #[cfg(test)]
        if let Some((hash, t)) = test_hooks::RESCHEDULE_OVERRIDE.lock().unwrap().clone()
            && task.data_hash == hash
        {
            return Ok(TaskOutcome::RescheduleAt(t));
        }
        #[cfg(test)]
        let hook = test_hooks::RATE_LIMIT_OVERRIDE.lock().unwrap().clone();
        #[cfg(test)]
        if let Some((hash, work, delay)) = hook
            && task.data_hash == hash
        {
            xmtp_common::time::sleep(work).await;
            return Err(TaskWorkerError::Identity(
                crate::identity::IdentityError::ApiClient(xmtp_api::ApiError::Api(
                    xmtp_proto::api::NetworkError::new(xmtp_api_grpc::error::GrpcError::Status(
                        xmtp_api_grpc::error::rate_limited_status("publish", delay),
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
            Some(xmtp_proto::xmtp::mls::database::task::Task::NotificationSync(_)) => {
                return Ok(crate::worker::notifications::run(context).await?);
            }
            Some(xmtp_proto::xmtp::mls::database::task::Task::ProcessPendingSelfRemove(
                pending,
            )) => {
                Self::process_pending_self_remove(task, pending, context).await?;
            }
            Some(xmtp_proto::xmtp::mls::database::task::Task::PullInDeadline(p)) => {
                // Runs on the worker thread — the sole rescheduler of existing
                // rows' `next_attempt_at_ns` — so no transaction is needed
                // (inserts happen off-thread; only deadline mutation is guarded).
                let matched = match TaskDataHash::try_from(p.target_data_hash.as_slice()) {
                    Ok(h) => {
                        // A target in failure backoff keeps its stored schedule,
                        // which holds any rate-limit delay, across restarts too.
                        // implements: RATE-041
                        let backing_off =
                            context.db().get_tasks()?.iter().any(|target| {
                                target.data_hash == h.as_ref() && target.attempts > 0
                            });
                        backing_off
                            || context
                                .db()
                                .pull_in_task_deadline(&h, p.not_later_than_ns)?
                    }
                    Err(_) => false, // malformed length: fall through to the miss log
                };
                if !matched {
                    // Completed one-shot target, or a producer broke the
                    // commit-target-first contract. Drop either way (retrying
                    // would spin forever on never-expiring nudges). warn: every
                    // in-tree pull-in targets a never-deleted singleton, so a
                    // miss is always anomalous today.
                    tracing::warn!(
                        target_data_hash = hex::encode(&p.target_data_hash),
                        "pull-in target missing; dropping nudge for task {}",
                        task.id
                    );
                }
            }
            Some(xmtp_proto::xmtp::mls::database::task::Task::KpRotation(_)) => {
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
            Some(xmtp_proto::xmtp::mls::database::task::Task::KpDeletion(_)) => {
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
            Some(xmtp_proto::xmtp::mls::database::task::Task::AddMissingInstallations(add)) => {
                Self::run_add_missing_installations(task, add, context).await?;
            }
            // A kind this build cannot run: `KpLiveness` has no handler yet,
            // and `None` is a newer build's kind (prost drops an unknown
            // `oneof task` field). Deleting would drop work a newer client
            // sharing this database still owes. `expires_at_ns` bounds an
            // unclaimed row.
            Some(xmtp_proto::xmtp::mls::database::task::Task::KpLiveness(_)) | None => {
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
    fn next_wakeup(
        next_attempt_at_ns: Option<i64>,
        // these are passed in for testing
        now: i64,
    ) -> xmtp_common::time::Duration {
        use xmtp_common::r#const::NS_IN_DAY;
        let now_plus_one_day = now.saturating_add(NS_IN_DAY);
        let next_task_wakeup = next_attempt_at_ns.unwrap_or(i64::MAX).min(now_plus_one_day);
        if now > next_task_wakeup {
            xmtp_common::time::Duration::from_nanos(0)
        } else {
            std::time::Duration::from_nanos((next_task_wakeup - now) as u64)
        }
    }

    /// Run a `ProcessPendingSelfRemove` task: load the group and remove members
    /// who requested to leave (a no-op unless this client is super-admin).
    async fn process_pending_self_remove(
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
        match crate::mls_store::MlsStore::new(context.clone()).group(&group_id) {
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
    /// retries and duplicate rows are safe.
    async fn run_add_missing_installations(
        task: &DbTask,
        add: xmtp_proto::xmtp::mls::database::AddMissingInstallations,
        context: &Context,
    ) -> Result<(), TaskWorkerError> {
        // A malformed group_id can never succeed — drop the task, don't retry.
        let Ok(group_id) = xmtp_proto::types::GroupId::try_from(add.group_id.as_slice()) else {
            tracing::warn!(
                "Task {} has a malformed group_id for AddMissingInstallations. Deleting.",
                task.id
            );
            context.db().delete_task(task.id)?;
            return Ok(());
        };
        match crate::mls_store::MlsStore::new(context.clone()).group(&group_id) {
            Ok(group) => {
                group.add_missing_installations().await?;
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tester;
    use crate::worker::{WorkerConfig, WorkerKind};
    use xmtp_db::tasks::{NewTask, data_hash_for};
    use xmtp_proto::xmtp::mls::database::{Task as TaskProto, task::Task as TaskKind};

    /// A unique one-shot payload: a PullInDeadline aimed at a random, nonexistent
    /// target. Applying it is a no-op; its data_hash is unique per call.
    fn unique_proto() -> TaskProto {
        TaskProto {
            task: Some(TaskKind::PullInDeadline(
                xmtp_proto::xmtp::mls::database::PullInDeadline {
                    target_data_hash: xmtp_common::rand_vec::<32>(),
                    not_later_than_ns: 0,
                },
            )),
        }
    }

    fn rate_limited(delay: xmtp_common::time::Duration) -> xmtp_api::ApiError {
        xmtp_api::dyn_err(xmtp_proto::api::ApiClientError::client(
            xmtp_api_grpc::error::GrpcError::Status(xmtp_api_grpc::error::rate_limited_status(
                "publish", delay,
            )),
        ))
    }

    /// Every task error that wraps a rate-limited request carries its delay
    /// to the runner, which schedules no earlier than it.
    // verifies: RATE-041
    #[xmtp_common::test(unwrap_try = true)]
    fn rate_limit_delays_reach_the_task_runner_through_each_wrapper() {
        use crate::{
            client::ClientError, groups::GroupError, identity::IdentityError,
            worker::key_package_maintenance::KeyPackageMaintenanceError,
        };
        use xmtp_common::RetryableError;
        use xmtp_id::{associations::SignatureError, scw_verifier::VerifierError};
        let delay = xmtp_common::time::Duration::from_secs(300);
        let scw = || {
            ClientError::from(SignatureError::from(VerifierError::Other(Box::new(
                rate_limited(delay),
            ))))
        };
        let mut summary = crate::groups::summary::SyncSummary::default();
        summary.add_publish_err(GroupError::WrappedApi(rate_limited(delay)));
        let errors = [
            TaskWorkerError::KeyPackageMaintenance(KeyPackageMaintenanceError::Rotation(
                IdentityError::ApiClient(rate_limited(delay)),
            )),
            TaskWorkerError::Identity(IdentityError::ApiClient(rate_limited(delay))),
            TaskWorkerError::Group(GroupError::Client(scw())),
            TaskWorkerError::Group(GroupError::SyncFailedToWait(Box::new(summary))),
            TaskWorkerError::DeviceSync(DeviceSyncError::Client(ClientError::Api(rate_limited(
                delay,
            )))),
            TaskWorkerError::LoadGroup(crate::mls_store::MlsStoreError::Api(rate_limited(delay))),
        ];
        for error in errors {
            assert_eq!(error.rate_limit_delay(), Some(delay), "{error}");
        }
        let mut summary = crate::groups::summary::SyncSummary::default();
        summary.add_publish_err(GroupError::WrappedApi(rate_limited(delay)));
        let nested = [
            DeviceSyncError::Sync(Box::new(summary)).rate_limit_delay(),
            DeviceSyncError::MlsStore(crate::mls_store::MlsStoreError::Api(rate_limited(delay)))
                .rate_limit_delay(),
            crate::groups::mls_sync::GroupMessageProcessingError::Client(ClientError::Api(
                rate_limited(delay),
            ))
            .rate_limit_delay(),
            crate::subscriptions::SubscribeError::LocalDelivery(
                crate::subscriptions::local_delivery::LocalDeliveryError::NetworkFailure(Arc::new(
                    crate::subscriptions::incoming::IncomingError::Store(
                        crate::mls_store::MlsStoreError::Api(rate_limited(delay)),
                    ),
                )),
            )
            .rate_limit_delay(),
        ];
        assert!(
            nested.iter().all(|found| *found == Some(delay)),
            "{nested:?}"
        );
        // The worker restart loop reads the same delay.
        use crate::worker::NeedsDbReconnect;
        assert_eq!(
            NeedsDbReconnect::restart_delay(&TaskWorkerError::Identity(IdentityError::ApiClient(
                rate_limited(delay)
            ))),
            Some(delay)
        );
        assert_eq!(
            NeedsDbReconnect::restart_delay(&DeviceSyncError::Client(ClientError::Api(
                rate_limited(delay)
            ))),
            Some(delay)
        );
        assert_eq!(
            xmtp_common::RetryableError::rate_limit_delay(&scw()),
            Some(delay)
        );
    }

    fn no_runner_cfg() -> WorkerConfig {
        let mut cfg = WorkerConfig::default();
        cfg.enabled.insert(WorkerKind::TaskRunner, false);
        cfg
    }

    /// Insert a task row and return the stored row (found by its data_hash).
    fn seed(
        db: &impl QueryTasks,
        proto: TaskProto,
        next: i64,
        expires: i64,
        attempts: i32,
        max: i32,
    ) -> xmtp_db::tasks::Task {
        let hash = data_hash_for(&proto);
        let task = NewTask::builder()
            .originating_message_sequence_id(0)
            .next_attempt_at_ns(next)
            .expires_at_ns(expires)
            .attempts(attempts)
            .max_attempts(max)
            .build(proto)
            .unwrap();
        db.create_or_ignore_task(task).unwrap();
        db.get_tasks()
            .unwrap()
            .into_iter()
            .find(|t| t.data_hash == hash.as_ref())
            .unwrap()
    }

    fn seed_raw(db: &impl QueryTasks, data: Vec<u8>, expires: i64) -> xmtp_db::tasks::Task {
        let mut task = NewTask::builder()
            .originating_message_sequence_id(0)
            .next_attempt_at_ns(xmtp_common::time::now_ns() - 1)
            .expires_at_ns(expires)
            .build(unique_proto())
            .unwrap();
        task.data_hash = xmtp_common::sha256_bytes(&data);
        task.data = data;
        let hash = task.data_hash.clone();
        db.create_or_ignore_task(task).unwrap();
        db.get_tasks()
            .unwrap()
            .into_iter()
            .find(|t| t.data_hash == hash)
            .unwrap()
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn undecodable_and_unhandled_kinds_are_deferred() {
        tester!(alix, worker_config: no_runner_cfg());
        let db = alix.context.db();
        let now = xmtp_common::time::now_ns();
        let liveness = TaskProto {
            task: Some(TaskKind::KpLiveness(Default::default())),
        };
        for (data, expires) in [
            (vec![0x52, 0x00], i64::MAX), // Unknown kind: field 10, empty message.
            (liveness.encode_to_vec(), i64::MAX),
            (vec![0x52, 0x00], now - 1),
        ] {
            let row = seed_raw(&db, data, expires);
            let id = row.id;
            let before = xmtp_common::time::now_ns();
            TaskWorker::run_and_reschedule_task(row, &alix.context).await?;
            let after = db.get_tasks()?.into_iter().find(|t| t.id == id);
            if expires < now {
                assert!(after.is_none(), "expired unknown kind must be reaped");
            } else {
                let after = after.expect("unhandled kind must survive");
                assert_eq!(after.attempts, 0, "deferral must not count as a failure");
                assert!(
                    after.next_attempt_at_ns >= before + UNKNOWN_TASK_DEFER_NS,
                    "deferral must push the deadline out"
                );
                db.delete_task(id)?;
            }
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn done_deletes() {
        tester!(alix, worker_config: no_runner_cfg());
        let db = alix.context.db();
        let now = xmtp_common::time::now_ns();
        let row = seed(&db, unique_proto(), now - 1, i64::MAX, 0, i32::MAX);
        TaskWorker::run_and_reschedule_task(row, &alix.context).await?;
        assert!(db.get_tasks()?.is_empty(), "Done task must be deleted");
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn add_missing_installations_missing_group_deletes_task() {
        tester!(alix, worker_config: no_runner_cfg());
        let db = alix.context.db();
        let now = xmtp_common::time::now_ns();
        let proto = TaskProto {
            task: Some(TaskKind::AddMissingInstallations(
                xmtp_proto::xmtp::mls::database::AddMissingInstallations {
                    group_id: xmtp_common::rand_vec::<16>(),
                },
            )),
        };
        let row = seed(&db, proto, now - 1, i64::MAX, 0, 3);
        TaskWorker::run_and_reschedule_task(row, &alix.context).await?;
        assert!(
            db.get_tasks()?.is_empty(),
            "task for a nonexistent group must be deleted, not retried"
        );
    }

    /// The delay counts from when the rejection arrived, not from when the
    /// turn started, and a pull-in cannot run the task inside it.
    // verifies: RATE-041
    #[xmtp_common::test(unwrap_try = true)]
    async fn a_rate_limited_task_waits_the_delay_from_the_rejection() {
        use xmtp_common::time::Duration;
        tester!(alix, worker_config: no_runner_cfg());
        let db = alix.context.db();
        let now = xmtp_common::time::now_ns();
        let proto = unique_proto();
        let hash = data_hash_for(&proto);
        let row = seed(&db, proto, now - 1, i64::MAX, 0, i32::MAX);
        // Longer than the default 3 s first backoff, so only the rejection
        // instant, not the turn's start, gives the bound below.
        let delay = Duration::from_secs(5);
        *test_hooks::RATE_LIMIT_OVERRIDE.lock().unwrap() =
            Some((hash.to_vec(), Duration::from_millis(300), delay));
        TaskWorker::run_and_reschedule_task(row, &alix.context).await?;
        *test_hooks::RATE_LIMIT_OVERRIDE.lock().unwrap() = None;
        let refused = xmtp_common::time::now_ns();
        let task = || {
            db.get_tasks()
                .unwrap()
                .into_iter()
                .find(|t| t.data_hash == hash.as_ref())
        };
        let held = task().expect("the task is kept");
        let hold = refused - 10_000_000 + delay.as_nanos() as i64;
        assert_eq!(held.attempts, 1);
        assert!(held.next_attempt_at_ns >= hold);

        // A pull-in, such as a startup or Welcome nudge, leaves a task in
        // failure backoff on its stored schedule.
        enqueue_pull_in(&alix.context, hash, xmtp_common::time::now_ns(), i64::MAX)?;
        let pull_in = db
            .get_tasks()?
            .into_iter()
            .find(|t| t.data_hash != hash.as_ref())
            .expect("the pull-in task");
        TaskWorker::run_and_reschedule_task(pull_in, &alix.context).await?;
        let after = task().expect("a held task does not run");
        assert_eq!(after.attempts, 1);
        assert_eq!(after.next_attempt_at_ns, held.next_attempt_at_ns);
    }

    /// A rate-limit delay longer than a task's remaining lifetime does not
    /// expire it: the wait the delay adds past the backoff is added to the
    /// task's expiry, so the task runs once the delay has passed.
    // verifies: RATE-041
    #[xmtp_common::test(unwrap_try = true)]
    async fn a_rate_limit_delay_does_not_expire_the_task() {
        use xmtp_common::time::Duration;
        tester!(alix, worker_config: no_runner_cfg());
        let db = alix.context.db();
        let now = xmtp_common::time::now_ns();
        let proto = unique_proto();
        let hash = data_hash_for(&proto);
        let expires = now + xmtp_common::NS_IN_HOUR;
        let row = seed(&db, proto, now - 1, expires, 0, i32::MAX);
        let delay = Duration::from_secs(2 * 60 * 60);
        *test_hooks::RATE_LIMIT_OVERRIDE.lock().unwrap() =
            Some((hash.to_vec(), Duration::ZERO, delay));
        TaskWorker::run_and_reschedule_task(row, &alix.context).await?;
        *test_hooks::RATE_LIMIT_OVERRIDE.lock().unwrap() = None;
        let held = db
            .get_tasks()?
            .into_iter()
            .find(|t| t.data_hash == hash.as_ref())
            .expect("the task is kept");
        assert!(held.next_attempt_at_ns >= now + delay.as_nanos() as i64);
        assert!(
            held.expires_at_ns > held.next_attempt_at_ns,
            "the task does not expire before it is due"
        );
    }

    /// A rate-limited turn does not use up the task's attempts, so a task
    /// refused at its last attempt stays and runs after the delay.
    // verifies: RATE-041
    #[xmtp_common::test(unwrap_try = true)]
    async fn a_rate_limited_turn_keeps_the_tasks_attempts() {
        use xmtp_common::time::Duration;
        tester!(alix, worker_config: no_runner_cfg());
        let db = alix.context.db();
        let now = xmtp_common::time::now_ns();
        let proto = unique_proto();
        let hash = data_hash_for(&proto);
        let row = seed(&db, proto, now - 1, i64::MAX, 19, 20);
        *test_hooks::RATE_LIMIT_OVERRIDE.lock().unwrap() =
            Some((hash.to_vec(), Duration::ZERO, Duration::from_millis(1)));
        TaskWorker::run_and_reschedule_task(row, &alix.context).await?;
        let task = || {
            db.get_tasks()
                .unwrap()
                .into_iter()
                .find(|t| t.data_hash == hash.as_ref())
        };
        let mut held = task().expect("the refused task is kept");
        assert_eq!(held.attempts, 19);
        held.next_attempt_at_ns = xmtp_common::time::now_ns() - 1;
        TaskWorker::run_and_reschedule_task(held, &alix.context).await?;
        *test_hooks::RATE_LIMIT_OVERRIDE.lock().unwrap() = None;
        let held = task().expect("a second refusal keeps it too");
        assert_eq!(held.attempts, 19);
    }

    /// When the retry time of a rate-limited task cannot be stored, the turn
    /// fails with the rejection, so the worker restarts after the delay instead
    /// of resending the still-due task after its one-second restart delay.
    // verifies: RATE-041
    #[xmtp_common::test(unwrap_try = true)]
    async fn a_rate_limited_task_whose_hold_is_not_stored_holds_the_restart() {
        use crate::worker::NeedsDbReconnect;
        use xmtp_common::time::Duration;
        tester!(alix, worker_config: no_runner_cfg());
        let db = alix.context.db();
        let now = xmtp_common::time::now_ns();
        let proto = unique_proto();
        let hash = data_hash_for(&proto);
        let row = seed(&db, proto, now - 1, i64::MAX, 0, 20);
        let delay = Duration::from_secs(5 * 60);
        *test_hooks::RATE_LIMIT_OVERRIDE.lock().unwrap() =
            Some((hash.to_vec(), Duration::ZERO, delay));
        *test_hooks::FAIL_RESCHEDULE.lock().unwrap() = true;
        let result = TaskWorker::run_and_reschedule_task(row, &alix.context).await;
        *test_hooks::RATE_LIMIT_OVERRIDE.lock().unwrap() = None;
        let error = result.expect_err("the write failed");
        assert_eq!(error.restart_delay(), Some(delay));
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn recurring_task_advances_and_does_not_hot_loop() {
        tester!(alix, worker_config: no_runner_cfg());
        let db = alix.context.db();
        let now = xmtp_common::time::now_ns();
        let proto = unique_proto();
        // High attempts + past-due: only the i64::MAX/i32::MAX seed keeps the
        // reaper (expires < now || attempts >= max) from deleting it.
        let row = seed(&db, proto.clone(), now - 1, i64::MAX, 1_000_000, i32::MAX);
        let target = now + xmtp_common::NS_IN_DAY;
        *test_hooks::RESCHEDULE_OVERRIDE.lock().unwrap() =
            Some((data_hash_for(&proto).to_vec(), target));

        TaskWorker::run_and_reschedule_task(row, &alix.context).await?;

        *test_hooks::RESCHEDULE_OVERRIDE.lock().unwrap() = None;
        let after = db.get_tasks()?.pop().expect("recurring row must survive");
        assert_eq!(
            after.next_attempt_at_ns, target,
            "deadline must ADVANCE, not stay past-due"
        );
        assert_eq!(after.attempts, 0, "success resets the backoff counter");
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn not_yet_due_task_is_not_run_early() {
        tester!(alix, worker_config: no_runner_cfg());
        let db = alix.context.db();
        let now = xmtp_common::time::now_ns();
        let proto = unique_proto();
        let far = now + 30 * xmtp_common::NS_IN_DAY;
        let row = seed(&db, proto.clone(), far, i64::MAX, 0, i32::MAX);
        // If the guard failed and the task ran, the hook would rewrite next_attempt.
        *test_hooks::RESCHEDULE_OVERRIDE.lock().unwrap() =
            Some((data_hash_for(&proto).to_vec(), now));

        TaskWorker::run_and_reschedule_task(row, &alix.context).await?;

        *test_hooks::RESCHEDULE_OVERRIDE.lock().unwrap() = None;
        let after = db.get_tasks()?.pop().unwrap();
        assert_eq!(
            after.next_attempt_at_ns, far,
            "not-yet-due task must not run on a 1-day-cap wake"
        );
        assert_eq!(
            after.attempts, 0,
            "not-yet-due task must not have its attempts incremented"
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn pull_in_arm_lowers_existing_target() {
        tester!(alix, worker_config: no_runner_cfg());
        let db = alix.context.db();
        let now = xmtp_common::time::now_ns();
        // Far-future recurring target.
        let target_proto = unique_proto();
        let target_hash = data_hash_for(&target_proto);
        let far = now + 30 * xmtp_common::NS_IN_DAY;
        seed(&db, target_proto, far, i64::MAX, 0, i32::MAX);
        // Due pull-in aimed at it.
        enqueue_pull_in(&alix.context, target_hash, now + 1_000, i64::MAX)?;
        let pull_in_row = db
            .get_tasks()?
            .into_iter()
            .find(|t| t.data_hash != target_hash.as_ref())
            .expect("pull-in row exists");

        TaskWorker::run_and_reschedule_task(pull_in_row, &alix.context).await?;

        let rows = db.get_tasks()?;
        let target = rows
            .iter()
            .find(|t| t.data_hash == target_hash.as_ref())
            .expect("target survives");
        assert_eq!(
            target.next_attempt_at_ns,
            now + 1_000,
            "arm must lower the target to the ceiling"
        );
        assert_eq!(rows.len(), 1, "applied pull-in must self-delete");
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn pull_in_task_runs_and_pulls_in() {
        tester!(alix); // live TaskRunner
        let db = alix.context.db();
        let now = xmtp_common::time::now_ns();
        let proto = unique_proto();
        // Ceiling 1 day out: the exact-equality assert below proves lowering
        // regardless of magnitude, and a generous ceiling means the target can't
        // become due (and get dispatched/deleted) even on a pathologically slow
        // CI runner — only constraints are "> test wall time" and "!= far".
        let ceiling = now + xmtp_common::NS_IN_DAY;
        let far = now + 30 * xmtp_common::NS_IN_DAY;
        seed(&db, proto.clone(), far, i64::MAX, 0, i32::MAX);
        let hash = data_hash_for(&proto);

        enqueue_pull_in(&alix.context, hash, ceiling, i64::MAX)?;

        // Poll up to ~10s (wasm-safe): the worker dispatches the due pull-in,
        // which lowers the target and self-deletes.
        let mut pulled = false;
        for _ in 0..50u32 {
            xmtp_common::time::sleep(std::time::Duration::from_millis(200)).await;
            let rows = db.get_tasks()?;
            let target_ok = rows
                .iter()
                .any(|t| t.data_hash == hash.as_ref() && t.next_attempt_at_ns == ceiling);
            let pull_in_gone = !rows.iter().any(|t| {
                t.data_hash != hash.as_ref() && t.next_attempt_at_ns <= xmtp_common::time::now_ns()
            });
            if target_ok && pull_in_gone {
                pulled = true;
                break;
            }
        }
        assert!(
            pulled,
            "worker must apply the pull-in and lower the target deadline"
        );
    }
}
