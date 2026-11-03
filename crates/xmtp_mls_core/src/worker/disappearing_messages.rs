use crate::context::CoreContext;
use crate::events::InternalEvent;
use crate::worker::{
    BoxedWorker, EVENT_BUFFER, NeedsDbReconnect, NewWorker, Worker, WorkerFactory,
};
use crate::worker::{WorkerKind, WorkerResult};
use futures::TryFutureExt;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use xmtp_common::time::now_ns;
use xmtp_db::{
    StorageError, XmtpMlsStorageProvider, group_message::EXPIRED_MESSAGE_BATCH, prelude::*,
};
use xmtp_events::{EventFilter, EventKind, Subscription};

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
    #[deny(clippy::wildcard_enum_match_arm)]
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
    Context: CoreContext + 'static,
{
    fn create(&self) -> BoxedWorker {
        Box::new(DisappearingMessagesWorker::new(self.context.clone()))
    }

    fn kind(&self) -> WorkerKind {
        WorkerKind::DisappearingMessages
    }

    fn event_interest(&self) -> Option<(EventFilter<InternalEvent>, Option<usize>)> {
        let filter =
            EventFilter::new([EventKind::ConversationMetadataChanged]).with_internal(|event| {
                matches!(
                    event,
                    InternalEvent::MessageStored {
                        expires_at_ns: Some(_),
                        ..
                    }
                )
            });
        Some((filter, Some(EVENT_BUFFER)))
    }
}

#[xmtp_common::async_trait]
impl<Context> Worker for DisappearingMessagesWorker<Context>
where
    Context: CoreContext + 'static,
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
}

impl<Context> NewWorker<Context> for DisappearingMessagesWorker<Context>
where
    Context: CoreContext + 'static,
{
    fn factory(context: Context) -> impl WorkerFactory + 'static {
        Factory { context }
    }
}

impl<Context> DisappearingMessagesWorker<Context>
where
    Context: CoreContext + 'static,
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
    Context: CoreContext + 'static,
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
        // implements: META-063
        let pass_start = now_ns();
        let mut total = 0;
        loop {
            let deleted = crate::state_tx::state_write_with_events(
                self.context.mls_storage(),
                self.context.events(),
                |tx, events| {
                    let storage = tx.storage();
                    let db = storage.db();
                    let deleted = db.delete_expired_messages(pass_start)?;
                    let n = deleted.len();
                    crate::events::emit_expired_messages(events, deleted, &db)?;
                    Ok::<_, StorageError>(xmtp_db::TransactionOutcome::Continue(n))
                },
            )
            .map_err(DisappearingMessagesCleanerError::DeleteExpired)?
            .into_continued();
            total += deleted;
            if deleted < EXPIRED_MESSAGE_BATCH as usize {
                break;
            }
            // Let waiting writers in between batches.
            xmtp_common::task::yield_now().await;
        }

        if total > 0 {
            tracing::info!("Successfully deleted {} expired messages", total);
        }

        Ok(())
    }
}

#[cfg(any(test, feature = "test-utils"))]
#[doc(hidden)]
pub mod test_support {
    use super::{DisappearingMessagesCleanerError, DisappearingMessagesWorker};
    use crate::context::CoreContext;

