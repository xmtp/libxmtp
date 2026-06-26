pub mod device_sync;
pub mod disappearing_messages;
pub mod key_package_cleaner;
pub mod metrics;
pub mod rearm_channel;
pub mod tasks;

use crate::context::XmtpSharedContext;
use device_sync::worker::SyncMetric;
use futures::future::{AbortHandle, Abortable};
use futures::{StreamExt, stream::FuturesUnordered};
use metrics::WorkerMetrics;
use parking_lot::Mutex;
use std::fmt::Debug;
use std::pin::Pin;
use std::{any::Any, collections::HashMap, hash::Hash, sync::Arc};
use tasks::TaskWorkerChannels;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use tracing::instrument::Instrumented;
use xmtp_common::{MaybeSend, MaybeSync, StreamHandle, if_native, if_wasm, time::Duration};
use xmtp_configuration::WORKER_RESTART_DELAY;

/// Hard cap on how long `WorkerRunner::shutdown` waits for the supervisor
/// task to drain after cancellation. Anything beyond this gets logged and
/// the task is allowed to detach — keeps `Client::close` bounded.
const WORKER_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(PartialEq, Eq, Copy, Clone, Hash, Debug)]
pub enum WorkerKind {
    DeviceSync,
    DisappearingMessages,
    KeyPackageCleaner,
    CommitLog,
    TaskRunner,
}

/// Configuration for the cadence and enablement of background workers.
///
/// `Default` (all-`None`, empty maps) reproduces the historical behavior:
/// every worker enabled, each using its own compiled-in interval const, no
/// jitter. All fields are opt-in so existing callers are unaffected.
///
/// Durations are nanoseconds, matching [`crate::builder::ForkRecoveryOpts`].
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

    /// A test-only `WorkerConfig` with a fast `KeyPackageCleaner` interval so
    /// timing-dependent tests don't wait the production 1-hour coarse cadence.
    ///
    /// 100 ms is fast enough for tests that sleep ≤11 s but slow enough that it
    /// won't busy-loop under a normal test run.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn for_testing() -> Self {
        let mut cfg = Self::default();
        cfg.interval_overrides.insert(
            WorkerKind::KeyPackageCleaner,
            std::time::Duration::from_millis(100).as_nanos() as u64,
        );
        cfg
    }
}

pub struct WorkerRunner {
    // When this is cloned into the Context this is empty, so the Context and Client have different views
    factories: Vec<DynFactory>,
    metrics: Arc<Mutex<HashMap<WorkerKind, DynMetrics>>>,
    task_channels: TaskWorkerChannels,
    handle: Mutex<Option<Box<dyn StreamHandle<StreamOutput = ()>>>>,
    // Per-worker abort handles. `shutdown` calls `abort()` on each so the
    // worker future is dropped at its next poll regardless of whether the
    // outer loop observed the cancellation token. Belt to the token's
    // suspenders for workers that don't yield to cancellation promptly.
    abort_handles: Mutex<Vec<AbortHandle>>,
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
            metrics: Arc::default(),
            task_channels: TaskWorkerChannels::default(),
            handle: Mutex::default(),
            abort_handles: Mutex::default(),
        }
    }

    pub fn metrics(&self) -> &Arc<Mutex<HashMap<WorkerKind, DynMetrics>>> {
        &self.metrics
    }

    pub fn sync_metrics(&self) -> Option<Arc<WorkerMetrics<SyncMetric>>> {
        self.metrics
            .lock()
            .get(&WorkerKind::DeviceSync)?
            .as_sync_metrics()
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
}

impl WorkerRunner {
    pub fn register_new_worker<W: Worker, C>(&mut self, ctx: C)
    where
        C: XmtpSharedContext + 'static,
    {
        let factory = W::factory(ctx);
        self.factories.push(Arc::new(factory))
    }

