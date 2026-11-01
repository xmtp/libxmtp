pub mod device_sync;
pub mod disappearing_messages;
pub use crate::core::hmac_epoch;
pub mod key_package_maintenance;
pub mod metrics;
pub(crate) mod notifications;
pub mod tasks;

use crate::context::CoreContext;
use crate::core::events::InternalEvent;
use crate::core::metrics::SyncMetric;
pub use crate::core::reconnect::{LocalNeedsDbReconnect, NeedsDbReconnect};
use crate::core::task_channels::TaskWorkerChannels;
use futures::future::{AbortHandle, Abortable};
use futures::{StreamExt, stream::FuturesUnordered};
use metrics::WorkerMetrics;
use parking_lot::Mutex;
use std::fmt::Debug;
use std::pin::Pin;
use std::{collections::HashMap, hash::Hash, sync::Arc};
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use tracing::instrument::Instrumented;
use xmtp_common::{MaybeSend, MaybeSync, StreamHandle, if_native, if_wasm, time::Duration};
use xmtp_configuration::WORKER_RESTART_DELAY;
use xmtp_events::{EventFilter, Subscription};

/// Hard cap on how long `WorkerRunner::shutdown` waits for the supervisor
/// task to drain after cancellation. Anything beyond this gets logged and
/// the task is allowed to detach — keeps `Client::close` bounded.
const WORKER_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);

#[cfg(test)]
pub(crate) mod test_hooks {
    use std::sync::Arc;
    use tokio::sync::Notify;

    type PauseHook = (Vec<u8>, Arc<Notify>, Arc<Notify>);
    pub(crate) static PAUSE_NEXT_SPAWN: parking_lot::Mutex<Option<PauseHook>> =
        parking_lot::Mutex::new(None);
}

#[derive(PartialEq, Eq, Copy, Clone, Hash, Debug)]
pub enum WorkerKind {
    AttachmentCleanup,
    DeviceSync,
    DisappearingMessages,
    KeyPackageCleaner,
    CommitLog,
    TaskRunner,
    /// Re-reads what the backend publishes about itself, hourly.
    ConfigurationRefresh,
    HmacEpoch,
}

/// Buffer depth of a worker subscription that may drop events: each wake only
/// makes the worker read its stored state again.
pub(crate) const EVENT_BUFFER: usize = 10;

/// Configuration for the cadence and enablement of background workers.
///
/// `Default` (all-`None`, empty maps) reproduces the historical behavior:
/// every worker enabled, each using its own compiled-in interval const, no
/// jitter. All fields are opt-in so existing callers are unaffected.
///
/// Durations are nanoseconds, matching [`crate::core::config::ForkRecoveryOpts`].
#[derive(Clone, Debug, Default)]
pub struct WorkerConfig {
    /// Global fallback interval (ns) for any worker without a per-kind
    /// override. `None` => each worker uses its own const default.
    pub default_interval_ns: Option<u64>,
    /// Per-worker interval override (ns). Wins over `default_interval_ns`.
    pub interval_overrides: HashMap<WorkerKind, u64>,
    /// Per-worker jitter (ns). An absent entry means `0` (deterministic).
    /// Jitter de-synchronizes a fleet of clients; scoping it per-worker
    /// avoids blanketing fast workers with a large jitter meant for a slow
    /// one (e.g. the daily CommitLog worker).
    pub jitter_overrides: HashMap<WorkerKind, u64>,
    /// Per-worker enable flag. An absent entry means enabled.
    pub enabled: HashMap<WorkerKind, bool>,
}

impl WorkerConfig {
    /// Resolve `(base, jitter)` for a worker.
    ///
    /// Base precedence: per-kind override > global default > `const_default`.
    /// A resolved base of `0` is clamped to `const_default` to avoid a
    /// pathological busy-loop. Jitter is the per-kind `jitter_overrides` entry
    /// (0 if absent).
    pub fn interval(&self, kind: WorkerKind, const_default: Duration) -> (Duration, Duration) {
        let base_ns = self
            .interval_overrides
            .get(&kind)
            .copied()
            .or(self.default_interval_ns);
        let base = match base_ns {
            Some(0) | None => const_default,
            Some(ns) => Duration::from_nanos(ns),
        };
        let jitter = self
            .jitter_overrides
            .get(&kind)
            .copied()
            .map(Duration::from_nanos)
            .unwrap_or(Duration::ZERO);
        (base, jitter)
    }

    /// `true` unless an explicit `false` entry exists for `kind`.
    pub fn worker_enabled(&self, kind: WorkerKind) -> bool {
        self.enabled.get(&kind).copied().unwrap_or(true)
    }
}

pub struct WorkerRunner {
    // When this is cloned into the Context this is empty, so the Context and Client have different views
    factories: Vec<DynFactory>,
    sync_metrics: SyncMetricsSlot,
    task_channels: TaskWorkerChannels,
    handle: Mutex<Option<Box<dyn StreamHandle<StreamOutput = ()>>>>,
    // Per-worker abort handles. `shutdown` calls `abort()` on each so the
    // worker future is dropped at its next poll regardless of whether the
    // outer loop observed the cancellation token. Belt to the token's
    // suspenders for workers that don't yield to cancellation promptly.
    abort_handles: Mutex<Vec<AbortHandle>>,
    subscriptions: HashMap<WorkerKind, Arc<Subscription<InternalEvent>>>,
}

impl Default for WorkerRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkerRunner {
    pub fn new() -> Self {
        Self {
            factories: Vec::new(),
            sync_metrics: SyncMetricsSlot::default(),
            task_channels: TaskWorkerChannels::default(),
            handle: Mutex::default(),
            abort_handles: Mutex::default(),
            subscriptions: HashMap::new(),
        }
    }

    /// The slot the device-sync worker's metrics land in, shared with the context.
    pub fn sync_metrics_slot(&self) -> &SyncMetricsSlot {
        &self.sync_metrics
    }

    pub fn sync_metrics(&self) -> Option<Arc<WorkerMetrics<SyncMetric>>> {
        self.sync_metrics.lock().clone()
    }

    pub fn task_channels(&self) -> &TaskWorkerChannels {
        &self.task_channels
    }

    /// True while the supervisor handle is held; false after a successful
    /// `shutdown` or before the first `spawn`. Used in tests and as a
    /// cheap liveness check.
    pub fn is_running(&self) -> bool {
        self.handle.lock().is_some()
    }

