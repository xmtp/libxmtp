use crate::{
    context::XmtpSharedContext,
    worker::{
        NeedsDbReconnect, Worker, WorkerFactory, WorkerKind,
        device_sync::{ArchiveOptions, DeviceSyncClient, DeviceSyncError},
        key_package_maintenance::KeyPackageMaintenance,
    },
};
use prost::Message;
use std::sync::Arc;
use xmtp_common::Event;
use xmtp_db::tasks::{NewTask as DbNewTask, QueryTasks, Task as DbTask};
use xmtp_db::{StorageError, diesel};
use xmtp_macro::log_event;
use xmtp_proto::{
    types::{WelcomeMessage, WelcomeMessageType},
    xmtp::mls::database::Task as TaskProto,
};

/// What `run_task` decided should happen to the task row after it ran.
///
/// One-shot tasks resolve to [`TaskOutcome::Done`] (the row is deleted).
/// The recurring `KpMaintenance` singleton resolves to
/// [`TaskOutcome::Reschedule`], carrying the fallback next-deadline so the
/// caller can atomically re-arm the row instead of deleting it.
#[derive(Debug)]
enum TaskOutcome {
    /// The task is finished; delete its row.
    Done,
    /// The task is recurring; re-arm its row. The `i64` is the fallback next
    /// deadline (ns) used if a fresher in-txn deadline isn't found.
    Reschedule(i64),
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
    #[error("task runner receiver locked")]
    ReceiverLocked,
    #[error("Cannot send sync archives without metrics handle")]
    MissingMetrics,
    #[error(transparent)]
    Conversion(#[from] xmtp_proto::ConversionError),
    #[error("key package maintenance error: {0}")]
    KeyPackageMaintenance(#[from] crate::worker::key_package_cleaner::KeyPackagesCleanerError),
}

impl NeedsDbReconnect for TaskWorkerError {
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
            TaskWorkerError::ReceiverLocked => false,
            TaskWorkerError::MissingMetrics => false,
            TaskWorkerError::Conversion(_) => false,
            // Forward through KeyPackagesCleanerError's own classifier so a
            // dropped pool surfacing during delete/rotate restarts the worker.
            TaskWorkerError::KeyPackageMaintenance(e) => e.needs_db_reconnect(),
        }
    }
}

/// Is this a recurring task (the `KpMaintenance` singleton)? Recurring tasks are
/// never reaped — they are re-armed in place instead of deleted. Returns false
/// on a decode error (treat an undecodable row as a one-shot so it can be reaped).
fn task_is_recurring(task: &DbTask) -> bool {
    matches!(
        TaskProto::decode(task.data.as_slice())
            .ok()
            .and_then(|t| t.task),
        Some(xmtp_proto::xmtp::mls::database::task::Task::KpMaintenance(
            _
        ))
    )
}

/// Message to the TaskRunner loop.
pub enum TaskMessage {
    /// Persist a new durable task row.
    New(DbNewTask),
    /// No-op wake: the task row was already inserted directly in a DB
    /// transaction; receiving this just makes the loop re-read the tasks table.
    Wake,
}

#[derive(Clone)]
pub struct TaskWorkerChannels {
    // Using unbounded to avoid potential issues with the receiver queue being full
    pub task_sender: tokio::sync::mpsc::UnboundedSender<TaskMessage>,
    pub task_receiver: Arc<tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<TaskMessage>>>,
}

