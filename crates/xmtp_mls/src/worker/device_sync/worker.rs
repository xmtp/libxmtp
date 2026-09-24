use super::{
    DeviceSyncClient, DeviceSyncError, decode_supported_content,
    preference_sync::{PreferenceUpdate, store_preference_updates},
};
use crate::{
    context::XmtpSharedContext,
    subscriptions::{
        incoming::{IncomingCoordinator, IncomingScope},
        internal::{GroupOrigin, InternalEvent, PreferenceOrigin},
    },
    worker::{
        BoxedWorker, DynMetrics, MetricsCasting, NeedsDbReconnect, Worker, WorkerFactory,
        WorkerKind, WorkerResult, metrics::WorkerMetrics,
    },
};
use futures::TryFutureExt;
use parking_lot::Mutex;
use prost::Message;
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::sync::OnceCell;
use tracing::instrument;
use xmtp_common::Event;
use xmtp_db::group_message::StoredGroupMessage;
use xmtp_db::prelude::*;
use xmtp_events::{ClientEvent, Subscription};
use xmtp_macro::log_event;
use xmtp_proto::xmtp::{
    device_sync::content::{
        DeviceSyncAcknowledge, PreferenceUpdates as PreferenceUpdatesProto,
        device_sync_content::Content as ContentProto,
    },
    mls::message_contents::EncodedContent,
};

const MAX_ATTEMPTS: i32 = 3;
type PendingEvent = Arc<Mutex<Option<(u64, xmtp_events::EventEnvelope<InternalEvent>)>>>;

#[cfg(test)]
pub(crate) mod test_hooks {
    use std::sync::Arc;
    use tokio::sync::Notify;

    type BlockHook = (Vec<u8>, Arc<Notify>, Arc<Notify>);
    pub(crate) static FAIL_NEXT_PREFERENCE_PUBLISH: parking_lot::Mutex<
        Option<(Vec<u8>, Arc<Notify>)>,
    > = parking_lot::Mutex::new(None);
    pub(crate) static BLOCK_NEXT_PREFERENCE_PUBLISH: parking_lot::Mutex<Option<BlockHook>> =
        parking_lot::Mutex::new(None);
}

pub struct SyncWorker<Context> {
    client: DeviceSyncClient<Context>,
    subscription: Option<Arc<Subscription<InternalEvent>>>,
    pending: PendingEvent,
    next_event_id: Arc<AtomicU64>,
    init: OnceCell<()>,
    metrics: Arc<WorkerMetrics<SyncMetric>>,
}

impl<Context> SyncWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    pub fn new(
        context: Context,
        metrics: Option<DynMetrics>,
        pending: PendingEvent,
        next_event_id: Arc<AtomicU64>,
    ) -> Self {
        let metrics = metrics
            .and_then(|m| m.as_sync_metrics())
            .unwrap_or(Arc::new(WorkerMetrics::new(context.installation_id())));
        let client = DeviceSyncClient::new(context, metrics.clone());

        Self {
            client,
            subscription: None,
            pending,
            next_event_id,
            init: OnceCell::new(),
            metrics,
        }
    }
}

struct Factory<Context> {
    context: Context,
    pending: PendingEvent,
    next_event_id: Arc<AtomicU64>,
}

impl<Context> WorkerFactory for Factory<Context>
where
    Context: XmtpSharedContext + 'static,
{
    fn create(&self, metrics: Option<DynMetrics>) -> (BoxedWorker, Option<DynMetrics>) {
        let worker = SyncWorker::new(
            self.context.clone(),
            metrics,
            self.pending.clone(),
            self.next_event_id.clone(),
        );
        let metrics = worker.metrics.clone();

        (Box::new(worker) as Box<_>, Some(metrics as Arc<_>))
    }

    fn kind(&self) -> WorkerKind {
        WorkerKind::DeviceSync
    }
}

#[xmtp_common::async_trait]
impl<Context> Worker for SyncWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    fn kind(&self) -> WorkerKind {
        WorkerKind::DeviceSync
    }

    fn set_subscription(&mut self, subscription: Arc<Subscription<InternalEvent>>) {
        self.subscription = Some(subscription);
    }

    fn metrics(&self) -> Option<DynMetrics> {
        Some(self.metrics.clone())
    }

    fn factory<C>(context: C) -> impl WorkerFactory + 'static
    where
        C: XmtpSharedContext + 'static,
    {
        Factory {
            context,
            pending: Arc::default(),
            next_event_id: Arc::new(AtomicU64::new(1)),
        }
    }

    async fn run_tasks(&mut self) -> WorkerResult<()> {
        self.run().map_err(|e| Box::new(e) as Box<_>).await
    }
}

impl<Context> SyncWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    async fn run(&mut self) -> Result<(), DeviceSyncError> {
        self.sync_init().await?;
        // Receipt must outlive each sync call so remote updates can wake this worker.
        let _receipt = IncomingCoordinator::for_context(&self.client.context)
            .acquire(IncomingScope::DeviceSyncGroups);
        self.metrics.increment_metric(SyncMetric::Init);