    /// The kinds of every registered worker factory. Used in tests to assert
    /// which workers a builder configuration registered.
    #[cfg(test)]
    pub(crate) fn registered_kinds(&self) -> Vec<WorkerKind> {
        self.factories.iter().map(|f| f.kind()).collect()
    }

    #[cfg(test)]
    pub(crate) fn subscribed_kinds(&self) -> Vec<WorkerKind> {
        self.subscriptions.keys().copied().collect()
    }

    #[cfg(test)]
    pub(crate) fn subscription_for_test(
        &self,
        kind: WorkerKind,
    ) -> Option<Arc<Subscription<InternalEvent>>> {
        self.subscriptions.get(&kind).cloned()
    }
}

impl WorkerRunner {
    pub fn register_new_worker<W: NewWorker<C>, C>(&mut self, ctx: C)
    where
        C: CoreContext + 'static,
    {
        let events = ctx.events().clone();
        let factory = W::factory(ctx);
        let kind = factory.kind();
        if let Some((filter, depth)) = factory.event_interest() {
            self.subscriptions
                .insert(kind, Arc::new(events.subscribe(filter, depth)));
        }
        self.factories.push(Arc::new(factory))
    }

    pub fn spawn<C>(self: &Arc<Self>, ctx: C)
    where
        C: CoreContext + 'static,
    {
        let mut handle_lock = self.handle.lock();
        if let Some(handle) = handle_lock.take() {
            handle.abort_handle().end();
        }
        // Force-abort any prior worker futures still alive from a previous spawn.
        for h in self.abort_handles.lock().drain(..) {
            h.abort();
        }

        let this = self.clone();
        let cancel = ctx.cancellation_token().clone();
        let handle = xmtp_common::spawn(
            None,
            async move {
                while !ctx.identity().is_ready() {
                    tokio::select! {
                        () = xmtp_common::time::sleep(Duration::from_millis(50)) => {},
                        () = cancel.cancelled() => {
                            this.close_subscriptions();
                            return;
                        }
                    }
                }

                #[cfg(test)]
                {
                    let paused = {
                        let mut hook = test_hooks::PAUSE_NEXT_SPAWN.lock();
                        if hook.as_ref().is_some_and(|(installation, _, _)| {
                            installation.as_slice() == ctx.installation_id().to_vec().as_slice()
                        }) {
                            hook.take()
                        } else {
                            None
                        }
                    };
                    if let Some((_, entered, release)) = paused {
                        entered.notify_one();
                        release.notified().await;
                    }
                }

                let mut futs = FuturesUnordered::new();
                let mut new_handles = Vec::with_capacity(this.factories.len());

                for factory in &this.factories {
                    let mut worker = factory.create();
                    if let Some(subscription) = this.subscriptions.get(&factory.kind()) {
                        worker.set_subscription(subscription.clone());
                    }

                    if let Some(metrics) = worker.sync_metrics() {
                        *this.sync_metrics.lock() = Some(metrics);
                    }
                    let (abort_handle, reg) = AbortHandle::new_pair();
                    new_handles.push(abort_handle);
                    futs.push(Abortable::new(
                        xmtp_common::bind_task_hub(worker.spawn(cancel.clone())),
                        reg,
                    ));
                }
                *this.abort_handles.lock() = new_handles;

                while let Some(outcome) = futs.next().await {
                    match outcome {
                        Ok(kind) => tracing::warn!("Worker {kind:?} completed unexpectedly"),
                        Err(_aborted) => tracing::debug!("worker aborted during shutdown"),
                    }
                }
                if cancel.is_cancelled() {
                    this.close_subscriptions();
                }
            }
            .instrument(tracing::debug_span!("xmtp_worker_supervisor")),
        );

        *handle_lock = Some(Box::new(handle));
    }

    /// Drain the running worker supervisor. Bounded by [`WORKER_SHUTDOWN_TIMEOUT`].
    /// Cancellation must be signalled separately (via the shared
    /// `CancellationToken` on the context); this method additionally aborts
    /// each worker's future to guarantee no further DB writes happen,
    /// independent of whether the worker's loop respects the token.
    pub async fn shutdown(&self) {
        // Hard-kill each worker future at its next poll. Critical for workers
        // that sit on long sleeps and only check the token between intervals.
        for h in self.abort_handles.lock().drain(..) {
            h.abort();
        }
        let mut handle = self.handle.lock().take();
        let Some(handle) = handle.as_mut() else {
            self.close_subscriptions();
            return;
        };
        match xmtp_common::time::timeout(WORKER_SHUTDOWN_TIMEOUT, handle.end_and_wait()).await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => tracing::debug!("worker supervisor ended with: {e:?}"),
            Err(_) => tracing::warn!(
                "worker supervisor did not drain within {:?}; abandoning",
                WORKER_SHUTDOWN_TIMEOUT
            ),
        }
        self.close_subscriptions();
    }

    fn close_subscriptions(&self) {
        for subscription in self.subscriptions.values() {
            subscription.close();
        }
    }

    pub async fn wait_for_sync_worker_init(&self) {
        let handle = self.sync_metrics.lock().clone();
        if let Some(handle) = handle {
            let _ = handle.wait_for_init().await;
        }
    }
}

pub type WorkerResult<T> = Result<T, Box<dyn NeedsDbReconnect>>;
if_native! {
    type SpawnWorkerFut = dyn Future<Output = WorkerKind> + Send;
}
if_wasm! {
    type SpawnWorkerFut = dyn Future<Output = WorkerKind>;
}

