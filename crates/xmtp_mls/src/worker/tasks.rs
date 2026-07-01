use crate::{
    context::XmtpSharedContext,
    worker::{
        NeedsDbReconnect, Worker, WorkerFactory, WorkerKind,
        device_sync::{ArchiveOptions, DeviceSyncClient, DeviceSyncError},
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

/// `Done` = one-shot, row deleted. `RescheduleAt(ns)` = recurring, row kept and
/// advanced to that absolute deadline.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum TaskOutcome {
    Done,
    // Constructed only by the cfg(test) hook until the first real recurring
    // task variant lands. cfg_attr(expect) self-cleans: when non-test code
    // first constructs RescheduleAt, the unfulfilled expectation becomes a
    // compiler warning that forces removal of this attribute.
    #[cfg_attr(not(test), expect(dead_code))]
    RescheduleAt(i64),
}

#[cfg(test)]
pub(crate) mod test_hooks {
    use std::sync::Mutex;
    /// `(target_data_hash, deadline)` → `run_task` returns `RescheduleAt(deadline)`
    /// for the matching task. Reset at test end; assumes process-per-test isolation.
    pub(crate) static RESCHEDULE_OVERRIDE: Mutex<Option<(Vec<u8>, i64)>> = Mutex::new(None);
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
        }
    }
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

/// Durably enqueue a `PullInDeadline` for `target_data_hash`, then wake the loop.
/// Row is committed before the wake; duplicates coalesce on data_hash. Lifetime is
/// bounded by `expires_at_ns` alone (pass `i64::MAX` for delivery-critical nudges).
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "KP-consumer plan adds production callers")
)]
pub(crate) fn enqueue_pull_in<Context: XmtpSharedContext>(
    context: &Context,
    target_data_hash: Vec<u8>,
    not_later_than_ns: i64,
    expires_at_ns: i64,
) -> Result<(), xmtp_db::StorageError> {
    let now = xmtp_common::time::now_ns();
    let task = xmtp_db::tasks::NewTask::builder()
        .originating_message_sequence_id(0)
        .originating_message_originator_id(0)
        .next_attempt_at_ns(now) // the pull-in itself is due immediately
        .expires_at_ns(expires_at_ns)
        .max_attempts(i32::MAX) // lifetime bounded by expires_at_ns, not retries
        .build(xmtp_proto::xmtp::mls::database::Task {
            task: Some(xmtp_proto::xmtp::mls::database::task::Task::PullInDeadline(
                xmtp_proto::xmtp::mls::database::PullInDeadline {
                    target_data_hash,
                    not_later_than_ns,
                },
            )),
        })?;
    context.db().create_or_ignore_task(task)?;
    context.task_channels().wake();
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
                let attempts = task.attempts + 1;
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
        #[cfg(test)]
        if let Some((hash, t)) = test_hooks::RESCHEDULE_OVERRIDE.lock().unwrap().clone()
            && task.data_hash == hash
        {
            return Ok(TaskOutcome::RescheduleAt(t));
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
            Some(xmtp_proto::xmtp::mls::database::task::Task::PullInDeadline(p)) => {
                // Runs on the worker thread — the sole rescheduler of existing
                // rows' deadlines — so no transaction is needed (inserts happen
                // off-thread; the precise invariant is over `next_attempt_at_ns`
                // mutation; see the recurrence design).
                context
                    .db()
                    .pull_in_task_deadline(&p.target_data_hash, p.not_later_than_ns)?;
            }
            Some(xmtp_proto::xmtp::mls::database::task::Task::KpRotation(_))
            | Some(xmtp_proto::xmtp::mls::database::task::Task::KpDeletion(_)) => {
                // Minimal arms: real handlers land with the KP-consumer impl.
                // Nothing seeds these singletons yet, so dropping is safe.
                tracing::warn!(
                    "KP task {} received before handler landed; dropping",
                    task.id
                );
                context.db().delete_task(task.id)?;
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
            .originating_message_originator_id(0)
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
            .find(|t| t.data_hash == hash)
            .unwrap()
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
    async fn recurring_task_advances_and_does_not_hot_loop() {
        tester!(alix, worker_config: no_runner_cfg());
        let db = alix.context.db();
        let now = xmtp_common::time::now_ns();
        let proto = unique_proto();
        let row = seed(&db, proto.clone(), now - 1, i64::MAX, 5, i32::MAX);
        let target = now + xmtp_common::NS_IN_DAY;
        *test_hooks::RESCHEDULE_OVERRIDE.lock().unwrap() = Some((data_hash_for(&proto), target));

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
    async fn never_expire_seed_survives_reaper() {
        tester!(alix, worker_config: no_runner_cfg());
        let db = alix.context.db();
        let now = xmtp_common::time::now_ns();
        let proto = unique_proto();
        // High attempts + past-due: only the i64::MAX/i32::MAX seed keeps the
        // reaper (expires < now || attempts >= max) from deleting it.
        let row = seed(&db, proto.clone(), now - 1, i64::MAX, 1_000_000, i32::MAX);
        *test_hooks::RESCHEDULE_OVERRIDE.lock().unwrap() =
            Some((data_hash_for(&proto), now + xmtp_common::NS_IN_DAY));

        TaskWorker::run_and_reschedule_task(row, &alix.context).await?;

        *test_hooks::RESCHEDULE_OVERRIDE.lock().unwrap() = None;
        let after = db.get_tasks()?;
        assert_eq!(after.len(), 1, "never-expire seed must not be reaped");
        assert_eq!(
            after[0].next_attempt_at_ns,
            now + xmtp_common::NS_IN_DAY,
            "seed must have been rescheduled to the target deadline"
        );
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
        *test_hooks::RESCHEDULE_OVERRIDE.lock().unwrap() = Some((data_hash_for(&proto), now));

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
        enqueue_pull_in(&alix.context, target_hash.clone(), now + 1_000, i64::MAX)?;
        let pull_in_row = db
            .get_tasks()?
            .into_iter()
            .find(|t| t.data_hash != target_hash)
            .expect("pull-in row exists");

        TaskWorker::run_and_reschedule_task(pull_in_row, &alix.context).await?;

        let rows = db.get_tasks()?;
        let target = rows
            .iter()
            .find(|t| t.data_hash == target_hash)
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

        enqueue_pull_in(&alix.context, hash.clone(), ceiling, i64::MAX)?;

        // Poll up to ~10s (wasm-safe): the worker dispatches the due pull-in,
        // which lowers the target and self-deletes.
        let mut pulled = false;
        for _ in 0..50u32 {
            xmtp_common::time::sleep(std::time::Duration::from_millis(200)).await;
            let rows = db.get_tasks()?;
            let target_ok = rows
                .iter()
                .any(|t| t.data_hash == hash && t.next_attempt_at_ns == ceiling);
            let pull_in_gone = !rows.iter().any(|t| {
                t.data_hash != hash && t.next_attempt_at_ns <= xmtp_common::time::now_ns()
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
