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

pub(super) fn log_incoming_preference_updates(
    updates: &[xmtp_proto::xmtp::device_sync::content::PreferenceUpdate],
) {
    tracing::info!(update_count = updates.len(), "Incoming preference updates");
}

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

#[cfg(test)]
struct TestTickGate {
    ready: futures::channel::oneshot::Sender<()>,
    release: futures::channel::oneshot::Receiver<()>,
}

pub struct SyncWorker<Context> {
    client: DeviceSyncClient<Context>,
    subscription: Option<Arc<Subscription<InternalEvent>>>,
    pending: PendingEvent,
    next_event_id: Arc<AtomicU64>,
    init: OnceCell<()>,
    metrics: Arc<WorkerMetrics<SyncMetric>>,
    #[cfg(test)]
    completed_turns: Option<Arc<[AtomicU64; 2]>>,
    #[cfg(test)]
    received_ticks: Option<Arc<AtomicU64>>,
    #[cfg(test)]
    tick_gates: std::collections::VecDeque<TestTickGate>,
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
            #[cfg(test)]
            completed_turns: None,
            #[cfg(test)]
            received_ticks: None,
            #[cfg(test)]
            tick_gates: Default::default(),
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
        // Keep the large startup future off the worker poll stack.
        Box::pin(self.sync_init()).await?;
        // Receipt must outlive each sync call so remote updates can wake this worker.
        let _receipt = IncomingCoordinator::for_context(&self.client.context)
            .acquire(IncomingScope::DeviceSyncGroups);
        self.metrics.increment_metric(SyncMetric::Init);

