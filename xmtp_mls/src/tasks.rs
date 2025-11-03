use std::sync::Arc;

use prost::Message;
use xmtp_db::tasks::{NewTask as DbNewTask, QueryTasks, Task as DbTask};
use xmtp_proto::{
    types::{WelcomeMessage, WelcomeMessageType},
    xmtp::mls::database::Task as TaskProto,
};

use crate::{
    context::XmtpSharedContext,
    worker::{NeedsDbReconnect, Worker, WorkerFactory, WorkerKind},
};

#[derive(thiserror::Error, Debug)]
pub enum TaskWorkerError {
    #[error("generic storage error: {0}")]
    Storage(#[from] xmtp_db::StorageError),
    #[error("group error: {0}")]
    Group(#[from] crate::groups::GroupError),
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
}

impl NeedsDbReconnect for TaskWorkerError {
    fn needs_db_reconnect(&self) -> bool {
        match self {
            TaskWorkerError::Storage(s)
            | TaskWorkerError::Group(crate::groups::GroupError::Storage(s)) => {
                s.db_needs_connection()
            }
            TaskWorkerError::Group(_) => false,
            TaskWorkerError::InvalidTaskData { .. } => false,
            TaskWorkerError::InvalidHash { .. } => false,
            TaskWorkerError::ReceiverLocked => false,
        }
    }
}

#[derive(Clone)]
pub struct TaskWorkerChannels {
    // Using unbounded to avoid potential issues with the receiver queue being full
    pub task_sender: tokio::sync::mpsc::UnboundedSender<DbNewTask>,
    pub task_receiver: Arc<tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<DbNewTask>>>,
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
            .send(new_task)
            .expect("Task receiver is owned by same struct");
    }
}

impl Default for TaskWorkerChannels {
    fn default() -> Self {
        Self::new()
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

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
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
        let channels = context.workers().task_channels().clone();
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
                task = receiver.recv() => {
                    let task = task.expect("Task sender is owned by the task worker");
                    self.context.db().create_task(task)?;
                }
                () = xmtp_common::time::sleep(next_wakeup) => {
                    if let Some(task) = next_task {
                        Self::run_and_reschedule_task(task, &self.context).await?;
                    }
                }
            }
        }
    }
    async fn run_and_reschedule_task(
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
            Ok(()) => {
                context.db().delete_task(task.id)?;
            }
            Err(error) => {
                let attempts = task.attempts + 1;
                let attempt_scaling_factor = (task.backoff_scaling_factor as f64).powi(attempts);
                let next_attempt_duration = (((task.initial_backoff_duration_ns as f64)
                    * attempt_scaling_factor) as i64)
                    .min(task.max_backoff_duration_ns);
                let next_attempt_at_ns = now.saturating_add(next_attempt_duration);
                tracing::warn!(%error, "Task {} retry failed. Retrying in {next_attempt_duration}ns", task.id);
                context
                    .db()
                    .update_task(task.id, attempts, now, next_attempt_at_ns)?;
            }
        }
        Ok(())
    }
    async fn run_task(task: &DbTask, context: &Context) -> Result<(), TaskWorkerError> {
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
                return Ok(());
            }
        };
        match task_proto.task {
            Some(xmtp_proto::xmtp::mls::database::task::Task::ProcessWelcomePointer(
                welcome_pointer,
            )) => {
                Self::process_welcome_pointer(task, welcome_pointer, context).await?;
            }
            None => {
                tracing::error!("Task {} has no data. Deleting.", task.id);
                context.db().delete_task(task.id)?;
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