#[xmtp_common::async_trait]
pub trait Worker: MaybeSend + MaybeSync + 'static {
    fn kind(&self) -> WorkerKind;

    fn set_subscription(&mut self, _subscription: Arc<Subscription<InternalEvent>>) {}

    async fn run_tasks(&mut self) -> Result<(), Box<dyn NeedsDbReconnect>>;

    /// The metrics of the device-sync worker; no other worker has any.
    fn sync_metrics(&self) -> Option<Arc<WorkerMetrics<SyncMetric>>> {
        None
    }

    /// Box the worker, erasing its type
    fn boxed(self) -> Box<dyn Worker>
    where
        Self: Sized,
    {
        Box::new(self) as Box<_>
    }

    // Wrap the outer loop (not each `run_tasks` impl) so individual workers
    // observe cancellation by having their in-flight future dropped at the
    // next await point — no per-impl plumbing required.
    fn spawn(
        mut self: Box<Self>,
        cancel: CancellationToken,
    ) -> Instrumented<Pin<Box<SpawnWorkerFut>>> {
        let kind_str = format!("{:?}", self.kind());
        let fut = Box::pin(async move {
            let kind = self.kind();
            let worker = format!("{:?}", kind);
            let run = async move {
                loop {
                    // No span here: `run_tasks` runs until worker death, so a
                    // wrapping span exports only on restart with an hours-long
                    // duration. Per-tick visibility comes from `worker_turn`;
                    // restarts from the structured logs below.
                    let outcome = self.run_tasks().await;
                    if let Err(err) = outcome {
                        if err.needs_db_reconnect() {
                            // drop the worker
                            tracing::debug!("pool disconnected. task will restart on reconnect");
                            break;
                        } else {
                            tracing::error!(worker = %worker, "{:?} worker error: {}", kind, err);
                            // A restart resends the refused work, so it waits
                            // out a rate-limit delay first. A delay too large
                            // for the clock saturates instead of overflowing.
                            // implements: RATE-041
                            let delay = err.restart_delay().map_or(WORKER_RESTART_DELAY, |delay| {
                                delay.max(WORKER_RESTART_DELAY)
                            });
                            let now = xmtp_common::time::Instant::now();
                            let until = xmtp_common::time::saturating_after(now, delay);
                            xmtp_common::time::sleep(until.saturating_duration_since(now)).await;
                            tracing::info!(worker = %worker, "Restarting {:?} worker...", kind);
                        }
                    }
                }
                self.kind()
            };

            tokio::select! {
                k = run => k,
                _ = cancel.cancelled() => {
                    tracing::debug!("{:?} worker cancelled", kind);
                    kind
                }
            }
        }) as Pin<Box<SpawnWorkerFut>>;
        fut.instrument(tracing::debug_span!("libxmtp_worker", kind = kind_str))
    }
}

/// A worker that can be built on contexts of type `C`.
pub trait NewWorker<C> {
    /// The factory the runner rebuilds this worker with after a restart.
    fn factory(context: C) -> impl WorkerFactory + 'static;
}

pub trait WorkerFactory: MaybeSend + MaybeSync {
    fn kind(&self) -> WorkerKind;

    /// The events that wake the worker: public event kinds plus an internal
    /// predicate, and the subscription's buffer depth (`None` drops nothing).
    /// `None` when no event wakes it.
    fn event_interest(&self) -> Option<(EventFilter<InternalEvent>, Option<usize>)> {
        None
    }

    /// Create a new worker
    fn create(&self) -> BoxedWorker;
}

pub type BoxedWorker = Box<dyn Worker>;
pub type DynFactory = Arc<dyn WorkerFactory>;

/// Where the device-sync worker's metrics land once the runner first builds
/// that worker. Empty while no device-sync worker has started.
pub type SyncMetricsSlot = Arc<Mutex<Option<Arc<WorkerMetrics<SyncMetric>>>>>;

#[cfg(test)]
pub(crate) mod event_subscription_tests {
    use super::*;
    use crate::subscriptions::internal::DeletedMessages;
    use crate::worker::device_sync::preference_sync::PreferenceUpdate;
    use xmtp_events::{EventBus, EventKind, EventWriter};

    /// The factory of every worker the builder can register, on a mock context.
    fn factories() -> Vec<DynFactory> {
        use crate::test::mock::{NewMockContext, context};
        fn factory<W: NewWorker<NewMockContext>>() -> DynFactory {
            Arc::new(W::factory(context()))
        }
        vec![
            factory::<crate::attachments::cleanup::AttachmentCleanup<NewMockContext>>(),
            factory::<crate::worker::device_sync::worker::SyncWorker<NewMockContext>>(),
            factory::<
                crate::worker::disappearing_messages::DisappearingMessagesWorker<NewMockContext>,
            >(),
            factory::<crate::groups::commit_log::CommitLogWorker<NewMockContext>>(),
            factory::<
                crate::worker::tasks::TaskWorker<
                    NewMockContext,
                    crate::task_handlers::MlsTaskHandlers,
                >,
            >(),
            factory::<crate::core::config_refresh::ConfigurationWorker<NewMockContext>>(),
            factory::<hmac_epoch::HmacEpochWorker<NewMockContext>>(),
        ]
    }

    /// The event interest of the worker of `kind`; `None` when no worker of
    /// that kind exists or no event wakes it.
    pub(crate) fn worker_event_filter(
        kind: WorkerKind,
    ) -> Option<(EventFilter<InternalEvent>, Option<usize>)> {
        factories()
            .into_iter()
            .find(|factory| factory.kind() == kind)?
            .event_interest()
    }