    /// Run one cleanup pass of `worker`, as its schedule runs it.
    pub async fn delete_expired_messages<Context>(
        worker: &mut DisappearingMessagesWorker<Context>,
    ) -> Result<(), DisappearingMessagesCleanerError>
    where
        Context: CoreContext + 'static,
    {
        worker.delete_expired_messages().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_context::{real_context, stored_msg};

    use std::collections::HashSet;
    use xmtp_db::group_message::StoredGroupMessage;
    use xmtp_events::{ClientEvent, EventBus, EventFilter, EventKind, EventWriter};

    use xmtp_proto::types::Cursor;
    use xmtp_proto::types::GroupId;

    // verifies: EVENT-001, META-063
    #[xmtp_common::test(unwrap_try = true)]
    async fn expired_message_worker_emits_once_after_deletion() {
        let context = real_context().await;
        let group_id = GroupId::from([9; 16]);
        let events = context
            .events()
            .subscribe_app(EventFilter::new([EventKind::MessageExpired]))?;
        let message_id = store_expiring_message(&context, group_id)?;
        wait_until_expired(&context, &message_id).await;
        let db = context.db();
        // A cohort larger than one batch: one pass must still delete all of it.
        let template = db.get_group_message(&message_id)?.unwrap();
        let mut expired_ids = HashSet::from([message_id]);
        for _ in 0..EXPIRED_MESSAGE_BATCH {
            let extra = StoredGroupMessage {
                id: xmtp_common::rand_vec::<32>(),
                ..template.clone()
            };
            extra.store(&db)?;
            expired_ids.insert(extra.id);
        }

        let mut worker = DisappearingMessagesWorker::new(context.clone());
        worker.delete_expired_messages().await?;
        for id in &expired_ids {
            assert!(db.get_group_message(id)?.is_none());
        }
        let emitted: Vec<_> = events
            .drain()
            .into_iter()
            .filter_map(|event| match event.client {
                Some(ClientEvent::MessageExpired(e)) if e.group_id == group_id.to_vec() => {
                    Some(e.message_id)
                }
                _ => None,
            })
            .collect();
        assert_eq!(emitted.len(), 1_001);
        assert_eq!(HashSet::from_iter(emitted), expired_ids);
        worker.delete_expired_messages().await?;
        assert!(events.drain().is_empty());
    }

    /// Store a group and an application message in it that expires one
    /// nanosecond after it was sent, as a group with disappearing settings
    /// of one nanosecond stores a message it sends. Return the message's ID.
    fn store_expiring_message(
        context: &impl CoreContext,
        group_id: GroupId,
    ) -> Result<Vec<u8>, StorageError> {
        use xmtp_db::group::{GroupMembershipState, StoredGroup};
        let db = context.db();
        StoredGroup::builder()
            .id(group_id)
            .created_at_ns(now_ns())
            .membership_state(GroupMembershipState::Allowed)
            .added_by_inbox_id(context.inbox_id())
            .conversation_type(xmtp_proto::types::ConversationType::Group)
            .build()?
            .store(&db)?;
        let sent_at_ns = now_ns();
        let message = StoredGroupMessage {
            decrypted_message_bytes: b"expires".to_vec(),
            sent_at_ns,
            sender_inbox_id: context.inbox_id().to_owned(),
            sender_installation_id: context.installation_id().to_vec(),
            expire_at_ns: Some(sent_at_ns + 1),
            ..stored_msg(Cursor(1), group_id)
        };
        message.store(&db)?;
        Ok(message.id)
    }

    /// The longest a test waits for a stored expiry.
    const EXPIRY_WAIT_LIMIT: Duration = Duration::from_secs(30);

    /// Wait until the stored expiry of `id` has passed. Confirmation replaces
    /// the local send time with the backend's, so the stored value can be
    /// later than the local clock. A skewed backend clock fails the test
    /// instead of hanging it.
    async fn wait_until_expired(context: &impl CoreContext, id: &[u8]) {
        let expire_at = context
            .db()
            .get_group_message(id)
            .unwrap()
            .and_then(|message| message.expire_at_ns)
            .expect("an expiring message");
        let now = now_ns();
        let wait = Duration::from_nanos(expire_at.saturating_sub(now).max(0) as u64);
        assert!(
            wait <= EXPIRY_WAIT_LIMIT,
            "stored expire_at_ns {expire_at} is {wait:?} after now_ns {now}, more than \
             {EXPIRY_WAIT_LIMIT:?}; the backend clock is probably ahead of the local clock"
        );
        while now_ns() < expire_at {
            let wait = (expire_at - now_ns()).max(0) as u64;
            xmtp_common::time::sleep(Duration::from_nanos(wait) + Duration::from_millis(1)).await;
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn stored_expiring_message_wakes_subscription_after_emit() {
        let (filter, depth) =
            DisappearingMessagesWorker::factory(crate::context::test_support::mock_context::<()>())
                .event_interest()
                .unwrap();
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
