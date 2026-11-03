pub use crate::core::task_channels::TaskWorkerChannels;
use crate::{
    context::CoreContext,
    core::events::InternalEvent,
    worker::{
        BoxedWorker, EVENT_BUFFER, NeedsDbReconnect, NewWorker, Worker, WorkerFactory, WorkerKind,
    },
};
use std::{marker::PhantomData, sync::Arc};
use xmtp_common::{MaybeSend, MaybeSync, RetryableError};
use xmtp_db::tasks::{QueryTasks, Task as DbTask, TaskDataHash};
use xmtp_db::{StorageError, diesel};
use xmtp_events::{EventEnvelope, EventFilter, EventKind, Subscription};
use xmtp_proto::xmtp::mls::database::PullInDeadline;

/// One task-runner turn for handler tests that run outside a running worker.
#[cfg(any(test, feature = "test-utils"))]
#[doc(hidden)]
pub mod test_support {
    use super::{DbTask, PullInDeadline, StorageError, TaskHandlers, TaskWorker};
    use crate::context::CoreContext;
    use xmtp_db::tasks::{NewTask, QueryTasks, TaskDataHash, data_hash_for};
    use xmtp_proto::xmtp::mls::database::{Task as TaskProto, task::Task as TaskKind};

    /// Run `task` through one turn of `worker`: the expiry and due checks,
    /// the dispatch, and the delete or reschedule that follows.
    pub async fn run_and_reschedule_task<Context, Handlers>(
        worker: &TaskWorker<Context, Handlers>,
        task: DbTask,
    ) -> Result<(), Handlers::Error>
    where
        Context: CoreContext,
        Handlers: TaskHandlers<Context>,
    {
        worker.run_and_reschedule_task(task).await
    }

    /// A unique one-shot payload: a PullInDeadline aimed at a random, nonexistent
    /// target. Applying it is a no-op; its data_hash is unique per call.
    pub fn unique_proto() -> TaskProto {
        TaskProto {
            task: Some(TaskKind::PullInDeadline(PullInDeadline {
                target_data_hash: xmtp_common::rand_vec::<32>(),
                not_later_than_ns: 0,
            })),
        }
    }

    /// Insert a task row and return the stored row (found by its data_hash).
    pub fn seed(
        db: &impl QueryTasks,
        proto: TaskProto,
        next: i64,
        expires: i64,
        attempts: i32,
        max: i32,
    ) -> DbTask {
        let hash = data_hash_for(&proto);
        let task = NewTask::builder()
            .originating_message_sequence_id(0)
            .next_attempt_at_ns(next)
            .expires_at_ns(expires)
            .attempts(attempts)
            .max_attempts(max)
            .build(proto)
            .expect("a task row");
        db.create_or_ignore_task(task).expect("a stored task");
        db.get_tasks()
            .expect("the stored tasks")
            .into_iter()
            .find(|t| t.data_hash == hash.as_ref())
            .expect("the seeded task")
    }

    /// Queue a pull-in of the task with `target_data_hash`, as a nudge does.
    pub fn enqueue_pull_in<Context: CoreContext>(
        context: &Context,
        target_data_hash: TaskDataHash,
        not_later_than_ns: i64,
        expires_at_ns: i64,
    ) -> Result<(), StorageError> {
        super::enqueue_pull_in(context, target_data_hash, not_later_than_ns, expires_at_ns)
    }
}

/// `Done` = one-shot, row deleted. `RescheduleAt(ns)` = recurring, row kept and
/// advanced to that absolute deadline.
#[derive(Debug, PartialEq, Eq)]
pub enum TaskOutcome {
    Done,
    RescheduleAt(i64),
}

/// Task-runner controls for tests. The runner reads them; tests set them.
/// Each assumes process-per-test isolation.
#[cfg(any(test, feature = "test-hooks"))]
#[doc(hidden)]
pub mod test_hooks {
    use std::sync::Mutex;

    static FAIL_RESCHEDULE: Mutex<bool> = Mutex::new(false);
    type ScheduledWitness = (Vec<u8>, tokio::sync::oneshot::Sender<()>);
    static TASK_SCHEDULED_CONSUMED: Mutex<Option<ScheduledWitness>> = Mutex::new(None);

