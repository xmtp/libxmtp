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
    use crate::messages::decoded_message::{DecodedMessage, DeletedBy, MessageBody};
    use crate::tester;
    use xmtp_events::{ClientEvent, EventBus, EventFilter, EventKind, EventWriter};
    use xmtp_mls_common::group::GroupMetadataOptions;
    use xmtp_mls_common::group_mutable_metadata::MessageDisappearingSettings;
    use xmtp_proto::types::GroupId;

    // verifies: EVENT-001
    #[xmtp_common::test(unwrap_try = true)]
    async fn expired_message_worker_emits_once_after_deletion() {
        tester!(alix, disable_workers);
        let group = alix.create_group(
            None,
            Some(GroupMetadataOptions {
                message_disappearing_settings: Some(MessageDisappearingSettings::new(1, 1)),
                ..Default::default()
            }),
        )?;
        let events = alix
            .context
            .events()
            .subscribe_app(EventFilter::new([EventKind::MessageExpired]))?;
        let message_id = group.send_message(b"expires", Default::default()).await?;
        wait_until_expired(&alix.context, &message_id).await;
        let db = alix.context.db();

        let mut worker = DisappearingMessagesWorker::new(alix.context.clone());
        worker.delete_expired_messages().await?;
        assert!(db.get_group_message(&message_id)?.is_none());
        assert!(matches!(
            events.drain().as_slice(),
            [xmtp_events::EventEnvelope {
                client: Some(ClientEvent::MessageExpired(expired)), ..
            }] if expired.group_id == group.group_id.to_vec()
                && expired.message_id == message_id
        ));
        worker.delete_expired_messages().await?;
        assert!(events.drain().is_empty());
    }

    /// The longest a test waits for a stored expiry.
    const EXPIRY_WAIT_LIMIT: Duration = Duration::from_secs(30);

    /// Wait until the stored expiry of `id` has passed. Confirmation replaces
    /// the local send time with the backend's, so the stored value can be
    /// later than the local clock. A skewed backend clock fails the test
    /// instead of hanging it.
    async fn wait_until_expired(context: &impl XmtpSharedContext, id: &[u8]) {
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

    /// A deletion item for an expired message is the deleted-message
    /// placeholder, so it carries no body and no host decodes an empty one.
    fn assert_no_body(
        deleted: &DecodedMessage,
        group_id: &GroupId,
        message_id: &[u8],
        sender_inbox_id: &str,
        deleted_by: &DeletedBy,
    ) {
        use crate::messages::enrichment::deleted_message_content_type;

        assert_eq!(deleted.metadata.group_id, *group_id);
        assert_eq!(deleted.metadata.id, message_id);
        assert_eq!(deleted.metadata.sender_inbox_id, sender_inbox_id);
        assert_eq!(
            deleted.metadata.content_type,
            deleted_message_content_type(),
            "the deletion item has no content type"
        );
        match &deleted.content {
            MessageBody::DeletedMessage { deleted_by: actual } => {
                assert_eq!(
                    actual, deleted_by,
                    "the deletion item names the wrong actor"
                )
            }
            other => panic!("the deletion item carried a body: {other:?}"),
        }
        assert_eq!(deleted.fallback_text, None);
    }

    /// The deletion stream of `context`, not polled until the test reads it.
    fn deletion_stream(
        context: &impl XmtpSharedContext,
    ) -> impl futures::Stream<Item = crate::subscriptions::Result<DecodedMessage>> {
        use crate::subscriptions::StreamMessages;
        context
            .events()
            .subscribe(
                EventFilter::default().with_internal(InternalEvent::is_message_deletion),
                Some(8),
            )
            .stream_message_deletions()
    }

    /// Encoded text with a fallback, so a test can see whether either leaks.
    fn secret_text() -> Vec<u8> {
        use xmtp_content_types::{ContentCodec, encoded_content_to_bytes, text::TextCodec};
        let mut secret = TextCodec::encode("secret".into()).unwrap();
        secret.fallback = Some("secret fallback".into());
        encoded_content_to_bytes(secret)
    }

    /// The three ways an expired message leaves the database: expiry cleanup,
    /// a group delete, and a local client delete.
    #[derive(Clone, Copy, Debug)]
    enum Removal {
        Cleanup,
        GroupDelete,
        ClientDelete,
    }

    /// Every deletion of an expired message reports which message was
    /// deleted, not what it said.
    // verifies: META-051
    #[rstest::rstest]
    #[case::cleanup(Removal::Cleanup)]
    #[case::group_delete(Removal::GroupDelete)]
    #[case::client_delete(Removal::ClientDelete)]
    #[xmtp_common::test(unwrap_try = true)]
    async fn expired_message_deletion_event_carries_no_body(
        #[case] removal: Removal,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use futures::StreamExt;

        tester!(alix, disable_workers);
        let group = alix.create_group(
            None,
            Some(GroupMetadataOptions {
                message_disappearing_settings: Some(MessageDisappearingSettings::new(1, 1)),
                ..Default::default()
            }),
        )?;
        let deletions = deletion_stream(&alix.context);
        futures::pin_mut!(deletions);
        let message_id = group
            .send_message(&secret_text(), Default::default())
            .await?;
        wait_until_expired(&alix.context, &message_id).await;

        match removal {
            Removal::Cleanup => {
                DisappearingMessagesWorker::new(alix.context.clone())
                    .delete_expired_messages()
                    .await?;
            }
            Removal::GroupDelete => {
                group.delete_message(message_id.clone())?;
            }
            Removal::ClientDelete => {
                assert_eq!(alix.delete_message(message_id.clone())?, 1);
            }
        }
        let deleted = xmtp_common::time::timeout(Duration::from_secs(5), deletions.next())
            .await?
            .expect("a deletion item")?;
        assert_no_body(
            &deleted,
            &group.group_id,
            &message_id,
            alix.inbox_id(),
            &DeletedBy::Sender,
        );
        Ok(())
    }

    /// A deletion item can wait in the broadcast buffer until after its
    /// message expired. It must not deliver the body then.
    // verifies: META-051
    #[xmtp_common::test(unwrap_try = true)]
    async fn deletion_that_expires_before_delivery_carries_no_body() {
        use futures::StreamExt;
        /// Long enough for the delete to finish before the expiry.
        const LIFESPAN: Duration = Duration::from_secs(3);

        tester!(alix, disable_workers);
        let group = alix.create_group(
            None,
            Some(GroupMetadataOptions {
                message_disappearing_settings: Some(MessageDisappearingSettings::new(
                    1,
                    LIFESPAN.as_nanos() as i64,
                )),
                ..Default::default()
            }),
        )?;
        let deletions = deletion_stream(&alix.context);
        futures::pin_mut!(deletions);
        let message_id = group
            .send_message(&secret_text(), Default::default())
            .await?;
        let expire_at = alix
            .context
            .db()
            .get_group_message(&message_id)?
            .and_then(|message| message.expire_at_ns)
            .expect("an expiring message");

        group.delete_message(message_id.clone())?;
        let emitted_at = now_ns();
        assert!(
            emitted_at < expire_at,
            "the delete finished at {emitted_at}, after the expiry {expire_at}; the test \
             needs a message that is live at emit time"
        );
        wait_until_expired(&alix.context, &message_id).await;

        let deleted = xmtp_common::time::timeout(Duration::from_secs(5), deletions.next())
            .await?
            .expect("a deletion item")?;
        assert_no_body(
            &deleted,
            &group.group_id,
            &message_id,
            alix.inbox_id(),
            &DeletedBy::Sender,
        );
    }

    /// Where a super admin's delete of an expired message is observed.
    #[derive(Clone, Copy, Debug)]
    enum Observer {
        /// The admin's own installation, which deletes locally.
        Admin,
        /// The sender's installation, which receives the delete.
        Sender,
    }

    /// A super admin's delete of an expired message names the admin, on the
    /// deleting installation and on a receiving one.
    // verifies: META-051
    #[rstest::rstest]
    #[case::local(Observer::Admin)]
    #[case::remote(Observer::Sender)]
    #[xmtp_common::test(unwrap_try = true)]
    async fn admin_delete_of_expired_message_names_the_admin(
        #[case] observer: Observer,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use futures::StreamExt;

        tester!(alix, disable_workers);
        tester!(bo, disable_workers);
        let alix_group = alix.create_group(
            None,
            Some(GroupMetadataOptions {
                message_disappearing_settings: Some(MessageDisappearingSettings::new(1, 1)),
                ..Default::default()
            }),
        )?;
        alix_group.add_members(&[bo.inbox_id()]).await?;
        bo.sync_welcomes().await?;
        let bo_group = bo.group(&alix_group.group_id)?;
        let deletions = match observer {
            Observer::Admin => deletion_stream(&alix.context).left_stream(),
            Observer::Sender => deletion_stream(&bo.context).right_stream(),
        };
        futures::pin_mut!(deletions);

        let message_id = bo_group
            .send_message(&secret_text(), Default::default())
            .await?;
        alix_group.sync().await?;
        wait_until_expired(&alix.context, &message_id).await;
        wait_until_expired(&bo.context, &message_id).await;
        alix_group.delete_message(message_id.clone())?;
        if let Observer::Sender = observer {
            alix_group.publish_messages().await?;
            bo_group.sync().await?;
        }

        let deleted = xmtp_common::time::timeout(Duration::from_secs(5), deletions.next())
            .await?
            .expect("a deletion item")?;
        assert_no_body(
            &deleted,
            &alix_group.group_id,
            &message_id,
            bo.inbox_id(),
            &DeletedBy::Admin(alix.inbox_id().to_string()),
        );
        Ok(())
    }

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