        self.run_internal().await
    }

    async fn run_internal(&mut self) -> Result<(), DeviceSyncError> {
        use futures::StreamExt;
        let (base, jitter) = self
            .client
            .context
            .worker_interval(WorkerKind::DeviceSync, Duration::from_secs(20));
        let mut intervals = xmtp_common::time::jittered_interval_stream(base, jitter);
        let _ = intervals.next().await;
        let subscription = self
            .subscription
            .as_ref()
            .expect("runner installs subscription")
            .clone();
        loop {
            let pending_event = { self.pending.lock().clone() };
            if let Some((id, event)) = pending_event {
                self.handle_pending_event(id, event).await?;
                continue;
            }
            tokio::select! {
                event = subscription.next() => {
                    let Some(event) = event else { break; };
                    let id = self.next_event_id.fetch_add(1, Ordering::Relaxed);
                    *self.pending.lock() = Some((id, event.clone()));
                    self.handle_pending_event(id, event).await?;
                }
                _ = intervals.next() => self.evt_new_sync_group_msg(true).await?,
            }
        }
        Ok(())
    }

    async fn handle_pending_event(
        &mut self,
        id: u64,
        event: xmtp_events::EventEnvelope<InternalEvent>,
    ) -> Result<(), DeviceSyncError> {
        self.handle_event(event.clone()).await?;
        let mut pending = self.pending.lock();
        if pending
            .as_ref()
            .is_some_and(|(pending_id, _)| *pending_id == id)
        {
            *pending = None;
        }
        Ok(())
    }

    #[tracing::instrument(skip_all, fields(worker = ?self.kind(), operation = "worker_turn", event = ?event))]
    async fn handle_event(
        &mut self,
        event: xmtp_events::EventEnvelope<InternalEvent>,
    ) -> Result<(), DeviceSyncError> {
        if matches!(
            event.client,
            Some(ClientEvent::IdentityOwnInstallationRevoked(_))
        ) {
            self.evt_cycle_hmac().await?;
        }
        match event.internal {
            Some(InternalEvent::GroupJoined {
                is_sync: true,
                origin: GroupOrigin::Welcomed,
                ..
            }) => self.evt_new_sync_group_from_welcome().await,
            Some(
                InternalEvent::MessageStored { is_sync: true, .. }
                | InternalEvent::SyncMessagePublished,
            ) => self.evt_new_sync_group_msg(false).await,
            Some(InternalEvent::PreferencesChanged {
                updates,
                origin: PreferenceOrigin::Local,
            }) => self.evt_sync_preferences(updates).await,
            _ => Ok(()),
        }
    }

    /// Initialize the sync group when the client is registered.
    #[instrument(level = "trace", skip_all)]
    async fn sync_init(&mut self) -> Result<(), DeviceSyncError> {
        let Self { init, client, .. } = &self;

        init.get_or_try_init(|| async {
            let conn = self.client.context.db();
            log_event!(
                Event::DeviceSyncInitializing,
                self.client.context.installation_id()
            );

            // The only thing that sync init really does right now is ensures that there's a sync group.
            if conn.primary_sync_group()?.is_none() {
                log_event!(
                    Event::DeviceSyncNoPrimarySyncGroup,
                    self.client.context.installation_id()
                );
                let sync_group = client.get_sync_group().await?;
                log_event!(
                    Event::DeviceSyncCreatedPrimarySyncGroup,
                    self.client.context.installation_id(),
                    group_id = sync_group.group_id
                );
            }

            log_event!(
                Event::DeviceSyncInitializingFinished,
                self.client.context.installation_id()
            );

            Ok(())
        })
        .await
        .copied()
    }

    async fn evt_new_sync_group_from_welcome(&self) -> Result<(), DeviceSyncError> {
        tracing::info!("New sync group from welcome detected.");

        // A new sync group from a welcome indicates a new installation.
        // Schedule durable per-group reconciliation on the TaskRunner —
        // a one-shot inline add here is lost forever if it fails once.
        self.client.schedule_add_installations_to_groups()?;

        self.metrics
            .increment_metric(SyncMetric::SyncGroupWelcomesProcessed);

        // Cycle the HMAC
        self.client.cycle_hmac().await?;

        Ok(())
    }

    async fn evt_new_sync_group_msg(&self, is_tick: bool) -> Result<(), DeviceSyncError> {
        let unprocessed_messages = self.client.context.db().unprocessed_sync_group_messages()?;

        if !is_tick || !unprocessed_messages.is_empty() {
            tracing::info!("Processing {} messages.", unprocessed_messages.len());
        }

        self.client
            .process_sync_group_messages(&self.metrics, unprocessed_messages)
            .await
    }

    async fn evt_sync_preferences(
        &self,
        updates: Vec<PreferenceUpdate>,
    ) -> Result<(), DeviceSyncError> {
        #[cfg(test)]
        {
            let blocked = {
                let mut hook = test_hooks::BLOCK_NEXT_PREFERENCE_PUBLISH.lock();
                if hook.as_ref().is_some_and(|(installation, _, _)| {
                    installation.as_slice()
                        == self.client.context.installation_id().to_vec().as_slice()
                }) {
                    hook.take()
                } else {
                    None
                }
            };
            if let Some((_, entered, release)) = blocked {
                entered.notify_one();
                release.notified().await;
            }
            let failed = {
                let mut hook = test_hooks::FAIL_NEXT_PREFERENCE_PUBLISH.lock();
                if hook.as_ref().is_some_and(|(installation, _)| {
                    installation.as_slice()
                        == self.client.context.installation_id().to_vec().as_slice()
                }) {
                    hook.take()
                } else {
                    None
                }
            };
            if let Some((_, observed)) = failed {
                observed.notify_one();
                return Err(DeviceSyncError::IO(std::io::Error::other(
                    "test preference publication failure",
                )));
            }
        }
        self.client.sync_preferences(updates).await?;
        Ok(())
    }

    async fn evt_cycle_hmac(&self) -> Result<(), DeviceSyncError> {
        self.client.cycle_hmac().await?;
        Ok(())
    }
}

