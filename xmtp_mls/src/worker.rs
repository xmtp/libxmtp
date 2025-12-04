use crate::tasks::TaskWorkerChannels;
use crate::{context::XmtpSharedContext, groups::device_sync::worker::SyncMetric};
use futures::{StreamExt, stream::FuturesUnordered};
use metrics::WorkerMetrics;
use parking_lot::Mutex;
use std::fmt::Debug;
use std::pin::Pin;
use std::{any::Any, collections::HashMap, hash::Hash, sync::Arc};
use xmtp_common::{MaybeSend, MaybeSync, StreamHandle, if_native, if_wasm, time::Duration};
use xmtp_configuration::WORKER_RESTART_DELAY;

pub mod metrics;

#[derive(PartialEq, Eq, Copy, Clone, Hash, Debug)]
pub enum WorkerKind {
    DeviceSync,
    DisappearingMessages,
    KeyPackageCleaner,
    Event,
    CommitLog,
    TaskRunner,
    PendingSelfRemove,
}

pub struct WorkerRunner {
    // When this is cloned into the Context this is empty, so the Context and Client have different views
    factories: Vec<DynFactory>,
    metrics: Arc<Mutex<HashMap<WorkerKind, DynMetrics>>>,
    task_channels: TaskWorkerChannels,
    handle: Mutex<Option<Box<dyn StreamHandle<StreamOutput = ()>>>>,
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

        let this = self.clone();
        let handle = xmtp_common::spawn(None, async move {
            while !ctx.identity().is_ready() {
                xmtp_common::time::sleep(Duration::from_millis(50)).await;
            }

            let mut futs = FuturesUnordered::new();

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
                futs.push(worker.spawn());
            }

            while let Some(kind) = futs.next().await {
                tracing::warn!("Worker {kind:?} completed unexpectedly")
            }
        });

        *handle_lock = Some(Box::new(handle));
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

    fn spawn(mut self: Box<Self>) -> Pin<Box<SpawnWorkerFut>> {
        let fut = async move {
            loop {
                if let Err(err) = self.run_tasks().await {
                    if err.needs_db_reconnect() {
                        // drop the worker
                        tracing::warn!("Pool disconnected. task will restart on reconnect");
                        break;
                    } else {
                        tracing::error!("{:?} worker error: {:?}", self.kind(), err);
                        xmtp_common::time::sleep(WORKER_RESTART_DELAY).await;
                        tracing::info!("Restarting {:?} worker...", self.kind());
                    }
                }
            }

            self.kind()
        };

        Box::pin(fut)
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
