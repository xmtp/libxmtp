use crate::context::XmtpSharedContext;
use crate::subscriptions::internal::InternalEvent;
use crate::worker::{BoxedWorker, NeedsDbReconnect, Worker, WorkerFactory};
use crate::worker::{WorkerKind, WorkerResult};
use futures::TryFutureExt;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use xmtp_common::time::now_ns;
use xmtp_db::{StorageError, XmtpMlsStorageProvider, prelude::*};
use xmtp_events::Subscription;

/// Default cap on how long the worker parks between deadline recomputes, used
/// when [`WorkerConfig`](crate::worker::WorkerConfig) supplies no override. With
/// the event subscription this is a fallback when no message is scheduled.
const FALLBACK_INTERVAL: Duration = Duration::from_secs(24 * 3600);

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
    subscription: Option<Arc<Subscription<InternalEvent>>>,
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

    fn set_subscription(&mut self, subscription: Arc<Subscription<InternalEvent>>) {
        self.subscription = Some(subscription);
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
        Self {
            context,
            subscription: None,
        }
    }
}

impl<Context> DisappearingMessagesWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    /// Event-driven loop: sleep until the soonest message expiry, deleting the
    /// batch when the deadline arrives. A stored-message fact makes it read the
    /// next deadline again.
    async fn run(&mut self) -> Result<(), DisappearingMessagesCleanerError> {
        // Resolve the fallback cap (and optional jitter) from WorkerConfig, the
        // same knobs the other workers honor.
        let (fallback, jitter) = self
            .context
            .worker_interval(WorkerKind::DisappearingMessages, FALLBACK_INTERVAL);
        let subscription = self
            .subscription
            .as_ref()
            .expect("runner installs subscription")
            .clone();
        loop {
            subscription.drain();

            let next = self
                .context
                .mls_storage()
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
                event = subscription.next() => {
                    if event.is_none() { break Ok(()); }
                }
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
        let deleted_messages = crate::state_tx::state_write_with_events(
            self.context.mls_storage(),
            self.context.events(),
            |tx, events| {
                let storage = tx.storage();
                let db = storage.db();
                let deleted = db.delete_expired_messages()?;
                crate::subscriptions::internal::emit_expired_messages(
                    events,
                    deleted.clone(),
                    &db,
                )?;
                Ok::<_, StorageError>(xmtp_db::TransactionOutcome::Continue(deleted))
            },
        )
        .map_err(DisappearingMessagesCleanerError::DeleteExpired)?
        .into_continued();

        if !deleted_messages.is_empty() {
            tracing::info!(
                "Successfully deleted {} expired messages",
                deleted_messages.len()
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xmtp_events::{EventBus, EventWriter};

    #[xmtp_common::test(unwrap_try = true)]
    async fn stored_expiring_message_wakes_subscription_after_emit() {
        let (filter, depth) =
            crate::worker::worker_event_filter(WorkerKind::DisappearingMessages).unwrap();
        let bus = EventBus::new();
        let subscription = bus.subscribe(filter, depth);
        let group_id = xmtp_proto::types::GroupId::from([7; 16]);
        bus.emit(
            None,
            Some(InternalEvent::MessageStored {
                group_id,
                message_id: vec![1],
                expires_at_ns: Some(now_ns() + 1_000_000),
                is_sync: false,
            }),
        );
        let event =
            xmtp_common::time::timeout(Duration::from_millis(100), subscription.next()).await?;
        assert!(matches!(
            event.unwrap().internal,
            Some(InternalEvent::MessageStored {
                expires_at_ns: Some(_),
                ..
            })
        ));
    }
}