    /// Makes the runner's next retry schedule write fail, once.
    pub fn fail_next_reschedule() {
        *FAIL_RESCHEDULE.lock().expect("task test hooks") = true;
    }

    pub(super) fn take_reschedule_failure() -> bool {
        std::mem::take(&mut *FAIL_RESCHEDULE.lock().expect("task test hooks"))
    }

    /// Sends on `consumed` once the task runner of `installation_id` reads a
    /// `TaskScheduled` event.
    pub fn on_task_scheduled_consumed(
        installation_id: Vec<u8>,
        consumed: tokio::sync::oneshot::Sender<()>,
    ) {
        *TASK_SCHEDULED_CONSUMED.lock().expect("task test hooks") =
            Some((installation_id, consumed));
    }

    pub(super) fn take_task_scheduled_witness(
        installation_id: &[u8],
    ) -> Option<tokio::sync::oneshot::Sender<()>> {
        let mut hook = TASK_SCHEDULED_CONSUMED.lock().expect("task test hooks");
        if hook
            .as_ref()
            .is_some_and(|(installation, _)| installation.as_slice() == installation_id)
        {
            hook.take().map(|(_, consumed)| consumed)
        } else {
            None
        }
    }
}

/// The handlers a [`TaskWorker`] runs. The runner owns each task's schedule:
/// expiry, attempts, backoff and rate-limit holds. The handlers run one row.
#[xmtp_common::async_trait]
pub trait TaskHandlers<Context>: MaybeSend + MaybeSync {
    /// The error of a failed run. The runner reads its rate-limit delay and
    /// its reconnect signal, and reports its own storage failures as this type.
    type Error: RetryableError
        + NeedsDbReconnect
        + From<StorageError>
        + std::error::Error
        + MaybeSend
        + MaybeSync
        + 'static;

    /// Run one due task. Gets the whole row: handlers use `task.id`, for
    /// example to delete it.
    async fn dispatch(&self, context: &Context, task: &DbTask) -> Result<TaskOutcome, Self::Error>;

