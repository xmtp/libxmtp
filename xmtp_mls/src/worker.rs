use crate::{
    configuration::WORKER_RESTART_DELAY, context::XmtpMlsLocalContext,
    groups::device_sync::worker::SyncMetric,
};
use metrics::WorkerMetrics;
use parking_lot::Mutex;
use std::{any::Any, fmt::Debug, hash::Hash, marker::PhantomData, sync::Arc};

pub mod metrics;

#[derive(PartialEq, Eq, Hash)]
pub enum WorkerKind {
    DeviceSync,
    DisappearingMessages,
    KeyPackageCleaner,
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) trait WorkerManager: Send + Sync {
    fn sync_metrics(&self) -> Option<Arc<WorkerMetrics<SyncMetric>>>;
    fn spawn(&self) -> WorkerKind;
}

#[cfg(target_arch = "wasm32")]
pub(crate) trait WorkerManager {
    fn sync_metrics(&self) -> Option<Arc<WorkerMetrics<SyncMetric>>>;
    fn spawn(&self) -> WorkerKind;
}

#[cfg(not(target_arch = "wasm32"))]
impl<W> WorkerManager for WorkerRunner<W>
where
    W: Worker + Send + Sync + 'static,
    <W as Worker>::Error: Send,
{
    fn sync_metrics(&self) -> Option<Arc<WorkerMetrics<SyncMetric>>> {
        self.metrics.lock().clone().and_then(|m| m.downcast().ok())
    }

    fn spawn(&self) -> WorkerKind {
        let mut worker = (self.create_fn)();
        *self.metrics.lock() = worker.metrics().map(|a| a as Arc<_>);
        let kind = worker.kind();

        xmtp_common::spawn(None, async move {
            loop {
                if let Err(err) = worker.run_tasks().await {
                    if err.needs_db_reconnect() {
                        tracing::warn!("Pool disconnected. task will restart on reconnect");
                        break;
                    } else {
                        tracing::error!("Worker error: {err:?}");
                        xmtp_common::time::sleep(WORKER_RESTART_DELAY).await;
                        tracing::info!("Restarting sync worker...");
                    }
                }
            }
        });

        kind
    }
}

#[cfg(target_arch = "wasm32")]
impl<W> WorkerManager for WorkerRunner<W>
where
    W: Worker + 'static,
{
    fn sync_metrics(&self) -> Option<Arc<WorkerMetrics<SyncMetric>>> {
        self.metrics.lock().clone().and_then(|m| m.downcast().ok())
    }

    fn spawn(&self) -> WorkerKind {
        let mut worker = (self.create_fn)();
        *self.metrics.lock() = worker.metrics().map(|a| a as Arc<_>);
        let kind = worker.kind();

        xmtp_common::spawn(None, async move {
            loop {
                if let Err(err) = worker.run_tasks().await {
                    if err.needs_db_reconnect() {
                        tracing::warn!("Pool disconnected. task will restart on reconnect");
                        break;
                    } else {
                        tracing::error!("Worker error: {err:?}");
                        xmtp_common::time::sleep(WORKER_RESTART_DELAY).await;
                        tracing::info!("Restarting sync worker...");
                    }
                }
            }
        });

        kind
    }
}

pub struct WorkerRunner<W> {
    pub metrics: Arc<Mutex<Option<Arc<dyn Any + Send + Sync>>>>,
    #[cfg(not(target_arch = "wasm32"))]
    create_fn: Box<dyn Fn() -> W + Send + Sync>,
    #[cfg(target_arch = "wasm32")]
    create_fn: Box<dyn Fn() -> W>,
    _worker: PhantomData<W>,
}

#[cfg(not(target_arch = "wasm32"))]
impl<W> WorkerRunner<W>
where
    W: Worker + Send + Sync + 'static,
    <W as Worker>::Error: Send,
{
    pub fn register_new_worker<ApiClient, Db, F>(
        context: &Arc<XmtpMlsLocalContext<ApiClient, Db>>,
        create_fn: F,
    ) where
        F: Fn() -> W + Send + Sync + 'static,
        W: Worker + 'static,
    {
        let create_fn = Box::new(create_fn);

        let metrics = Arc::new(Mutex::default());
        let runner = Box::new(WorkerRunner {
            metrics: metrics.clone(),
            create_fn,
            _worker: PhantomData::<W>,
        });

        let kind = runner.spawn();

        context.workers.lock().insert(kind, runner as Box<_>);
    }
}

#[cfg(target_arch = "wasm32")]
impl<W> WorkerRunner<W>
where
    W: Worker + 'static,
{
    pub fn register_new_worker<ApiClient, Db, F>(
        context: &Arc<XmtpMlsLocalContext<ApiClient, Db>>,
        create_fn: F,
    ) where
        F: Fn() -> W + 'static,
        W: Worker + 'static,
    {
        let create_fn = Box::new(create_fn);

        let metrics = Arc::new(Mutex::default());
        let runner = Box::new(WorkerRunner {
            metrics: metrics.clone(),
            create_fn,
            _worker: PhantomData::<W>,
        });

        let kind = runner.spawn();

        context.workers.lock().insert(kind, runner as Box<_>);
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait Worker {
    type Error: NeedsDbReconnect + Debug;

    fn kind(&self) -> WorkerKind;
    async fn run_tasks(&mut self) -> Result<(), Self::Error>;
    fn metrics(&self) -> Option<Arc<dyn Any + Send + Sync>> {
        None
    }
}

pub trait NeedsDbReconnect {
    fn needs_db_reconnect(&self) -> bool;
}
