use super::{
    DeviceSyncClient, DeviceSyncError, decode_supported_content,
    preference_sync::{PreferenceUpdate, store_preference_updates},
};
use crate::{
    context::XmtpSharedContext,
    subscriptions::{LocalEvents, SyncWorkerEvent},
    worker::{
        BoxedWorker, DynMetrics, MetricsCasting, NeedsDbReconnect, Worker, WorkerFactory,
        WorkerKind, WorkerResult, metrics::WorkerMetrics,
    },
};
use futures::TryFutureExt;
use prost::Message;
use std::{sync::Arc, time::Duration};
use tokio::sync::{OnceCell, broadcast};
use tracing::instrument;
use xmtp_common::Event;
use xmtp_db::group_message::StoredGroupMessage;
use xmtp_db::prelude::*;
use xmtp_macro::log_event;
use xmtp_proto::xmtp::{
    device_sync::content::{
        DeviceSyncAcknowledge, PreferenceUpdates as PreferenceUpdatesProto,
        device_sync_content::Content as ContentProto,
    },
    mls::message_contents::EncodedContent,
};

const MAX_ATTEMPTS: i32 = 3;

pub struct SyncWorker<Context> {
    client: DeviceSyncClient<Context>,
    receiver: broadcast::Receiver<SyncWorkerEvent>,
    init: OnceCell<()>,
    metrics: Arc<WorkerMetrics<SyncMetric>>,
}

impl<Context> SyncWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    pub fn new(context: Context, metrics: Option<DynMetrics>) -> Self {
        let receiver = context.worker_events().subscribe();
        let metrics = metrics
            .and_then(|m| m.as_sync_metrics())
            .unwrap_or(Arc::new(WorkerMetrics::new(context.installation_id())));
        let client = DeviceSyncClient::new(context, metrics.clone());

        Self {
            client,
            receiver,
            init: OnceCell::new(),
            metrics,
        }
    }
}

struct Factory<Context> {
    context: Context,
}

impl<Context> WorkerFactory for Factory<Context>
where
    Context: XmtpSharedContext + 'static,
{
    fn create(&self, metrics: Option<DynMetrics>) -> (BoxedWorker, Option<DynMetrics>) {
        let worker = SyncWorker::new(self.context.clone(), metrics);
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

    fn metrics(&self) -> Option<DynMetrics> {
        Some(self.metrics.clone())
    }

    fn factory<C>(context: C) -> impl WorkerFactory + 'static
    where
        C: XmtpSharedContext + 'static,
    {
        Factory { context }
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
        self.metrics.increment_metric(SyncMetric::Init);

        let tick_fut = Self::tick(self.client.context.clone());
        let run_fut = self.run_internal();

        tokio::select! {
            _ = tick_fut => Ok(()),
            res = run_fut => res,
        }
    }

    async fn run_internal(&mut self) -> Result<(), DeviceSyncError> {
        use tokio::sync::broadcast::error::RecvError;
        loop {
            let event = match self.receiver.recv().await {
                Ok(event) => event,
                Err(RecvError::Lagged(skipped)) => {
                    // The skipped events may have included NewSyncGroupFromWelcome,
                    // whose durable task rows were never created. Re-scheduling is
                    // cheap and deduped, so recover level-triggered instead of
                    // losing the edge; a Tick-equivalent sweep covers skipped
                    // NewSyncGroupMsg events the same way.
                    tracing::warn!(
                        skipped,
                        "sync worker receiver lagged; re-scheduling installation reconciliation"
                    );
                    self.client.schedule_add_installations_to_groups()?;
                    self.evt_new_sync_group_msg(true).await?;
                    continue;
                }
                Err(RecvError::Closed) => break,
            };

            // Tick is the internal timer heartbeat (every 20s): no real work, so
            // dispatch it directly without opening a worker_turn span.
            if matches!(event, SyncWorkerEvent::Tick) {
                self.evt_new_sync_group_msg(true).await?;
                continue;
            }

            tracing::info!(
                installation_id = %self.client.context.installation_id(),
                "new sync worker event: {event:?}",
            );
            self.handle_event(event).await?;
        }
        Ok(())
    }

    #[tracing::instrument(skip_all, fields(worker = ?self.kind(), operation = "worker_turn", event = ?event))]
    async fn handle_event(&mut self, event: SyncWorkerEvent) -> Result<(), DeviceSyncError> {
        match event {
            SyncWorkerEvent::NewSyncGroupFromWelcome(_group_id) => {
                self.evt_new_sync_group_from_welcome().await
            }
            SyncWorkerEvent::NewSyncGroupMsg => self.evt_new_sync_group_msg(false).await,
            SyncWorkerEvent::SyncPreferences(preference_updates) => {
                self.evt_sync_preferences(preference_updates).await
            }
            SyncWorkerEvent::CycleHMAC => self.evt_cycle_hmac().await,
            // Tick is intentionally filtered out in `run_internal` before reaching
            // here, so it never opens a worker_turn span.
            SyncWorkerEvent::Tick => unreachable!("Tick is handled before dispatch"),
        }
    }

    async fn tick(ctx: Context) {
        use futures::StreamExt;
        let (base, jitter) = ctx.worker_interval(
            crate::worker::WorkerKind::DeviceSync,
            Duration::from_secs(20),
        );
        let mut intervals = xmtp_common::time::jittered_interval_stream(base, jitter);
        // The interval stream yields immediately on its first poll; skip that
        // so the first Tick is sent only after a full interval, preserving the
        // original sleep-then-send cadence.
        let _ = intervals.next().await;
        while intervals.next().await.is_some() {
            // We don't need to worry about a mutex lock for device sync
            // to ensure that a sync payload is not being processed by two
            // threads at once because there should only ever be one sync worker
            // and the sync worker processes all events in series.
            let _ = ctx.worker_events().send(SyncWorkerEvent::Tick);
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
        let updates = self.client.sync_preferences(updates).await?;

        updates.iter().for_each(|update| match update {
            PreferenceUpdate::Consent(_) => self.metrics.increment_metric(SyncMetric::ConsentSent),
            PreferenceUpdate::Hmac { .. } => self.metrics.increment_metric(SyncMetric::HmacSent),
        });
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
        let conn = self.context.db();
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
                let updated = store_preference_updates(updates.clone(), &conn, handle)?;
                if !updated.is_empty() {
                    let _ = self
                        .context
                        .local_events()
                        .send(LocalEvents::PreferencesChanged(updated));
                }
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