    /// What a worker of `kind` subscribes to: its public kinds, its buffer
    /// depth, and the labels of the `events` its internal predicate admits.
    fn event_interest_for(
        kind: WorkerKind,
        events: &[(&'static str, InternalEvent)],
    ) -> Option<(Vec<EventKind>, Option<usize>, Vec<&'static str>)> {
        let (filter, depth) = worker_event_filter(kind)?;
        let kinds = filter.kinds.clone();
        let bus = EventBus::new();
        let subscription = bus.subscribe(filter, depth);
        let mut admitted = vec![];
        for (label, event) in events {
            bus.emit(None, Some(event.clone()));
            if !subscription.drain().is_empty() {
                admitted.push(*label);
            }
        }
        Some((kinds, depth, admitted))
    }

    /// One labelled sample of every internal event, with a matching and a
    /// non-matching value for each field a worker predicate reads. The match
    /// has no wildcard, so a new variant fails to compile here until it has a
    /// sample.
    fn internal_event_samples() -> Vec<(&'static str, InternalEvent)> {
        let group_id = xmtp_proto::types::GroupId::from([7; 16]);
        let stored = |expires_at_ns, is_sync| InternalEvent::MessageStored {
            group_id,
            message_id: vec![1],
            expires_at_ns,
            is_sync,
        };
        let deleted = || DeletedMessages {
            messages: vec![],
            deleted_by_inbox_id: None,
        };
        let samples = vec![
            (
                "GroupJoined{is_sync:false}",
                InternalEvent::GroupJoined {
                    group_id,
                    is_sync: false,
                },
            ),
            (
                "GroupJoined{is_sync:true}",
                InternalEvent::GroupJoined {
                    group_id,
                    is_sync: true,
                },
            ),
            (
                "MessageStored{expires:None,is_sync:false}",
                stored(None, false),
            ),
            (
                "MessageStored{expires:None,is_sync:true}",
                stored(None, true),
            ),
            (
                "MessageStored{expires:Some,is_sync:false}",
                stored(Some(1), false),
            ),
            (
                "MessageStored{expires:Some,is_sync:true}",
                stored(Some(1), true),
            ),
            ("OutboxChanged", InternalEvent::OutboxChanged),
            ("TaskScheduled", InternalEvent::TaskScheduled),
            (
                "NotificationSettingsChanged",
                InternalEvent::NotificationSettingsChanged,
            ),
            (
                "PreferencesChanged{empty}",
                InternalEvent::PreferencesChanged { updates: vec![] },
            ),
            (
                "PreferencesChanged{hmac}",
                InternalEvent::PreferencesChanged {
                    updates: vec![PreferenceUpdate::Hmac {
                        key: vec![3; 32].into(),
                        cycled_at_ns: 1,
                    }],
                },
            ),
            ("MessagesDeleted", InternalEvent::MessagesDeleted(deleted())),
            ("MessagesExpired", InternalEvent::MessagesExpired(deleted())),
        ];
        for (_, event) in &samples {
            match event {
                InternalEvent::GroupJoined { .. }
                | InternalEvent::MessageStored { .. }
                | InternalEvent::OutboxChanged
                | InternalEvent::TaskScheduled
                | InternalEvent::NotificationSettingsChanged
                | InternalEvent::PreferencesChanged { .. }
                | InternalEvent::MessagesDeleted(_)
                | InternalEvent::MessagesExpired(_) => {}
            }
        }
        samples
    }

    /// Which events wake each worker, frozen before the crate split. The
    /// match has no wildcard, so a new worker kind needs a row.
    fn frozen_interest(
        kind: WorkerKind,
    ) -> Option<(Vec<EventKind>, Option<usize>, Vec<&'static str>)> {
        match kind {
            WorkerKind::DeviceSync => Some((
                vec![],
                None,
                vec![
                    "MessageStored{expires:None,is_sync:true}",
                    "MessageStored{expires:Some,is_sync:true}",
                    "OutboxChanged",
                ],
            )),
            WorkerKind::DisappearingMessages => Some((
                vec![EventKind::ConversationMetadataChanged],
                Some(10),
                vec![
                    "MessageStored{expires:Some,is_sync:false}",
                    "MessageStored{expires:Some,is_sync:true}",
                ],
            )),
            WorkerKind::TaskRunner => Some((
                vec![
                    EventKind::ConsentChanged,
                    EventKind::ConversationJoined,
                    EventKind::ConversationRemoved,
                    EventKind::HmacKeysUpdated,
                    EventKind::ArchiveRestored,
                ],
                Some(10),
                vec![
                    "GroupJoined{is_sync:false}",
                    "GroupJoined{is_sync:true}",
                    "TaskScheduled",
                    "NotificationSettingsChanged",
                    "PreferencesChanged{empty}",
                    "PreferencesChanged{hmac}",
                ],
            )),
            WorkerKind::AttachmentCleanup
            | WorkerKind::KeyPackageCleaner
            | WorkerKind::CommitLog
            | WorkerKind::ConfigurationRefresh
            | WorkerKind::HmacEpoch => None,
        }
    }

    #[xmtp_common::test]
    fn worker_event_interest_matches_frozen_table() {
        let samples = internal_event_samples();
        for kind in [
            WorkerKind::AttachmentCleanup,
            WorkerKind::DeviceSync,
            WorkerKind::DisappearingMessages,
            WorkerKind::KeyPackageCleaner,
            WorkerKind::CommitLog,
            WorkerKind::TaskRunner,
            WorkerKind::ConfigurationRefresh,
            WorkerKind::HmacEpoch,
        ] {
            assert_eq!(
                event_interest_for(kind, &samples),
                frozen_interest(kind),
                "{kind:?}"
            );
        }
    }

    /// The outbox and the sync messages are stored state; the worker wakes
    /// only for them, and never drops a wake while busy.
    // verifies: SYNC-029
    #[xmtp_common::test(unwrap_try = true)]
    async fn device_sync_wakes_for_sync_messages_and_the_outbox() {
        let (filter, depth) = worker_event_filter(WorkerKind::DeviceSync).unwrap();
        assert_eq!(depth, None);
        let bus = EventBus::new();
        let subscription = bus.subscribe(filter, depth);
        let group_id = xmtp_proto::types::GroupId::from([9; 16]);
        let stored = |is_sync| InternalEvent::MessageStored {
            group_id,
            message_id: vec![],
            expires_at_ns: None,
            is_sync,
        };
        for event in [
            InternalEvent::PreferencesChanged { updates: vec![] },
            InternalEvent::GroupJoined {
                group_id,
                is_sync: true,
            },
            stored(false),
        ] {
            bus.emit(None, Some(event));
        }
        assert!(subscription.drain().is_empty());
        for _ in 0..1_000 {
            bus.emit(None, Some(InternalEvent::OutboxChanged));
        }
        bus.emit(None, Some(stored(true)));
        assert_eq!(subscription.drain().len(), 1_001);
    }
}

// These fixtures use native pool errors. Browser connection lifecycle tests
// cover closed persistent handles in the WASM database module.
#[cfg(all(test, not(target_arch = "wasm32")))]
mod disconnect_propagation_tests {
    //! Pins that a dropped-pool signal survives the wrapper error types each
    //! worker surfaces from `run_tasks`, so `needs_db_reconnect()` stays `true`.
    use super::NeedsDbReconnect;
    use crate::groups::GroupError;
    use crate::groups::commit_log::CommitLogError;
    use crate::mls_store::MlsStoreError;
    use crate::subscriptions::SubscribeError;
    use crate::worker::device_sync::DeviceSyncError;
    use crate::worker::key_package_maintenance::KeyPackageMaintenanceError;
    use xmtp_db::{ConnectionError, PlatformStorageError, StorageError};

    /// A `StorageError` that signals the connection pool was dropped.
    fn disconnect_storage() -> StorageError {
        StorageError::Platform(PlatformStorageError::PoolNeedsConnection)
    }

    /// A `ConnectionError` that signals the connection pool was dropped.
    fn disconnect_connection() -> ConnectionError {
        ConnectionError::Platform(PlatformStorageError::PoolNeedsConnection)
    }

    /// A storage error that is NOT a disconnect — must never trip the contract.
    fn benign_storage() -> StorageError {
        StorageError::InvalidHmacLength
    }

    #[xmtp_common::test]
    fn group_error_forwards_disconnect() {
        use crate::subscriptions::barrier::{
            BarrierCause, BarrierError, BarrierFailure, BarrierTopic,
        };
        use crate::subscriptions::incoming::IncomingError;
        use std::sync::Arc;
        use xmtp_proto::types::{Cursor, Topic};

        assert!(GroupError::Storage(disconnect_storage()).needs_db_reconnect());
        assert!(GroupError::Db(disconnect_connection()).needs_db_reconnect());
        assert!(
            GroupError::MlsStore(MlsStoreError::Connection(disconnect_connection()))
                .needs_db_reconnect()
        );
        // A non-disconnect storage failure inside a GroupError must not stop the worker.
        assert!(!GroupError::Storage(benign_storage()).needs_db_reconnect());
        assert!(!GroupError::InvalidGroupMembership.needs_db_reconnect());

        let barrier = |cause| BarrierError::Incomplete {
            reason: BarrierFailure::Blocked,
            unfinished: vec![BarrierTopic {
                topic: Topic::new_group_message([1; 32]),
                scope_generation: None,
                target: Some(Cursor(3)),
                received: Cursor(2),
                processed: Cursor(1),
                unresolved_welcomes: Vec::new(),
                inactive: false,
                cause: Some(cause),
            }],
        };
        assert!(
            GroupError::StreamBarrier(barrier(BarrierCause::Storage(Arc::new(
                disconnect_storage(),
            ))))
            .needs_db_reconnect()
        );
        assert!(
            GroupError::PublishedButUnconfirmed {
                intent_id: 1,
                cause: Some(Box::new(barrier(BarrierCause::Receiver(Arc::new(
                    IncomingError::Processing(
                        crate::groups::mls_sync::GroupMessageProcessingError::Storage(
                            disconnect_storage(),
                        ),
                    ),
                ))))),
                rate_limit_delay: None,
            }
            .needs_db_reconnect()
        );
        assert!(
            GroupError::Sync(Box::new(crate::groups::summary::SyncSummary::other(
                GroupError::Storage(disconnect_storage()),
            )))
            .needs_db_reconnect()
        );
        assert!(
            !GroupError::StreamBarrier(barrier(BarrierCause::Blocked("unsupported".into())))
                .needs_db_reconnect()
        );
    }

    #[xmtp_common::test]
    fn mls_store_error_forwards_disconnect() {
        assert!(MlsStoreError::Storage(disconnect_storage()).needs_db_reconnect());
        assert!(MlsStoreError::Connection(disconnect_connection()).needs_db_reconnect());
        assert!(!MlsStoreError::Storage(benign_storage()).needs_db_reconnect());
    }

    #[xmtp_common::test]
    fn subscribe_error_forwards_disconnect() {
        assert!(SubscribeError::Storage(disconnect_storage()).needs_db_reconnect());
        assert!(SubscribeError::Db(disconnect_connection()).needs_db_reconnect());
        assert!(
            SubscribeError::from(GroupError::Storage(disconnect_storage())).needs_db_reconnect()
        );
        assert!(!SubscribeError::Storage(benign_storage()).needs_db_reconnect());
    }

    // Per-worker `run_tasks` error types — what the supervisor actually inspects.

    #[xmtp_common::test]
    fn task_worker_load_group_forwards_disconnect() {
        use crate::task_handlers::TaskWorkerError;
        // Self-remove tasks load the MLS group (MlsStoreError) and remove members
        // (GroupError); a dropped pool must bubble through both paths.
        assert!(
            TaskWorkerError::LoadGroup(MlsStoreError::Connection(disconnect_connection()))
                .needs_db_reconnect()
        );
        assert!(
            TaskWorkerError::Group(GroupError::Storage(disconnect_storage())).needs_db_reconnect()
        );
        assert!(!TaskWorkerError::Group(GroupError::InvalidGroupMembership).needs_db_reconnect());
    }

    #[xmtp_common::test]
    fn commit_log_error_forwards_disconnect() {
        assert!(CommitLogError::Connection(disconnect_connection()).needs_db_reconnect());
        assert!(
            CommitLogError::GroupError(GroupError::Storage(disconnect_storage()))
                .needs_db_reconnect()
        );
        assert!(
            CommitLogError::SyncError(crate::groups::summary::SyncSummary::other(
                GroupError::Storage(disconnect_storage()),
            ))
            .needs_db_reconnect()
        );
        // A transient (non-disconnect) connection error must not stop the worker.
        assert!(
            !CommitLogError::Connection(ConnectionError::DisconnectInTransaction)
                .needs_db_reconnect()
        );
    }

    #[xmtp_common::test]
    fn device_sync_error_forwards_disconnect() {
        assert!(DeviceSyncError::Storage(disconnect_storage()).needs_db_reconnect());
        assert!(DeviceSyncError::Db(disconnect_connection()).needs_db_reconnect());
        assert!(
            DeviceSyncError::Group(GroupError::Storage(disconnect_storage())).needs_db_reconnect()
        );
        assert!(
            DeviceSyncError::MlsStore(MlsStoreError::Connection(disconnect_connection()))
                .needs_db_reconnect()
        );
        assert!(!DeviceSyncError::Storage(benign_storage()).needs_db_reconnect());
        assert!(!DeviceSyncError::InvalidPayload.needs_db_reconnect());
    }

    /// Every wrapper variant that holds an error with a reconnect signal
    /// forwards a dropped pool, at each level a worker or a client caller
    /// inspects.
    #[xmtp_common::test]
    fn every_wrapper_forwards_db_disconnect() {
        use crate::client::ClientError;
        use crate::client::notifications::NotificationError;
        use crate::groups::{
            mls_sync::GroupMessageProcessingError, summary::SyncSummary,
            validated_commit::CommitValidationError,
        };
        use crate::identity::IdentityError;
        use crate::identity_updates::{
            IdentityDependencyError, IdentityUpdateError, InstallationDiffError,
        };
        use crate::intents::ProcessIntentError;
        use crate::messages::enrichment::EnrichMessageError;
        use crate::server_configuration::ConfigurationFetchError;
        use crate::subscriptions::{incoming::IncomingError, local_delivery::LocalDeliveryError};
        use crate::task_handlers::TaskWorkerError;
        use std::sync::Arc;
        use xmtp_db::{consent_record::ConsentActError, sql_key_store::SqlKeyStoreError};

        let group = || GroupError::Storage(disconnect_storage());
        let processing = || GroupMessageProcessingError::Storage(disconnect_storage());
        let identity_update = || IdentityUpdateError::Storage(disconnect_storage());
        let identity = || IdentityError::StorageError(disconnect_storage());
        let dependency = || IdentityDependencyError::Storage(disconnect_storage());
        let diff = || InstallationDiffError::Storage(disconnect_storage());
        let commit = || CommitValidationError::StorageError(disconnect_storage());
        let summary = || Box::new(SyncSummary::other(group()));
        let incoming = || Arc::new(IncomingError::Storage(disconnect_storage()));
        let local_delivery = || LocalDeliveryError::Storage(disconnect_storage());
        let device_sync = || DeviceSyncError::Storage(disconnect_storage());

        let cases = [
            (
                "ClientError::Storage",
                ClientError::Storage(disconnect_storage()).db_needs_connection(),
            ),
            (
                "ClientError::ConsentAct",
                ClientError::ConsentAct(ConsentActError::Storage(disconnect_storage()))
                    .db_needs_connection(),
            ),
            (
                "ClientError::Db",
                ClientError::Db(disconnect_connection()).db_needs_connection(),
            ),
            (
                "ClientError::Group",
                ClientError::Group(Box::new(group())).db_needs_connection(),
            ),
            (
                "ClientError::Identity",
                ClientError::Identity(identity()).db_needs_connection(),
            ),
            (
                "ClientError::IdentityUpdate",
                ClientError::IdentityUpdate(identity_update()).db_needs_connection(),
            ),
            (
                "ClientError::MlsStore",
                ClientError::MlsStore(MlsStoreError::Connection(disconnect_connection()))
                    .db_needs_connection(),
            ),
            (
                "ClientError::ConfigurationUnavailable",
                ClientError::ConfigurationUnavailable(Box::new(ConfigurationFetchError::Storage(
                    disconnect_storage(),
                )))
                .db_needs_connection(),
            ),
            ("GroupError::Storage", group().needs_db_reconnect()),
            (
                "GroupError::IdentityUpdate",
                GroupError::IdentityUpdate(identity_update()).needs_db_reconnect(),
            ),
            (
                "GroupError::Db",
                GroupError::Db(disconnect_connection()).needs_db_reconnect(),
            ),
            (
                "GroupError::MlsStore",
                GroupError::MlsStore(MlsStoreError::Storage(disconnect_storage()))
                    .needs_db_reconnect(),
            ),
            (
                "GroupError::Identity",
                GroupError::Identity(identity()).needs_db_reconnect(),
            ),
            (
                "GroupError::ReceiveError",
                GroupError::ReceiveError(processing()).needs_db_reconnect(),
            ),
            (
                "GroupError::Sync",
                GroupError::Sync(summary()).needs_db_reconnect(),
            ),
            (
                "GroupError::SyncFailedToWait",
                GroupError::SyncFailedToWait(summary()).needs_db_reconnect(),
            ),
            (
                "GroupError::CommitValidation",
                GroupError::CommitValidation(commit()).needs_db_reconnect(),
            ),
            (
                "GroupError::InstallationDiff",
                GroupError::InstallationDiff(diff()).needs_db_reconnect(),
            ),
            (
                "GroupError::ProcessIntent",
                GroupError::ProcessIntent(ProcessIntentError::Storage(disconnect_storage()))
                    .needs_db_reconnect(),
            ),
            (
                "GroupError::SqlKeyStore",
                GroupError::SqlKeyStore(SqlKeyStoreError::Connection(disconnect_connection()))
                    .needs_db_reconnect(),
            ),
            (
                "GroupMessageProcessingError::Storage",
                processing().needs_db_reconnect(),
            ),
            (
                "GroupMessageProcessingError::Db",
                GroupMessageProcessingError::Db(disconnect_connection()).needs_db_reconnect(),
            ),
            (
                "GroupMessageProcessingError::IdentityUpdate",
                GroupMessageProcessingError::IdentityUpdate(identity_update()).needs_db_reconnect(),
            ),
            (
                "GroupMessageProcessingError::Identity",
                GroupMessageProcessingError::Identity(identity()).needs_db_reconnect(),
            ),
            (
                "GroupMessageProcessingError::PreparedAttempt",
                GroupMessageProcessingError::PreparedAttempt(Box::new(group()))
                    .needs_db_reconnect(),
            ),
            (
                "GroupMessageProcessingError::CommitValidation",
                GroupMessageProcessingError::CommitValidation(commit()).needs_db_reconnect(),
            ),
            (
                "CommitValidationError::StorageError",
                commit().needs_db_reconnect(),
            ),
            (
                "CommitValidationError::InstalledState",
                CommitValidationError::InstalledState(Box::new(commit())).needs_db_reconnect(),
            ),
            (
                "CommitValidationError::IdentityDependency",
                CommitValidationError::IdentityDependency(dependency()).needs_db_reconnect(),
            ),
            (
                "CommitValidationError::InstallationDiff",
                CommitValidationError::InstallationDiff(diff()).needs_db_reconnect(),
            ),
            (
                "IdentityError::StorageError",
                identity().needs_db_reconnect(),
            ),
            (
                "IdentityError::Db",
                IdentityError::Db(disconnect_connection()).needs_db_reconnect(),
            ),
            (
                "IdentityError::OpenMlsStorageError",
                IdentityError::OpenMlsStorageError(SqlKeyStoreError::Connection(
                    disconnect_connection(),
                ))
                .needs_db_reconnect(),
            ),
            (
                "IdentityUpdateError::Storage",
                identity_update().needs_db_reconnect(),
            ),
            (
                "IdentityUpdateError::Db",
                IdentityUpdateError::Db(disconnect_connection()).needs_db_reconnect(),
            ),
            (
                "IdentityUpdateError::Identity",
                IdentityUpdateError::Identity(identity()).needs_db_reconnect(),
            ),
            (
                "IdentityUpdateError::Publish",
                IdentityUpdateError::Publish(Box::new(identity_update())).needs_db_reconnect(),
            ),
            (
                "InstallationDiffError::Storage",
                diff().needs_db_reconnect(),
            ),
            (
                "InstallationDiffError::Db",
                InstallationDiffError::Db(disconnect_connection()).needs_db_reconnect(),
            ),
            (
                "InstallationDiffError::IdentityDependency",
                InstallationDiffError::IdentityDependency(dependency()).needs_db_reconnect(),
            ),
            (
                "InstallationDiffError::IdentityUpdate",
                InstallationDiffError::IdentityUpdate(identity_update()).needs_db_reconnect(),
            ),
            (
                "IdentityDependencyError::Storage",
                dependency().needs_db_reconnect(),
            ),
            (
                "IdentityDependencyError::IdentityUpdate",
                IdentityDependencyError::IdentityUpdate(Box::new(identity_update()))
                    .needs_db_reconnect(),
            ),
            (
                "SubscribeError::Group",
                SubscribeError::from(group()).needs_db_reconnect(),
            ),
            (
                "SubscribeError::Storage",
                SubscribeError::Storage(disconnect_storage()).needs_db_reconnect(),
            ),
            (
                "SubscribeError::Db",
                SubscribeError::Db(disconnect_connection()).needs_db_reconnect(),
            ),
            (
                "SubscribeError::LocalDelivery",
                SubscribeError::LocalDelivery(local_delivery()).needs_db_reconnect(),
            ),
            (
                "SubscribeError::ReceiveGroup",
                SubscribeError::ReceiveGroup(Box::new(processing())).needs_db_reconnect(),
            ),
            (
                "SubscribeError::Enriched(Storage)",
                SubscribeError::Enriched(EnrichMessageError::Storage(disconnect_storage()))
                    .needs_db_reconnect(),
            ),
            (
                "SubscribeError::Enriched(DbConnection)",
                SubscribeError::Enriched(EnrichMessageError::DbConnection(disconnect_connection()))
                    .needs_db_reconnect(),
            ),
            (
                "LocalDeliveryError::Storage",
                local_delivery().needs_db_reconnect(),
            ),
            (
                "LocalDeliveryError::SessionFailure",
                LocalDeliveryError::SessionFailure(Arc::new(local_delivery())).needs_db_reconnect(),
            ),
            (
                "LocalDeliveryError::NetworkRecoveryExhausted",
                LocalDeliveryError::NetworkRecoveryExhausted {
                    attempts: 1,
                    source: Some(incoming()),
                }
                .needs_db_reconnect(),
            ),
            (
                "LocalDeliveryError::NetworkFailure",
                LocalDeliveryError::NetworkFailure(incoming()).needs_db_reconnect(),
            ),
            (
                "DeviceSyncError::Storage",
                device_sync().needs_db_reconnect(),
            ),
            (
                "DeviceSyncError::Db",
                DeviceSyncError::Db(disconnect_connection()).needs_db_reconnect(),
            ),
            (
                "DeviceSyncError::IdentityUpdate",
                DeviceSyncError::IdentityUpdate(identity_update()).needs_db_reconnect(),
            ),
            (
                "DeviceSyncError::Group",
                DeviceSyncError::Group(group()).needs_db_reconnect(),
            ),
            (
                "DeviceSyncError::MlsStore",
                DeviceSyncError::MlsStore(MlsStoreError::Storage(disconnect_storage()))
                    .needs_db_reconnect(),
            ),
            (
                "DeviceSyncError::OwnStateUnavailable",
                DeviceSyncError::OwnStateUnavailable(identity_update()).needs_db_reconnect(),
            ),
            (
                "DeviceSyncError::Sync",
                DeviceSyncError::Sync(summary()).needs_db_reconnect(),
            ),
            (
                "DeviceSyncError::Archive",
                DeviceSyncError::Archive(xmtp_archive::ArchiveError::Storage(disconnect_storage()))
                    .needs_db_reconnect(),
            ),
            (
                "TaskWorkerError::Storage",
                TaskWorkerError::Storage(disconnect_storage()).needs_db_reconnect(),
            ),
            (
                "TaskWorkerError::LoadGroup",
                TaskWorkerError::LoadGroup(MlsStoreError::Storage(disconnect_storage()))
                    .needs_db_reconnect(),
            ),
            (
                "TaskWorkerError::Group",
                TaskWorkerError::Group(group()).needs_db_reconnect(),
            ),
            (
                "TaskWorkerError::DeviceSync",
                TaskWorkerError::DeviceSync(device_sync()).needs_db_reconnect(),
            ),
            (
                "TaskWorkerError::Identity",
                TaskWorkerError::Identity(identity()).needs_db_reconnect(),
            ),
            (
                "TaskWorkerError::KeyPackageMaintenance",
                TaskWorkerError::KeyPackageMaintenance(KeyPackageMaintenanceError::Storage(
                    disconnect_storage(),
                ))
                .needs_db_reconnect(),
            ),
            (
                "TaskWorkerError::Notification(Storage)",
                TaskWorkerError::Notification(NotificationError::Storage(disconnect_storage()))
                    .needs_db_reconnect(),
            ),
            (
                "TaskWorkerError::Notification(Group)",
                TaskWorkerError::Notification(NotificationError::Group(group()))
                    .needs_db_reconnect(),
            ),
            (
                "ConfigurationFetchError::Storage",
                ConfigurationFetchError::Storage(disconnect_storage()).db_needs_connection(),
            ),
        ];
        let hidden: Vec<&str> = cases
            .iter()
            .filter(|(_, forwarded)| !forwarded)
            .map(|(wrapper, _)| *wrapper)
            .collect();
        assert!(
            hidden.is_empty(),
            "wrappers that hide a disconnect: {hidden:?}"
        );
    }

    #[xmtp_common::test]
    fn key_package_maintenance_error_forwards_disconnect() {
        use crate::identity::IdentityError;
        assert!(KeyPackageMaintenanceError::Storage(disconnect_storage()).needs_db_reconnect());
        // Per-key-package delete returns an IdentityError; a disconnect must
        // bubble whether it arrives as a StorageError or a bare ConnectionError.
        assert!(
            KeyPackageMaintenanceError::Identity(IdentityError::StorageError(disconnect_storage()))
                .needs_db_reconnect()
        );
        assert!(
            KeyPackageMaintenanceError::Identity(IdentityError::Db(disconnect_connection()))
                .needs_db_reconnect()
        );
        assert!(!KeyPackageMaintenanceError::Storage(benign_storage()).needs_db_reconnect());
    }
}

#[cfg(test)]
mod worker_config_tests {
    use super::{WorkerConfig, WorkerKind};
    use std::time::Duration;

    #[xmtp_common::test]
    fn default_is_all_enabled_no_overrides() {
        let cfg = WorkerConfig::default();
        assert!(cfg.worker_enabled(WorkerKind::DeviceSync));
        assert!(cfg.worker_enabled(WorkerKind::CommitLog));
        let (base, jitter) = cfg.interval(WorkerKind::DisappearingMessages, Duration::from_secs(7));
        assert_eq!(base, Duration::from_secs(7), "falls back to const default");
        assert_eq!(jitter, Duration::ZERO, "no jitter by default");
    }

    #[xmtp_common::test]
    fn per_kind_override_beats_global_default() {
        let mut cfg = WorkerConfig {
            default_interval_ns: Some(Duration::from_secs(3).as_nanos() as u64),
            ..Default::default()
        };
        cfg.interval_overrides.insert(
            WorkerKind::KeyPackageCleaner,
            Duration::from_secs(9).as_nanos() as u64,
        );
        let (base, _) = cfg.interval(WorkerKind::KeyPackageCleaner, Duration::from_secs(99));
        assert_eq!(base, Duration::from_secs(9), "per-kind override wins");
        let (other, _) = cfg.interval(WorkerKind::CommitLog, Duration::from_secs(99));
        assert_eq!(
            other,
            Duration::from_secs(3),
            "global default for un-overridden worker"
        );
    }

    #[xmtp_common::test]
    fn zero_resolved_base_clamps_to_const() {
        let mut cfg = WorkerConfig::default();
        cfg.interval_overrides.insert(WorkerKind::CommitLog, 0);
        let (base, _) = cfg.interval(WorkerKind::CommitLog, Duration::from_secs(60));
        assert_eq!(
            base,
            Duration::from_secs(60),
            "zero base clamps to const default"
        );
    }

    #[xmtp_common::test]
    fn per_kind_jitter_is_carried() {
        let mut cfg = WorkerConfig::default();
        cfg.jitter_overrides.insert(
            WorkerKind::TaskRunner,
            Duration::from_secs(2).as_nanos() as u64,
        );
        let (_, jitter) = cfg.interval(WorkerKind::TaskRunner, Duration::from_secs(1));
        assert_eq!(jitter, Duration::from_secs(2));
    }

    #[xmtp_common::test]
    fn jitter_is_scoped_per_worker() {
        let mut cfg = WorkerConfig::default();
        cfg.jitter_overrides.insert(
            WorkerKind::CommitLog,
            Duration::from_secs(6 * 3600).as_nanos() as u64,
        );
        // The jittered worker gets its jitter...
        let (_, commit_log_jitter) = cfg.interval(WorkerKind::CommitLog, Duration::from_secs(300));
        assert_eq!(commit_log_jitter, Duration::from_secs(6 * 3600));
        // ...while an un-listed worker stays deterministic.
        let (_, disappearing_jitter) =
            cfg.interval(WorkerKind::DisappearingMessages, Duration::from_secs(1));
        assert_eq!(disappearing_jitter, Duration::ZERO);
    }

    #[xmtp_common::test]
    fn disabled_entry_reports_false() {
        let mut cfg = WorkerConfig::default();
        cfg.enabled.insert(WorkerKind::DeviceSync, false);
        assert!(!cfg.worker_enabled(WorkerKind::DeviceSync));
        assert!(
            cfg.worker_enabled(WorkerKind::CommitLog),
            "absent => enabled"
        );
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod restart_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A worker whose every turn is refused by a rate limit.
    struct RateLimited {
        runs: Arc<AtomicUsize>,
        /// The rejection each turn fails with.
        status: tonic::Status,
    }

    #[xmtp_common::async_trait]
    impl Worker for RateLimited {
        fn kind(&self) -> WorkerKind {
            WorkerKind::CommitLog
        }

        async fn run_tasks(&mut self) -> Result<(), Box<dyn NeedsDbReconnect>> {
            self.runs.fetch_add(1, Ordering::SeqCst);
            let refused = xmtp_api::ApiError::Api(xmtp_proto::api::NetworkError::new(
                xmtp_api_grpc::error::GrpcError::Status(self.status.clone()),
            ));
            Err(Box::new(crate::groups::commit_log::CommitLogError::Api(
                refused,
            )))
        }
    }

    /// A worker that failed with a rate limit restarts after the delay, not
    /// after the one-second restart delay.
    // verifies: RATE-041
    #[xmtp_common::test(unwrap_try = true)]
    async fn a_rate_limited_worker_restarts_after_the_delay() {
        tokio::time::pause();
        let runs = Arc::new(AtomicUsize::new(0));
        let cancel = CancellationToken::new();
        let worker = Box::new(RateLimited {
            runs: runs.clone(),
            status: xmtp_api_grpc::error::rate_limited_status("request", Duration::from_secs(300)),
        });
        let handle = tokio::spawn(worker.spawn(cancel.clone()));
        tokio::time::sleep(Duration::from_secs(10)).await;
        assert_eq!(runs.load(Ordering::SeqCst), 1);
        tokio::time::sleep(Duration::from_secs(300)).await;
        assert_eq!(runs.load(Ordering::SeqCst), 2);
        cancel.cancel();
        handle.await?;
    }

    /// A delay too large for the clock holds the restart without a panic,
    /// which a release build would turn into an abort.
    // verifies: RATE-041
    #[xmtp_common::test(unwrap_try = true)]
    async fn an_out_of_range_delay_holds_the_restart_without_panicking() {
        tokio::time::pause();
        let runs = Arc::new(AtomicUsize::new(0));
        let cancel = CancellationToken::new();
        let worker = Box::new(RateLimited {
            runs: runs.clone(),
            status: xmtp_api_grpc::error::rate_limited_status_raw(i64::MAX, 0),
        });
        let handle = tokio::spawn(worker.spawn(cancel.clone()));
        tokio::time::sleep(Duration::from_secs(365 * 24 * 60 * 60)).await;
        assert_eq!(runs.load(Ordering::SeqCst), 1);
        cancel.cancel();
        handle.await?;
    }
}
