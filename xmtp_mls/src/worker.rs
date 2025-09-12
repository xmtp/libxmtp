use crate::{context::XmtpSharedContext, groups::device_sync::worker::SyncMetric};
use metrics::WorkerMetrics;
use parking_lot::Mutex;
use std::fmt::Debug;
use std::{any::Any, collections::HashMap, hash::Hash, sync::Arc};
use tokio_util::sync::CancellationToken;
use xmtp_configuration::WORKER_RESTART_DELAY;

pub mod metrics;

#[derive(PartialEq, Eq, Copy, Clone, Hash, Debug)]
pub enum WorkerKind {
    DeviceSync,
    DisappearingMessages,
    KeyPackageCleaner,
    Event,
    CommitLog,
}

#[derive(Clone)]
pub struct WorkerRunner {
    factories: Vec<DynFactory>,
    metrics: Arc<Mutex<HashMap<WorkerKind, DynMetrics>>>,
    cancellation_token: CancellationToken,
}

impl Default for WorkerRunner {
    fn default() -> Self {
        Self {
            factories: Vec::new(),
            metrics: Arc::new(Mutex::new(HashMap::new())),
            cancellation_token: CancellationToken::new(),
        }
    }
}

impl WorkerRunner {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn sync_metrics(&self) -> Option<Arc<WorkerMetrics<SyncMetric>>> {
        self.metrics
            .lock()
            .get(&WorkerKind::DeviceSync)?
            .as_sync_metrics()
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

    pub fn spawn(&self) {
        for factory in &self.factories {
            let metric = self.metrics.lock().get(&factory.kind()).cloned();
            let (worker, metrics) = factory.create(metric);

            if let Some(metrics) = metrics {
                self.metrics.lock().insert(factory.kind(), metrics);
            }

            if let Some(metrics) = worker.metrics() {
                let mut m = self.metrics.lock();
                m.insert(worker.kind(), metrics);
            }
            let token = self.cancellation_token.child_token();
            worker.spawn(token)
        }
    }

    /// Cancel all workers
    pub fn cancel_all(&self) {
        self.cancellation_token.cancel();
    }

    /// Check if workers have been cancelled
    pub fn is_cancelled(&self) -> bool {
        self.cancellation_token.is_cancelled()
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

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait Worker {
    fn kind(&self) -> WorkerKind;

    async fn run_tasks(&mut self) -> Result<(), Box<dyn NeedsDbReconnect>>;

    fn metrics(&self) -> Option<Arc<dyn Any + Send + Sync>> {
        None
    }

    fn factory<C>(context: C) -> impl WorkerFactory + 'static
    where
        Self: Sized,
        C: XmtpSharedContext + Send + Sync + 'static;

    /// Box the worker, erasing its type
    fn boxed(self) -> Box<dyn Worker>
    where
        Self: Sized + 'static,
    {
        Box::new(self) as Box<_>
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn spawn(mut self: Box<Self>, cancellation_token: CancellationToken)
    where
        Self: Send + Sync + 'static,
    {
        xmtp_common::spawn(None, async move {
            'outer: loop {
                tokio::select! {
                    _ = cancellation_token.cancelled() => {
                        tracing::info!("{:?} worker received cancellation signal", self.kind());
                        break 'outer;
                    }
                    result = self.run_tasks() => {
                        if let Err(err) = result {
                            if err.needs_db_reconnect() {
                                // drop the worker
                                tracing::debug!("{:?} worker stopping: database disconnected", self.kind());
                                break 'outer;
                            } else {
                                tracing::error!("{:?} worker error: {:?}", self.kind(), err);
                                // Sleep with cancellation support
                                tokio::select! {
                                    _ = cancellation_token.cancelled() => {
                                        tracing::info!("{:?} worker received cancellation signal during restart delay", self.kind());
                                        break 'outer;
                                    }
                                    _ = xmtp_common::time::sleep(WORKER_RESTART_DELAY) => {
                                        tracing::info!("Restarting {:?} worker...", self.kind());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });
    }

    #[cfg(target_arch = "wasm32")]
    fn spawn(mut self: Box<Self>, cancellation_token: CancellationToken)
    where
        Self: 'static,
    {
        xmtp_common::spawn(None, async move {
            'outer: loop {
                tokio::select! {
                    _ = cancellation_token.cancelled() => {
                        tracing::info!("{:?} worker received cancellation signal", self.kind());
                        break 'outer;
                    }
                    result = self.run_tasks() => {
                        if let Err(err) = result {
                            if err.needs_db_reconnect() {
                                // drop the worker
                                tracing::debug!("{:?} worker stopping: database disconnected", self.kind());
                                break 'outer;
                            } else {
                                tracing::error!("Worker error: {err:?}");
                                // Sleep with cancellation support
                                tokio::select! {
                                    _ = cancellation_token.cancelled() => {
                                        tracing::info!("{:?} worker received cancellation signal during restart delay", self.kind());
                                        break 'outer;
                                    }
                                    _ = xmtp_common::time::sleep(WORKER_RESTART_DELAY) => {
                                        tracing::info!("Restarting {:?} worker...", self.kind());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });
    }
}

#[cfg_attr(not(target_arch = "wasm32"), trait_variant::make(NeedsDbReconnect: Send + Sync))]
#[cfg_attr(target_arch = "wasm32", trait_variant::make(NeedsDbReconnect: xmtp_common::Wasm))]
pub trait LocalNeedsDbReconnect: std::error::Error {
    fn needs_db_reconnect(&self) -> bool;
}

// #[cfg_attr(not(target_arch = "wasm32"), trait_variant::make(WorkerFactory: Send + Sync))]
// #[cfg_attr(target_arch = "wasm32", trait_variant::make(WorkerFactory: xmtp_common::Wasm))]
pub trait WorkerFactory: Send + Sync {
    fn kind(&self) -> WorkerKind;
    /// Create a new worker
    fn create(&self, metrics: Option<DynMetrics>) -> (BoxedWorker, Option<DynMetrics>);
}

#[cfg(target_arch = "wasm32")]
pub type BoxedWorker = Box<dyn Worker>;
#[cfg(not(target_arch = "wasm32"))]
pub type BoxedWorker = Box<dyn Worker + Send + Sync>;

pub type DynMetrics = Arc<dyn Any + Send + Sync>;

pub trait MetricsCasting {
    fn as_sync_metrics(&self) -> Option<Arc<WorkerMetrics<SyncMetric>>>;
}

impl MetricsCasting for DynMetrics {
    fn as_sync_metrics(&self) -> Option<Arc<WorkerMetrics<SyncMetric>>> {
        self.clone().downcast().ok()
    }
}

#[cfg(target_arch = "wasm32")]
pub type DynFactory = Arc<dyn WorkerFactory>;
#[cfg(not(target_arch = "wasm32"))]
pub type DynFactory = Arc<dyn WorkerFactory + Send + Sync>;