impl<Context> DeviceSyncClient<Context>
where
    Context: XmtpSharedContext,
{
    async fn process_sync_group_messages(
        &self,
        handle: &WorkerMetrics<SyncMetric>,
        messages: Vec<StoredGroupMessage>,
    ) -> Result<(), DeviceSyncError>
    where
        Context::Db: 'static,
    {
        let installation_id = self.installation_id();

        for msg in messages {
            let content = EncodedContent::decode(&*msg.decrypted_message_bytes)
                .ok()
                .and_then(|content| decode_supported_content(&content.content));
            let Some(content) = content else {
                // Older installations can send archive transfer content. Its
                // reserved oneof fields decode as unsupported content here.
                // Mark it complete so the worker does not retry it forever.
                self.context
                    .db()
                    .mark_device_sync_msg_as_processed(&msg.id)?;
                continue;
            };
            let is_external = msg.sender_installation_id != installation_id;

            let msg_type = match &content {
                ContentProto::PreferenceUpdates(_) => "PreferenceUpdates",
                ContentProto::Acknowledge(_) => "Acknowledge",
            };

            log_event!(
                Event::DeviceSyncProcessingMessages,
                self.context.installation_id(),
                msg_type,
                external = is_external,
                message_id = #msg.id,
                group_id = msg.group_id
            );

            if let Err(err) = self.process_message(handle, &msg, content).await {
                // A failed message is non-fatal (log + bump attempt), but a
                // dropped pool must stop the worker; bubble it.
                if err.needs_db_reconnect() {
                    return Err(err);
                }
                log_event!(
                    Event::DeviceSyncMessageProcessingError,
                    self.context.installation_id(),
                    error = %err,
                    message_id = #msg.id
                );
                self.context
                    .db()
                    .increment_device_sync_msg_attempt(&msg.id, MAX_ATTEMPTS)?;
            } else {
                self.context
                    .db()
                    .mark_device_sync_msg_as_processed(&msg.id)?;
            }
        }

        Ok(())
    }

    async fn process_message(
        &self,
        handle: &WorkerMetrics<SyncMetric>,
        msg: &StoredGroupMessage,
        content: ContentProto,
    ) -> Result<(), DeviceSyncError>
    where
        Context::Db: 'static,
    {
        let installation_id = self.context.installation_id();
        let is_external = msg.sender_installation_id != installation_id;

        match content {
            ContentProto::PreferenceUpdates(PreferenceUpdatesProto { updates }) => {
                if is_external {
                    tracing::info!("Incoming preference updates: {updates:?}");
                }
                tracing::info!(
                    "{} storing preference updates",
                    self.context.installation_id()
                );
                // We'll process even our own messages here. The sync group message ordering takes authority over our own here.
                crate::state_tx::state_write_with_events(
                    self.context.mls_storage(),
                    self.context.events(),
                    |tx, events| {
                        let storage = tx.storage();
                        let db = storage.db();
                        let updated = store_preference_updates(updates.clone(), &db, handle)?;
                        crate::subscriptions::internal::emit_preference_updates_with_public(
                            events,
                            updated.public,
                            updated.legacy.clone(),
                            PreferenceOrigin::Sync,
                            &db,
                        )?;
                        if !updated.legacy.is_empty() {
                            self.context.task_channels().mark_notification_changed();
                        }
                        Ok::<_, xmtp_db::StorageError>(xmtp_db::TransactionOutcome::Continue(
                            updated.legacy,
                        ))
                    },
                )?
                .into_continued();
            }
            ContentProto::Acknowledge(DeviceSyncAcknowledge { .. }) => {
                return Ok(());
            }
        }

        Ok(())
    }
}

#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug)]
pub enum SyncMetric {
    Init,
    SyncGroupCreated,
    SyncGroupWelcomesProcessed,
    HmacSent,
    HmacReceived,
    ConsentSent,
    ConsentReceived,
}

impl WorkerMetrics<SyncMetric> {
    pub async fn wait_for_init(&self) -> Result<(), xmtp_common::time::Expired> {
        self.register_interest(SyncMetric::Init, 1).wait().await
    }
}