impl Default for TaskWorkerChannels {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskWorkerChannels {
    pub fn new() -> Self {
        let (task_sender, task_receiver) = tokio::sync::mpsc::unbounded_channel();
        Self {
            task_sender,
            task_receiver: Arc::new(tokio::sync::Mutex::new(task_receiver)),
        }
    }
    pub fn send(&self, new_task: DbNewTask) {
        self.task_sender
            .send(TaskMessage::New(new_task))
            .expect("Task receiver is owned by same struct");
    }
    /// Wake the TaskRunner to re-evaluate its next due task. Use after inserting
    /// a task row directly in a DB transaction (best-effort; idempotent).
    pub fn wake(&self) {
        self.task_sender
            .send(TaskMessage::Wake)
            .expect("Task receiver is owned by same struct");
    }
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
    channels: TaskWorkerChannels,
}

#[xmtp_common::async_trait]
impl<Context> Worker for TaskWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    fn kind(&self) -> WorkerKind {
        WorkerKind::TaskRunner
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
        let channels = context.task_channels().clone();
        Self { context, channels }
    }
    pub async fn run(&mut self) -> Result<(), TaskWorkerError> {
        let mut receiver = match self.channels.task_receiver.try_lock() {
            Ok(receiver) => receiver,
            Err(_) => return Err(TaskWorkerError::ReceiverLocked),
        };
        loop {
            let next_task = self.context.db().get_next_task()?;
            let next_wakeup = Self::next_wakeup(
                next_task.as_ref().map(|t| t.next_attempt_at_ns),
                xmtp_common::time::now_ns(),
            );
            tokio::select! {
                msg = receiver.recv() => {
                    // A Wake is a no-op here: its row is already in the DB, and
                    // any recv loops back to recompute the next due task.
                    if let TaskMessage::New(task) =
                        msg.expect("Task sender is owned by the task worker")
                    {
                        self.context.db().create_task(task)?;
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
    async fn run_and_reschedule_task(
        task: DbTask,
        context: &Context,
    ) -> Result<(), TaskWorkerError> {
        let now = xmtp_common::time::now_ns();
        let recurring = task_is_recurring(&task);

        if task.expires_at_ns < now || task.attempts >= task.max_attempts {
            if recurring {
                // The recurring singleton is NEVER deleted. If it looks dead
                // (expired or attempts exhausted), renew it in place via the
                // atomic re-arm, then FALL THROUGH so it still dispatches this
                // turn. Because the fetched `task` value we keep using below was
                // already due/dead (next_attempt_at_ns <= now), the
                // `task.next_attempt_at_ns > now` guard further down won't
                // early-return, so a dead/overdue recurring task runs now.
                let fallback = KeyPackageMaintenance::next_deadline(context)?.max(now);
                context.db().reschedule_kp_task(
                    task.id,
                    fallback,
                    fallback + xmtp_common::NS_IN_DAY,
                )?;
                // fall through to dispatch
            } else {
                // one-shot: reap it
                context.db().delete_task(task.id)?;
                return Ok(());
            }
        }
        if task.next_attempt_at_ns > now {
            // This will get called again
            tracing::warn!(
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
            Ok(TaskOutcome::Reschedule(fallback)) => {
                // Atomic floor-re-read closes the nudge race: reschedule_kp_task
                // re-reads the live KP deadline inside the same txn and writes
                // MIN(fallback, fresh), so a rotation queued during the run is
                // never clobbered to the far fallback deadline.
                context.db().reschedule_kp_task(
                    task.id,
                    fallback,
                    fallback + xmtp_common::NS_IN_DAY,
                )?;
            }
            Err(error) => {
                // Cap a recurring task's attempts strictly below max_attempts so
                // the reaper (attempts >= max_attempts) can never delete the
                // singleton. One-shot tasks keep the plain +1 so they can age out.
                let attempts = if recurring {
                    (task.attempts + 1).min(task.max_attempts - 1)
                } else {
                    task.attempts + 1
                };
                let attempt_scaling_factor = (task.backoff_scaling_factor as f64).powi(attempts);
                let next_attempt_duration = (((task.initial_backoff_duration_ns as f64)
                    * attempt_scaling_factor) as i64)
                    .min(task.max_backoff_duration_ns);
                let next_attempt_at_ns = now.saturating_add(next_attempt_duration);
                tracing::warn!(%error, "Task {} retry failed. Retrying in {next_attempt_duration}ns", task.id);
                match context
                    .db()
                    .update_task(task.id, attempts, now, next_attempt_at_ns)
                {
                    Ok(_) => {}
                    // The row was concurrently deleted (e.g. a dead-row cleanup in
                    // upsert_pending_self_remove_task crossed this retry). Nothing
                    // left to reschedule — don't abort the worker loop.
                    Err(StorageError::DieselResult(diesel::result::Error::NotFound)) => {
                        tracing::debug!("Task {} vanished before reschedule; skipping", task.id);
                    }
                    Err(e) => return Err(e.into()),
                }
            }
        }
        Ok(())
    }
    async fn run_task(task: &DbTask, context: &Context) -> Result<TaskOutcome, TaskWorkerError> {
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
            Some(xmtp_proto::xmtp::mls::database::task::Task::ProcessWelcomePointer(
                welcome_pointer,
            )) => {
                Self::process_welcome_pointer(task, welcome_pointer, context).await?;
            }
            Some(xmtp_proto::xmtp::mls::database::task::Task::SendSyncArchive(
                send_sync_archive,
            )) => {
                let Some(metrics) = context.sync_metrics().clone() else {
                    return Err(TaskWorkerError::MissingMetrics);
                };
                let Some(proto_options) = send_sync_archive.options.clone() else {
                    tracing::warn!(
                        "SendSyncArchive task has no archive options. Unable to process."
                    );
                    return Ok(TaskOutcome::Done);
                };
                let options: ArchiveOptions = proto_options.into();

                let client = DeviceSyncClient::new(context.clone(), metrics);

                let pin = send_sync_archive.pin.clone().unwrap_or_else(|| {
                    let pin = xmtp_common::rand_string::<5>();
                    format!("{pin:04}")
                });

                client
                    .send_archive(
                        &options,
                        &xmtp_proto::types::GroupId::try_from(
                            send_sync_archive.sync_group_id.as_slice(),
                        )?,
                        &pin,
                        &send_sync_archive.server_url,
                    )
                    .await
                    .inspect_err(|e| {
                        log_event!(
                            Event::DeviceSyncArchiveUploadFailure,
                            context.installation_id(),
                            group_id = send_sync_archive.sync_group_id,
                            pin = send_sync_archive.pin(),
                            err = %e
                        )
                    })?;
            }
            Some(xmtp_proto::xmtp::mls::database::task::Task::ProcessPendingSelfRemove(
                pending,
            )) => {
                Self::process_pending_self_remove(task, pending, context).await?;
            }
            Some(xmtp_proto::xmtp::mls::database::task::Task::KpMaintenance(_)) => {
                KeyPackageMaintenance::delete_expired(context)?;
                KeyPackageMaintenance::rotate_if_needed(context).await?;
                // next_deadline is read LAST (after the work) so a rotation
                // queued during this run is reflected; reschedule_kp_task
                // re-reads it atomically inside the re-arm txn anyway.
                let fallback = KeyPackageMaintenance::next_deadline(context)?;
                return Ok(TaskOutcome::Reschedule(fallback));
            }
            None => {
                tracing::error!("Task {} has no data. Deleting.", task.id);
                context.db().delete_task(task.id)?;
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
    async fn process_welcome_pointer(
        task: &DbTask,
        welcome_pointer: xmtp_proto::xmtp::mls::message_contents::WelcomePointer,
        context: &Context,
    ) -> Result<(), TaskWorkerError> {
        let decrypted_welcome_pointer = WelcomeMessage {
            cursor: xmtp_proto::types::Cursor::new(
                task.originating_message_sequence_id as u64,
                task.originating_message_originator_id as u32,
            ),
            created_ns: chrono::DateTime::from_timestamp_nanos(task.created_at_ns),
            variant: WelcomeMessageType::DecryptedWelcomePointer(
                welcome_pointer
                    .try_into()
                    .map_err(crate::groups::GroupError::from)?,
            ),
        };
        let welcome_service = crate::groups::welcome_sync::WelcomeService::new(context.clone());
        let validator = crate::groups::InitialMembershipValidator::new(context);
        let group = welcome_service
            // cursor_increment is false because the cursor has already been incremented
            .process_new_welcome(&decrypted_welcome_pointer, false, validator)
            .await?;
        if let Some(group) = group {
            context
                .local_events()
                .send(crate::subscriptions::LocalEvents::NewGroup(group.group_id))
                .ok();
        }
        Ok(())
    }
}

#[cfg(test)]
#[cfg(not(target_arch = "wasm32"))]
mod tests {
    use super::*;
    use crate::tester;
    use xmtp_db::tasks::NewTask as DbNewTask;
    use xmtp_proto::xmtp::mls::database::{
        ProcessPendingSelfRemove, Task as TaskProto, task::Task as TaskKind,
    };

    /// Seed a task row with `proto` payload that is immediately due
    /// (`next_attempt_at_ns = now`) and pull it back out as a `DbTask`.
    fn seed_task<C: XmtpSharedContext>(context: &C, proto: TaskProto) -> DbTask {
        let now = xmtp_common::time::now_ns();
        let new_task = DbNewTask::builder()
            .originating_message_sequence_id(0)
            .originating_message_originator_id(0)
            .next_attempt_at_ns(now)
            .expires_at_ns(now + xmtp_common::NS_IN_DAY)
            .build(proto)
            .unwrap();
        context.db().create_task(new_task).unwrap()
    }

    /// Existing arms keep their behavior: a `ProcessPendingSelfRemove` task with a
    /// malformed group_id is a drop-case — `run_task` deletes the row and returns
    /// `Ok(TaskOutcome::Done)`, and `run_and_reschedule_task` reaps it (no row
    /// left, no reschedule). Only the return-type wrapping changed.
    #[xmtp_common::test(unwrap_try = true)]
    async fn existing_arms_return_done() {
        tester!(alix, disable_workers);
        let context = alix.context.clone();

        // A 3-byte group_id can never convert to a GroupId -> the arm drops it.
        let proto = TaskProto {
            task: Some(TaskKind::ProcessPendingSelfRemove(
                ProcessPendingSelfRemove {
                    group_id: vec![1, 2, 3],
                },
            )),
        };
        let task = seed_task(&context, proto);
        assert!(!task_is_recurring(&task), "self-remove is not recurring");

        let outcome = TaskWorker::run_task(&task, &context).await?;
        assert!(
            matches!(outcome, TaskOutcome::Done),
            "malformed self-remove resolves to Done"
        );
        // The drop-arm already deleted the row inside run_task.
        assert!(
            !context.db().get_tasks()?.iter().any(|t| t.id == task.id),
            "malformed self-remove row was deleted by the arm"
        );
    }

    /// Load-bearing regression guard: the recurring KpMaintenance singleton is
    /// NEVER reaped. Forced dead (attempts >= max_attempts), it is re-armed in
    /// place (still present, attempts reset to 0). A non-recurring task in the
    /// same dead state IS deleted.
    #[xmtp_common::test(unwrap_try = true)]
    async fn recurring_task_is_never_reaped() {
        tester!(alix, disable_workers);
        let context = alix.context.clone();

        // --- recurring: forced dead, must survive ---
        context.db().ensure_kp_maintenance_task()?;
        let kp = context
            .db()
            .get_tasks()?
            .into_iter()
            .find(task_is_recurring)
            .expect("ensure_kp_maintenance_task seeded a KP row");
        assert!(task_is_recurring(&kp));

        // Force it dead: attempts == max_attempts (the reaper's kill condition).
        let now = xmtp_common::time::now_ns();
        let dead_kp = context.db().update_task(kp.id, kp.max_attempts, now, now)?;
        assert!(
            dead_kp.expires_at_ns < now || dead_kp.attempts >= dead_kp.max_attempts,
            "KP task is in a dead state before the run"
        );

        TaskWorker::run_and_reschedule_task(dead_kp, &context).await?;

        let after = context
            .db()
            .get_tasks()?
            .into_iter()
            .find(task_is_recurring)
            .expect("recurring KP task must NOT be reaped");
        assert_eq!(after.attempts, 0, "re-armed KP task has attempts reset");

        // --- non-recurring: same dead state, IS deleted ---
        let proto = TaskProto {
            task: Some(TaskKind::ProcessPendingSelfRemove(
                ProcessPendingSelfRemove {
                    group_id: vec![9, 9, 9],
                },
            )),
        };
        let oneshot = seed_task(&context, proto);
        let dead_oneshot = context
            .db()
            .update_task(oneshot.id, oneshot.max_attempts, now, now)?;
        assert!(!task_is_recurring(&dead_oneshot));

        TaskWorker::run_and_reschedule_task(dead_oneshot, &context).await?;
        assert!(
            !context.db().get_tasks()?.iter().any(|t| t.id == oneshot.id),
            "dead one-shot task was reaped"
        );
    }

    /// The KpMaintenance arm runs delete/rotate and resolves to
    /// `Reschedule(_)` (never `Done`), so the recurring row is re-armed, not
    /// deleted. A fresh client needs no rotation, so this stays DB-only.
    #[xmtp_common::test(unwrap_try = true)]
    async fn kp_maintenance_arm_reschedules() {
        tester!(alix, disable_workers);
        let context = alix.context.clone();

        let task = seed_task(&context, xmtp_db::tasks::kp_maintenance_task_proto());
        assert!(task_is_recurring(&task));

        let outcome = TaskWorker::run_task(&task, &context).await?;
        assert!(
            matches!(outcome, TaskOutcome::Reschedule(_)),
            "KpMaintenance arm resolves to Reschedule, got {outcome:?}"
        );
    }
}