    /// Runs when the runner starts, before it reads a task.
    fn on_start(&self, _context: &Context) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Runs once for each batch of events the runner reads that holds an
    /// event other than its own `TaskScheduled`, and gets the whole batch.
    /// Public events such as `ConsentChanged` and `ArchiveRestored` arrive here.
    fn on_events(
        &self,
        _context: &Context,
        _events: &[EventEnvelope<InternalEvent>],
    ) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// Durably enqueue a `PullInDeadline` for `target_data_hash`, then emit its fact.
/// The row is committed before the event; duplicates coalesce on data_hash. Lifetime is
/// bounded by `expires_at_ns` alone (pass `NEVER_EXPIRES` for critical nudges).
/// Callers must commit the target row FIRST: a pull-in never waits for its target —
/// a miss is dropped (debug-logged), since a missing target is normally a completed
/// one-shot and retrying would spin forever on never-expiring nudges.
pub(crate) fn enqueue_pull_in<Context: CoreContext>(
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
                PullInDeadline {
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

/// Run a `PullInDeadline` task: lower its target's deadline to the ceiling.
pub(crate) fn apply_pull_in<Context: CoreContext>(
    context: &Context,
    task: &DbTask,
    pull_in: PullInDeadline,
) -> Result<(), StorageError> {
    // Runs on the worker thread — the sole rescheduler of existing
    // rows' `next_attempt_at_ns` — so no transaction is needed
    // (inserts happen off-thread; only deadline mutation is guarded).
    let matched = match TaskDataHash::try_from(pull_in.target_data_hash.as_slice()) {
        Ok(h) => {
            // A target in failure backoff keeps its stored schedule,
            // which holds any rate-limit delay, across restarts too.
            // implements: RATE-041
            let backing_off = context
                .db()
                .get_tasks()?
                .iter()
                .any(|target| target.data_hash == h.as_ref() && target.attempts > 0);
            backing_off
                || context
                    .db()
                    .pull_in_task_deadline(&h, pull_in.not_later_than_ns)?
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
            target_data_hash = hex::encode(&pull_in.target_data_hash),
            "pull-in target missing; dropping nudge for task {}",
            task.id
        );
    }
    Ok(())
}

/// What wakes the runner: its own `TaskScheduled`, and the changes the
/// notification task follows.
fn event_interest() -> (EventFilter<InternalEvent>, Option<usize>) {
    let filter = EventFilter::new([
        EventKind::ConsentChanged,
        EventKind::ConversationJoined,
        EventKind::ConversationRemoved,
        EventKind::HmacKeysUpdated,
        // implements: PUSH-260
        // An import writes consent without consent.changed.
        EventKind::ArchiveRestored,
    ])
    .with_internal(|event| {
        matches!(
            event,
            InternalEvent::TaskScheduled
                | InternalEvent::NotificationSettingsChanged
                | InternalEvent::GroupJoined { .. }
                | InternalEvent::PreferencesChanged { .. }
        )
    });
    (filter, Some(EVENT_BUFFER))
}

pub struct Factory<Context, Handlers> {
    context: Context,
    handlers: PhantomData<fn() -> Handlers>,
}

impl<Context, Handlers> WorkerFactory for Factory<Context, Handlers>
where
    Context: CoreContext + 'static,
    Handlers: TaskHandlers<Context> + Default + 'static,
{
    fn kind(&self) -> WorkerKind {
        WorkerKind::TaskRunner
    }

    fn event_interest(&self) -> Option<(EventFilter<InternalEvent>, Option<usize>)> {
        Some(event_interest())
    }

    fn create(&self) -> BoxedWorker {
        Box::new(TaskWorker::new(self.context.clone(), Handlers::default()))
    }
}

pub struct TaskWorker<Context, Handlers> {
    context: Context,
    handlers: Handlers,
    subscription: Option<Arc<Subscription<InternalEvent>>>,
}

impl<Context, Handlers> NewWorker<Context> for TaskWorker<Context, Handlers>
where
    Context: CoreContext + 'static,
    Handlers: TaskHandlers<Context> + Default + 'static,
{
    fn factory(context: Context) -> impl WorkerFactory + 'static {
        Factory::<Context, Handlers> {
            context,
            handlers: PhantomData,
        }
    }
}

#[xmtp_common::async_trait]
impl<Context, Handlers> Worker for TaskWorker<Context, Handlers>
where
    Context: CoreContext + 'static,
    Handlers: TaskHandlers<Context> + 'static,
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
}

impl<Context, Handlers> TaskWorker<Context, Handlers>
where
    Context: CoreContext,
    Handlers: TaskHandlers<Context>,
{
    pub fn new(context: Context, handlers: Handlers) -> Self {
        Self {
            context,
            handlers,
            subscription: None,
        }
    }

    #[cfg(any(test, feature = "test-hooks"))]
    fn record_task_fact(&self, event: &xmtp_events::EventEnvelope<InternalEvent>) {
        if !matches!(&event.internal, Some(InternalEvent::TaskScheduled)) {
            return;
        }
        let witness =
            test_hooks::take_task_scheduled_witness(&self.context.installation_id().to_vec());
        if let Some(received) = witness {
            let _ = received.send(());
        }
    }

    pub async fn run(&mut self) -> Result<(), Handlers::Error> {
        if self.subscription.is_none() {
            let (filter, depth) = event_interest();
            self.subscription = Some(Arc::new(self.context.events().subscribe(filter, depth)));
        }
        let subscription = self
            .subscription
            .as_ref()
            .expect("subscription is initialized")
            .clone();
        self.handlers.on_start(&self.context)?;
        loop {
            let drained = subscription.drain();
            #[cfg(any(test, feature = "test-hooks"))]
            for event in &drained {
                self.record_task_fact(event);
            }
            self.forward_events(&drained)?;
            let next_task = self.context.db().get_next_task()?;
            let next_wakeup = Self::next_wakeup(
                next_task.as_ref().map(|t| t.next_attempt_at_ns),
                xmtp_common::time::now_ns(),
            );
            tokio::select! {
                event = subscription.next() => {
                    let Some(event) = event else { break Ok(()); };
                    #[cfg(any(test, feature = "test-hooks"))]
                    self.record_task_fact(&event);
                    let mut events = vec![event];
                    events.extend(subscription.drain());
                    self.forward_events(&events)?;
                }
                () = xmtp_common::time::sleep(next_wakeup) => {
                    if let Some(task) = next_task {
                        self.run_and_reschedule_task(task).await?;
                    }
                }
            }
        }
    }

    /// Hand a batch of events to the handlers, unless it holds only the
    /// runner's own `TaskScheduled` facts.
    fn forward_events(
        &self,
        events: &[EventEnvelope<InternalEvent>],
    ) -> Result<(), Handlers::Error> {
        if events
            .iter()
            .any(|event| !matches!(event.internal, Some(InternalEvent::TaskScheduled)))
        {
            self.handlers.on_events(&self.context, events)?;
        }
        Ok(())
    }

    #[tracing::instrument(skip_all, fields(worker = "TaskRunner", operation = "worker_turn"))]
    pub(crate) async fn run_and_reschedule_task(
        &self,
        task: DbTask,
    ) -> Result<(), Handlers::Error> {
        let context = &self.context;
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
        match self.handlers.dispatch(context, &task).await {
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
                // Every kind backs off from when its run finished, so a long
                // run does not use up its own backoff.
                let finished = xmtp_common::time::now_ns();
                let mut attempts = task.attempts + 1;
                let attempt_scaling_factor = (task.backoff_scaling_factor as f64).powi(attempts);
                let next_attempt_duration = (((task.initial_backoff_duration_ns as f64)
                    * attempt_scaling_factor) as i64)
                    .min(task.max_backoff_duration_ns);
                let backoff_at_ns = finished.saturating_add(next_attempt_duration);
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
                    let until = finished
                        .saturating_add(i64::try_from(delay.as_nanos()).unwrap_or(i64::MAX));
                    next_attempt_at_ns = next_attempt_at_ns.max(until);
                    expires_at_ns = expires_at_ns
                        .saturating_add(next_attempt_at_ns.saturating_sub(backoff_at_ns));
                }
                let next_attempt_duration = next_attempt_at_ns.saturating_sub(finished);
                tracing::warn!(%error, "Task {} retry failed. Retrying in {next_attempt_duration}ns", task.id);
                #[cfg(any(test, feature = "test-hooks"))]
                let fail_reschedule = test_hooks::take_reschedule_failure();
                #[cfg(not(any(test, feature = "test-hooks")))]
                let fail_reschedule = false;
                let stored = if fail_reschedule {
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
}

#[cfg(test)]
mod tests {
    use super::test_support::{seed, unique_proto};
    use super::*;
    use crate::core::test_context::real_context;
    use crate::identity::IdentityError;
    use prost::Message;
    use xmtp_db::tasks::data_hash_for;
    use xmtp_proto::xmtp::mls::database::{Task as TaskProto, task::Task as TaskKind};

    /// Handlers that apply each task's pull-in, as the client's dispatch
    /// does, and finish the task.
    struct PullInHandler;

    #[xmtp_common::async_trait]
    impl<Context: CoreContext> TaskHandlers<Context> for PullInHandler {
        type Error = IdentityError;

        async fn dispatch(
            &self,
            context: &Context,
            task: &DbTask,
        ) -> Result<TaskOutcome, IdentityError> {
            let task_proto = TaskProto::decode(task.data.as_slice()).expect("a stored task");
            let Some(TaskKind::PullInDeadline(pull_in)) = task_proto.task else {
                panic!("only pull-in tasks are seeded");
            };
            apply_pull_in(context, task, pull_in)?;
            Ok(TaskOutcome::Done)
        }
    }

    /// Handlers that reschedule the task with `data_hash` to `target`.
    struct RescheduleAtHandler {
        data_hash: Vec<u8>,
        target: i64,
    }

    #[xmtp_common::async_trait]
    impl<Context: CoreContext> TaskHandlers<Context> for RescheduleAtHandler {
        type Error = IdentityError;

        async fn dispatch(
            &self,
            _context: &Context,
            task: &DbTask,
        ) -> Result<TaskOutcome, IdentityError> {
            assert_eq!(task.data_hash, self.data_hash, "only one task is seeded");
            Ok(TaskOutcome::RescheduleAt(self.target))
        }
    }

    /// The failure a [`FailAfter`] run ends with: not retryable, no
    /// rate-limit delay and no reconnect, as an invalid task hash is.
    #[derive(Debug, thiserror::Error)]
    enum TaskFixtureError {
        #[error(transparent)]
        Storage(#[from] StorageError),
        #[error("invalid hash for {id}, expected: {expected}, got: {got}")]
        InvalidHash {
            id: i64,
            expected: String,
            got: String,
        },
    }

    impl RetryableError for TaskFixtureError {
        fn is_retryable(&self) -> bool {
            false
        }
    }

    impl NeedsDbReconnect for TaskFixtureError {
        fn needs_db_reconnect(&self) -> bool {
            false
        }
    }

    /// Handlers whose every run works for the given time, then fails.
    struct FailAfter(xmtp_common::time::Duration);

    #[xmtp_common::async_trait]
    impl<Context: CoreContext> TaskHandlers<Context> for FailAfter {
        type Error = TaskFixtureError;

        async fn dispatch(
            &self,
            _context: &Context,
            task: &DbTask,
        ) -> Result<TaskOutcome, TaskFixtureError> {
            xmtp_common::time::sleep(self.0).await;
            Err(TaskFixtureError::InvalidHash {
                id: task.id.into(),
                expected: String::new(),
                got: String::new(),
            })
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn done_deletes() {
        let context = real_context().await;
        let db = context.db();
        let now = xmtp_common::time::now_ns();
        let row = seed(&db, unique_proto(), now - 1, i64::MAX, 0, i32::MAX);
        TaskWorker::new(context.clone(), PullInHandler)
            .run_and_reschedule_task(row)
            .await?;
        assert!(db.get_tasks()?.is_empty(), "Done task must be deleted");
    }

    /// A failed task backs off from when its run finished, so a long run
    /// does not use up its own backoff.
    #[xmtp_common::test(unwrap_try = true)]
    async fn failed_task_backs_off_from_run_finish() {
        use xmtp_common::time::Duration;
        let context = real_context().await;
        let db = context.db();
        let proto = unique_proto();
        let hash = data_hash_for(&proto);
        let row = seed(
            &db,
            proto,
            xmtp_common::time::now_ns() - 1,
            i64::MAX,
            0,
            i32::MAX,
        );
        let backoff = (row.initial_backoff_duration_ns as f64
            * (row.backoff_scaling_factor as f64).powi(1)) as i64;
        let work = Duration::from_secs(2);
        let started = xmtp_common::time::now_ns();
        TaskWorker::new(context.clone(), FailAfter(work))
            .run_and_reschedule_task(row)
            .await?;
        let failed = db
            .get_tasks()?
            .into_iter()
            .find(|t| t.data_hash == hash.as_ref())
            .expect("the failed task is kept");
        assert_eq!(failed.attempts, 1);
        assert!(failed.next_attempt_at_ns >= started + work.as_nanos() as i64 + backoff);
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn recurring_task_advances_and_does_not_hot_loop() {
        let context = real_context().await;
        let db = context.db();
        let now = xmtp_common::time::now_ns();
        let proto = unique_proto();
        // High attempts + past-due: only the i64::MAX/i32::MAX seed keeps the
        // reaper (expires < now || attempts >= max) from deleting it.
        let row = seed(&db, proto.clone(), now - 1, i64::MAX, 1_000_000, i32::MAX);
        let target = now + xmtp_common::NS_IN_DAY;
        let handler = RescheduleAtHandler {
            data_hash: data_hash_for(&proto).to_vec(),
            target,
        };

        TaskWorker::new(context.clone(), handler)
            .run_and_reschedule_task(row)
            .await?;

        let after = db.get_tasks()?.pop().expect("recurring row must survive");
        assert_eq!(
            after.next_attempt_at_ns, target,
            "deadline must ADVANCE, not stay past-due"
        );
        assert_eq!(after.attempts, 0, "success resets the backoff counter");
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn not_yet_due_task_is_not_run_early() {
        let context = real_context().await;
        let db = context.db();
        let now = xmtp_common::time::now_ns();
        let proto = unique_proto();
        let far = now + 30 * xmtp_common::NS_IN_DAY;
        let row = seed(&db, proto.clone(), far, i64::MAX, 0, i32::MAX);
        // If the guard failed and the task ran, the handler would rewrite next_attempt.
        let handler = RescheduleAtHandler {
            data_hash: data_hash_for(&proto).to_vec(),
            target: now,
        };

        TaskWorker::new(context.clone(), handler)
            .run_and_reschedule_task(row)
            .await?;

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
}
