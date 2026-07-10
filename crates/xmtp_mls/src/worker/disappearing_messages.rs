use crate::context::XmtpSharedContext;
use crate::worker::{BoxedWorker, NeedsDbReconnect, Worker, WorkerFactory};
use crate::worker::{WorkerKind, WorkerResult};
use futures::TryFutureExt;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use xmtp_common::time::now_ns;
use xmtp_db::{StorageError, prelude::*};

/// Default cap on how long the worker parks between deadline recomputes, used
/// when [`WorkerConfig`](crate::worker::WorkerConfig) supplies no override. With
/// the dedicated non-lossy re-arm channel this should never be the trigger in
/// practice; it bounds the worst case only against a never-emitted or lost
/// re-arm, and is the sleep when no disappearing messages are scheduled.
const FALLBACK_INTERVAL: Duration = Duration::from_secs(24 * 3600);

/// Dedicated wake channel for the disappearing-messages worker.
///
/// A **capacity-1** mpsc carrying unit `()` "recompute the next expiry deadline"
/// nudges. Capacity 1 is deliberate: the worker drains and re-queries the DB on
/// every wake, so a single pending nudge already captures "something changed" —
/// more would be redundant. It also bounds memory to one slot even when no worker
/// is consuming the channel (e.g. the disappearing worker is disabled or
/// `disable_workers` is set), so `rearm()` can never accumulate without bound.
#[derive(Clone)]
pub struct DisappearingChannels {
    sender: tokio::sync::mpsc::Sender<()>,
    pub receiver: Arc<tokio::sync::Mutex<tokio::sync::mpsc::Receiver<()>>>,
}

impl Default for DisappearingChannels {
    fn default() -> Self {
        Self::new()
    }
}

impl DisappearingChannels {
    pub fn new() -> Self {
        let (sender, receiver) = tokio::sync::mpsc::channel(1);
        Self {
            sender,
            receiver: Arc::new(tokio::sync::Mutex::new(receiver)),
        }
    }

    /// Wake the worker to recompute its next-expiry deadline. Best-effort and
    /// non-blocking: if a nudge is already queued (slot full) or no worker is
    /// consuming, the send is dropped — the worker recomputes from the DB on its
    /// next wake regardless, so a dropped duplicate nudge changes nothing.
    pub fn rearm(&self) {
        let _ = self.sender.try_send(());
    }
}

#[derive(Debug, Error)]
pub enum DisappearingMessagesCleanerError {
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("failed to delete expired messages: {0}")]
    DeleteExpired(StorageError),
}

impl NeedsDbReconnect for DisappearingMessagesCleanerError {
    fn needs_db_reconnect(&self) -> bool {
        match self {
            Self::Storage(s) | Self::DeleteExpired(s) => s.db_needs_connection(),
        }
    }
}

pub struct DisappearingMessagesWorker<Context> {
    context: Context,
}

struct Factory<Context> {
    context: Context,
}

impl<Context> WorkerFactory for Factory<Context>
where
    Context: XmtpSharedContext + 'static,
{
    fn create(
        &self,
        metrics: Option<crate::worker::DynMetrics>,
    ) -> (BoxedWorker, Option<crate::worker::DynMetrics>) {
        let worker = Box::new(DisappearingMessagesWorker::new(self.context.clone())) as Box<_>;
        (worker, metrics)
    }

    fn kind(&self) -> WorkerKind {
        WorkerKind::DisappearingMessages
    }
}

#[xmtp_common::async_trait]
impl<Context> Worker for DisappearingMessagesWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    fn kind(&self) -> WorkerKind {
        WorkerKind::DisappearingMessages
    }

    async fn run_tasks(&mut self) -> WorkerResult<()> {
        self.run().map_err(|e| Box::new(e) as Box<_>).await
    }

    fn factory<C>(context: C) -> impl WorkerFactory + 'static
    where
        Self: Sized,
        C: XmtpSharedContext + 'static,
    {
        Factory { context }
    }
}

impl<Context> DisappearingMessagesWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    pub fn new(context: Context) -> Self {
        Self { context }
    }
}

impl<Context> DisappearingMessagesWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    /// Event-driven loop: sleep until the soonest message expiry, deleting the
    /// batch when the deadline arrives. A re-arm signal (sent post-commit when a
    /// disappearing message is stored) wakes the loop early to recompute the
    /// deadline. With no disappearing messages scheduled, parks for `FALLBACK_MAX`.
    async fn run(&mut self) -> Result<(), DisappearingMessagesCleanerError> {
        // Resolve the fallback cap (and optional jitter) from WorkerConfig, the
        // same knobs the other workers honor.
        let (fallback, jitter) = self
            .context
            .worker_interval(WorkerKind::DisappearingMessages, FALLBACK_INTERVAL);
        let receiver = self.context.disappearing_channels().receiver.clone();
        let mut receiver = receiver.lock().await;
        loop {
            // Coalesce any pending re-arm signals so we recompute the deadline once.
            while receiver.try_recv().is_ok() {}

            let next = self
                .context
                .db()
                .min_expire_at_ns()
                .map_err(|e| DisappearingMessagesCleanerError::Storage(e.into()))?;
            // A real expiry drives a precise deadline (no jitter — we don't want
            // to delay actual deletions). Only the idle/fallback wake is jittered,
            // to de-synchronize a fleet of clients booted together.
            let dur = match next {
                Some(expire_at) => {
                    Duration::from_nanos((expire_at - now_ns()).max(0) as u64).min(fallback)
                }
                None => fallback.saturating_add(xmtp_common::time::rand_offset(jitter)),
            };

            tokio::select! {
                // A disappearing message was stored; loop to recompute the deadline.
                _ = receiver.recv() => {}
                // Deadline reached (or fallback); delete whatever is now expired.
                () = xmtp_common::time::sleep(dur) => {
                    self.delete_expired_messages().await?;
                }
            }
        }
    }

    /// Iterate on the list of groups and delete expired messages
    #[tracing::instrument(skip_all, fields(worker = ?self.kind(), operation = "worker_turn"))]
    async fn delete_expired_messages(&mut self) -> Result<(), DisappearingMessagesCleanerError> {
        let db = self.context.db();
        // Propagated to the supervisor, which is the sole logger for worker errors.
        let deleted_messages = db
            .delete_expired_messages()
            .map_err(|e| DisappearingMessagesCleanerError::DeleteExpired(e.into()))?;

        if !deleted_messages.is_empty() {
            tracing::info!(
                "Successfully deleted {} expired messages",
                deleted_messages.len()
            );

            // Emit a single event for all deleted messages
            // this avoids a hot loop that may starve async tasks.
            let _ =
                self.context
                    .local_events()
                    .send(crate::subscriptions::LocalEvents::MsgsDeleted(
                        deleted_messages,
                    ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[xmtp_common::test(unwrap_try = true)]
    async fn rearm_delivers_a_signal() {
        let ch = DisappearingChannels::new();
        ch.rearm();
        let mut rx = ch.receiver.lock().await;
        assert!(rx.recv().await.is_some());
    }
}