    pub fn spawn<C>(self: &Arc<Self>, ctx: C)
    where
        C: XmtpSharedContext + 'static,
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
                    xmtp_common::time::sleep(Duration::from_millis(50)).await;
                }

                let mut futs = FuturesUnordered::new();
                let mut new_handles = Vec::with_capacity(this.factories.len());

                for factory in &this.factories {
                    let metric = this.metrics.lock().get(&factory.kind()).cloned();
                    let (worker, metrics) = factory.create(metric);

                    if let Some(metrics) = metrics {
                        this.metrics.lock().insert(factory.kind(), metrics);
                    }

                    if let Some(metrics) = worker.metrics() {
                        let mut m = this.metrics.lock();
                        m.insert(worker.kind(), metrics);
                    }
                    let (abort_handle, reg) = AbortHandle::new_pair();
                    new_handles.push(abort_handle);
                    futs.push(Abortable::new(worker.spawn(cancel.clone()), reg));
                }
                *this.abort_handles.lock() = new_handles;

                while let Some(outcome) = futs.next().await {
                    match outcome {
                        Ok(kind) => tracing::warn!("Worker {kind:?} completed unexpectedly"),
                        Err(_aborted) => tracing::debug!("worker aborted during shutdown"),
                    }
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
    }

    pub async fn wait_for_sync_worker_init(&self) {
        let handle = self
            .metrics
            .lock()
            .get(&WorkerKind::DeviceSync)
            .cloned()
            .and_then(|h| h.as_sync_metrics());
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

    async fn run_tasks(&mut self) -> Result<(), Box<dyn NeedsDbReconnect>>;

    fn metrics(&self) -> Option<DynMetrics> {
        None
    }

    fn factory<C>(context: C) -> impl WorkerFactory + 'static
    where
        Self: Sized,
        C: XmtpSharedContext + 'static;

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
                    let outcome = async { self.run_tasks().await }
                        .instrument(tracing::info_span!(
                            "worker_run",
                            operation = "worker_run",
                            worker = %worker
                        ))
                        .await;
                    if let Err(err) = outcome {
                        if err.needs_db_reconnect() {
                            // drop the worker
                            tracing::debug!("pool disconnected. task will restart on reconnect");
                            break;
                        } else {
                            tracing::error!("{:?} worker error: {}", kind, err);
                            xmtp_common::time::sleep(WORKER_RESTART_DELAY).await;
                            tracing::info!("Restarting {:?} worker...", kind);
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

#[cfg_attr(not(target_arch = "wasm32"), trait_variant::make(NeedsDbReconnect: Send + Sync))]
#[cfg_attr(target_arch = "wasm32", trait_variant::make(NeedsDbReconnect: xmtp_common::Wasm))]
pub trait LocalNeedsDbReconnect: std::error::Error {
    fn needs_db_reconnect(&self) -> bool;
}

pub trait WorkerFactory: MaybeSend + MaybeSync {
    fn kind(&self) -> WorkerKind;
    /// Create a new worker
    fn create(&self, metrics: Option<DynMetrics>) -> (BoxedWorker, Option<DynMetrics>);
}

pub type BoxedWorker = Box<dyn Worker>;
pub type DynFactory = Arc<dyn WorkerFactory>;

pub type DynMetrics = Arc<dyn Any + Send + Sync>;

pub trait MetricsCasting {
    fn as_sync_metrics(&self) -> Option<Arc<WorkerMetrics<SyncMetric>>>;
}

impl MetricsCasting for DynMetrics {
    fn as_sync_metrics(&self) -> Option<Arc<WorkerMetrics<SyncMetric>>> {
        self.clone().downcast().ok()
    }
}

// Native-only: `db_needs_connection` is always `false` on wasm, so the contract
// these tests assert only has teeth on native targets.
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
    use crate::worker::key_package_cleaner::KeyPackagesCleanerError;
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
        assert!(GroupError::Storage(disconnect_storage()).needs_db_reconnect());
        assert!(GroupError::Db(disconnect_connection()).needs_db_reconnect());
        assert!(
            GroupError::MlsStore(MlsStoreError::Connection(disconnect_connection()))
                .needs_db_reconnect()
        );
        // A non-disconnect storage failure inside a GroupError must not stop the worker.
        assert!(!GroupError::Storage(benign_storage()).needs_db_reconnect());
        assert!(!GroupError::InvalidGroupMembership.needs_db_reconnect());
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
        use crate::worker::tasks::TaskWorkerError;
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
        assert!(
            DeviceSyncError::Subscribe(SubscribeError::Db(disconnect_connection()))
                .needs_db_reconnect()
        );
        assert!(!DeviceSyncError::Storage(benign_storage()).needs_db_reconnect());
        assert!(!DeviceSyncError::InvalidPayload.needs_db_reconnect());
    }

    #[xmtp_common::test]
    fn key_package_cleaner_error_forwards_disconnect() {
        use crate::identity::IdentityError;
        assert!(KeyPackagesCleanerError::Storage(disconnect_storage()).needs_db_reconnect());
        // Per-key-package delete returns an IdentityError; a disconnect must
        // bubble whether it arrives as a StorageError or a bare ConnectionError.
        assert!(
            KeyPackagesCleanerError::Identity(IdentityError::StorageError(disconnect_storage()))
                .needs_db_reconnect()
        );
        assert!(
            KeyPackagesCleanerError::Identity(IdentityError::Db(disconnect_connection()))
                .needs_db_reconnect()
        );
        assert!(!KeyPackagesCleanerError::Storage(benign_storage()).needs_db_reconnect());
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
