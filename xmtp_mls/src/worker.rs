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

pub(crate) trait WorkerManager: Send + Sync {
    fn sync_metrics(&self) -> Option<Arc<WorkerMetrics<SyncMetric>>>;
    fn spawn(&self) -> WorkerKind;
}

impl<W> WorkerManager for WorkerRunner<W>
where
    W: Worker + Send + Sync,
    <W as Worker>::Error: Send,
{
    fn sync_metrics(&self) -> Option<Arc<WorkerMetrics<SyncMetric>>> {
        self.metrics.lock().clone().and_then(|m| m.downcast().ok())
    }

    fn spawn(&self) -> WorkerKind {
        let mut worker = (self.create_fn)();
        *self.metrics.lock() = worker.metrics();
        let kind = worker.kind();

        tokio::task::spawn_local({
            async move {
                while let Err(err) = worker.run_tasks().await {
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
    create_fn: Box<dyn Fn() -> W + Send + Sync>,
    _worker: PhantomData<W>,
}

impl<W> WorkerRunner<W>
where
    W: Worker + Send + Sync,
    <W as Worker>::Error: Send,
{
    pub fn register_new_worker<ApiClient, Db, F>(
        context: &Arc<XmtpMlsLocalContext<ApiClient, Db>>,
        create_fn: F,
    ) where
        F: Fn() -> W + Send + Sync + 'static,
        W: Worker + Send + Sync,
    {
        let metrics = Arc::new(Mutex::default());
        let runner = Box::new(WorkerRunner {
            metrics: metrics.clone(),
            create_fn: Box::new(create_fn),
            _worker: PhantomData::<W>,
        });

        let kind = runner.spawn();

        context.workers.lock().insert(kind, runner as Box<_>);
    }
}

#[async_trait::async_trait]
pub trait Worker: 'static {
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