        // Keep event futures off the containing worker poll stack.
        Box::pin(self.run_internal()).await
    }

    async fn run_internal(&mut self) -> Result<(), DeviceSyncError> {
        use futures::StreamExt;
        let (base, jitter) = self
            .client
            .context
            .worker_interval(WorkerKind::DeviceSync, Duration::from_secs(20));
        let intervals = xmtp_common::time::jittered_interval_stream(base, jitter);
        #[cfg(test)]
        let intervals = {
            let gates = Arc::new(Mutex::new(std::mem::take(&mut self.tick_gates)));
            let intervals = intervals.then(move |instant| {
                let gate = gates.lock().pop_front();
                async move {
                    if let Some(gate) = gate {
                        let _ = gate.ready.send(());
                        let _ = gate.release.await;
                    }
                    instant
                }
            });
            xmtp_common::wasm_or_native! {
                native => { intervals.boxed() },
                wasm => { intervals.boxed_local() },
            }
        };
        let mut intervals = intervals;
        // Poll events while native startup jitter delays the first tick.
        let mut skip_initial_tick = cfg!(not(target_arch = "wasm32"));
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
                _ = intervals.next() => {
                    #[cfg(test)]
                    if let Some(ticks) = &self.received_ticks {
                        ticks.fetch_add(1, Ordering::SeqCst);
                    }
                    if skip_initial_tick {
                        skip_initial_tick = false;
                        continue;
                    }
                    self.evt_new_sync_group_msg(true).await?;
                },
            }
        }
        Ok(())
    }

    async fn handle_pending_event(
        &mut self,
        id: u64,
        event: xmtp_events::EventEnvelope<InternalEvent>,
    ) -> Result<(), DeviceSyncError> {
        // Pending state stays owned here until the event succeeds.
        Box::pin(self.handle_event(event.clone())).await?;
        let mut pending = self.pending.lock();
        if pending
            .as_ref()
            .is_some_and(|(pending_id, _)| *pending_id == id)
        {
            *pending = None;
        }
        Ok(())
    }

    #[tracing::instrument(skip_all, fields(worker = ?self.kind(), operation = "worker_turn"))]
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
            log_event!(
                Event::DeviceSyncInitializing,
                self.client.context.installation_id()
            );

            // The only thing that sync init really does right now is ensures that there's a sync group.
            if client.primary_sync_group()?.is_none() {
                log_event!(
                    Event::DeviceSyncNoPrimarySyncGroup,
                    self.client.context.installation_id()
                );
                // Sync-group creation polls membership publication below this call.
                let sync_group = Box::pin(client.get_sync_group()).await?;
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

        let result = self
            .client
            .process_sync_group_messages(&self.metrics, unprocessed_messages)
            .await;
        #[cfg(test)]
        if result.is_ok()
            && let Some(turns) = &self.completed_turns
        {
            turns[usize::from(is_tick)].fetch_add(1, Ordering::SeqCst);
        }
        result
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
    pub(super) async fn process_sync_group_messages(
        &self,
        handle: &WorkerMetrics<SyncMetric>,
        messages: Vec<StoredGroupMessage>,
    ) -> Result<(), DeviceSyncError>
    where
        Context::Db: 'static,
    {
        let installation_id = self.installation_id();

        for msg in messages {
            // Older builds stored sync groups from other inboxes.
            // implements: SYNC-011
            if msg.sender_inbox_id != self.context.inbox_id() {
                tracing::warn!(
                    group_id = %msg.group_id,
                    sender_inbox_id = %msg.sender_inbox_id,
                    "dropping a sync message from another inbox"
                );
                self.context
                    .db()
                    .mark_device_sync_msg_as_processed(&msg.id)?;
                continue;
            }
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
                    log_incoming_preference_updates(&updates);
                }
                // implements: PROC-036
                tracing::info!(update_count = updates.len(), "storing preference updates");
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

#[cfg(test)]
mod startup_tests {
    use super::*;
    use crate::{tester, worker::WorkerConfig};
    use futures::FutureExt;
    use xmtp_events::EventWriter;

    #[xmtp_common::test(unwrap_try = true)]
    async fn queued_event_precedes_first_periodic_turn() {
        const PERIOD: Duration = Duration::from_secs(4);
        const EVENT_WAIT: Duration = Duration::from_secs(1);
        const PERIOD_WAIT: Duration = Duration::from_secs(6);
        let config = WorkerConfig {
            interval_overrides: [(WorkerKind::DeviceSync, PERIOD.as_nanos() as u64)]
                .into_iter()
                .collect(),
            ..Default::default()
        };
        tester!(alix, disable_workers, worker_config: config);
        let turns = Arc::new([AtomicU64::new(0), AtomicU64::new(0)]);
        let mut worker = SyncWorker::new(
            alix.context.clone(),
            None,
            Arc::new(Mutex::new(None)),
            Arc::new(AtomicU64::new(0)),
        );
        worker.completed_turns = Some(turns.clone());
        let (filter, depth) = crate::worker::worker_event_filter(WorkerKind::DeviceSync).unwrap();
        let subscription = Arc::new(alix.context.events().subscribe(filter, depth));
        worker.set_subscription(subscription.clone());
        alix.context
            .events()
            .emit(None, Some(InternalEvent::SyncMessagePublished));

        {
            let run = worker.run_internal().fuse();
            let observe = async {
                xmtp_common::time::timeout(
                    EVENT_WAIT,
                    xmtp_common::wait_for_eq(|| async { turns[0].load(Ordering::SeqCst) }, 1),
                )
                .await
                .expect("queued event must complete before the first periodic tick")
                .expect("queued event wait must complete");
                assert_eq!(turns[1].load(Ordering::SeqCst), 0);
                xmtp_common::time::timeout(
                    PERIOD_WAIT,
                    xmtp_common::wait_for_eq(
                        || async { turns[1].load(Ordering::SeqCst) >= 1 },
                        true,
                    ),
                )
                .await
                .expect("periodic reconciliation must still complete")
                .expect("periodic turn wait must complete");
                assert_eq!(turns[0].load(Ordering::SeqCst), 1);
            }
            .fuse();
            futures::pin_mut!(run, observe);
            futures::select! {
                () = observe => {},
                result = run => panic!("worker ended before observed turns: {result:?}"),
            }
        }
        subscription.close();
        alix.close().await?;
    }
    struct TickControl {
        gate: TestTickGate,
        ready: futures::channel::oneshot::Receiver<()>,
        release: futures::channel::oneshot::Sender<()>,
    }

    impl TickControl {
        fn new() -> Self {
            let (ready_tx, ready) = futures::channel::oneshot::channel();
            let (release, release_rx) = futures::channel::oneshot::channel();
            Self {
                gate: TestTickGate {
                    ready: ready_tx,
                    release: release_rx,
                },
                ready,
                release,
            }
        }
    }

    fn jitter_config() -> WorkerConfig {
        const PERIOD: Duration = Duration::from_millis(40);
        const JITTER: Duration = Duration::from_millis(20);
        WorkerConfig {
            interval_overrides: [(WorkerKind::DeviceSync, PERIOD.as_nanos() as u64)]
                .into_iter()
                .collect(),
            jitter_overrides: [(WorkerKind::DeviceSync, JITTER.as_nanos() as u64)]
                .into_iter()
                .collect(),
            ..Default::default()
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn jittered_initial_tick_does_not_block_events_or_change_first_turn() {
        const WAIT: Duration = Duration::from_secs(1);
        tester!(alix, disable_workers, worker_config: jitter_config());
        let turns = Arc::new([AtomicU64::new(0), AtomicU64::new(0)]);
        let ticks = Arc::new(AtomicU64::new(0));
        let mut worker = SyncWorker::new(
            alix.context.clone(),
            None,
            Arc::new(Mutex::new(None)),
            Arc::new(AtomicU64::new(0)),
        );
        worker.completed_turns = Some(turns.clone());
        worker.received_ticks = Some(ticks.clone());
        let first = TickControl::new();
        let second = TickControl::new();
        worker.tick_gates.extend([first.gate, second.gate]);
        let (filter, depth) = crate::worker::worker_event_filter(WorkerKind::DeviceSync).unwrap();
        let subscription = Arc::new(alix.context.events().subscribe(filter, depth));
        worker.set_subscription(subscription.clone());
        {
            let run = worker.run_internal().fuse();
            let observe = async {
                xmtp_common::time::timeout(WAIT, first.ready)
                    .await
                    .expect("actual jittered first tick must reach the private gate")
                    .expect("first tick gate must remain owned");
                alix.context
                    .events()
                    .emit(None, Some(InternalEvent::SyncMessagePublished));
                xmtp_common::time::timeout(
                    WAIT,
                    xmtp_common::wait_for_eq(|| async { turns[0].load(Ordering::SeqCst) }, 1),
                )
                .await
                .expect("queued event must complete while the initial tick is pending")
                .expect("queued event wait must complete");
                assert_eq!(turns[1].load(Ordering::SeqCst), 0);
                first.release.send(()).expect("release the first tick");
                xmtp_common::time::timeout(WAIT, second.ready)
                    .await
                    .expect("actual jittered second tick must reach the private gate")
                    .expect("second tick gate must remain owned");
                assert_eq!(ticks.load(Ordering::SeqCst), 1);
                let first_periodic = u64::from(cfg!(target_arch = "wasm32"));
                assert_eq!(
                    turns[1].load(Ordering::SeqCst),
                    first_periodic,
                    "native skips its first tick; Wasm reconciles its first tick"
                );
                second.release.send(()).expect("release the second tick");
                xmtp_common::time::timeout(
                    WAIT,
                    xmtp_common::wait_for_eq(
                        || async { turns[1].load(Ordering::SeqCst) > first_periodic },
                        true,
                    ),
                )
                .await
                .expect("periodic reconciliation must complete after the second tick")
                .expect("periodic reconciliation wait must complete");
            }
            .fuse();
            futures::pin_mut!(run, observe);
            futures::select! {
                () = observe => {},
                result = run => panic!("worker ended before observed turns: {result:?}"),
            }
        }
        subscription.close();
        alix.close().await?;
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn closing_subscription_stops_worker_with_initial_tick_pending() {
        const WAIT: Duration = Duration::from_secs(1);
        tester!(alix, disable_workers, worker_config: jitter_config());
        let mut worker = SyncWorker::new(
            alix.context.clone(),
            None,
            Arc::new(Mutex::new(None)),
            Arc::new(AtomicU64::new(0)),
        );
        let first = TickControl::new();
        worker.tick_gates.push_back(first.gate);
        let (filter, depth) = crate::worker::worker_event_filter(WorkerKind::DeviceSync).unwrap();
        let subscription = Arc::new(alix.context.events().subscribe(filter, depth));
        worker.set_subscription(subscription.clone());
        let (result, ()) = xmtp_common::time::timeout(WAIT, async {
            futures::join!(worker.run_internal(), async {
                first
                    .ready
                    .await
                    .expect("actual first tick reaches the gate");
                subscription.close();
            })
        })
        .await
        .expect("closed subscription must stop while the initial tick is pending");
        result?;
        assert!(
            first.release.is_canceled(),
            "ending the worker must drop the pending tick receiver"
        );
        alix.close().await?;
    }
}
