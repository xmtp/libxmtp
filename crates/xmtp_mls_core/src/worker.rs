pub mod disappearing_messages;
pub use crate::hmac_epoch;
pub mod metrics;
pub mod tasks;

use crate::context::CoreContext;
use crate::events::InternalEvent;
use crate::metrics::SyncMetric;
pub use crate::reconnect::{LocalNeedsDbReconnect, NeedsDbReconnect};
use crate::task_channels::TaskWorkerChannels;
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

/// Worker runner controls for tests. Each assumes process-per-test isolation.
#[cfg(any(test, feature = "test-hooks"))]
#[doc(hidden)]
pub mod test_hooks {
    use std::sync::Arc;
    use tokio::sync::Notify;

    type PauseHook = (Vec<u8>, Arc<Notify>, Arc<Notify>);
    static PAUSE_NEXT_SPAWN: parking_lot::Mutex<Option<PauseHook>> = parking_lot::Mutex::new(None);

    /// Pauses the next worker spawn of `installation_id` once its identity is
    /// ready and before any worker starts: notifies `entered`, then waits for
    /// `release`.
    pub fn pause_next_spawn(installation_id: Vec<u8>, entered: Arc<Notify>, release: Arc<Notify>) {
        *PAUSE_NEXT_SPAWN.lock() = Some((installation_id, entered, release));
    }

    pub(super) fn take_spawn_pause(installation_id: &[u8]) -> Option<(Arc<Notify>, Arc<Notify>)> {
        let mut hook = PAUSE_NEXT_SPAWN.lock();
        if hook
            .as_ref()
            .is_some_and(|(installation, _, _)| installation.as_slice() == installation_id)
        {
            hook.take().map(|(_, entered, release)| (entered, release))
        } else {
            None
        }
    }
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
/// Durations are nanoseconds, matching [`crate::config::ForkRecoveryOpts`].
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
    #[cfg(any(test, feature = "test-utils"))]
    pub fn registered_kinds(&self) -> Vec<WorkerKind> {
        self.factories.iter().map(|f| f.kind()).collect()
    }

    /// The kinds of every worker that subscribed to events at registration.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn subscribed_kinds(&self) -> Vec<WorkerKind> {
        self.subscriptions.keys().copied().collect()
    }

    /// The event subscription the worker of `kind` receives, if it has one.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn subscription_for_test(
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

                #[cfg(any(test, feature = "test-hooks"))]
                {
                    let paused = test_hooks::take_spawn_pause(&ctx.installation_id().to_vec());
                    if let Some((entered, release)) = paused {
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

    async fn run_tasks(&mut self) -> WorkerResult<()>;

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
