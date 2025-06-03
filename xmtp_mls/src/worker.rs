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

impl<Core> WorkerManager for WorkerRunner<Core>
where
    Core: Worker + Send + Sync + 'static,
{
    fn sync_metrics(&self) -> Option<Arc<WorkerMetrics<SyncMetric>>> {
        self.metrics.lock().clone().and_then(|m| m.downcast().ok())
    }

    fn spawn(&self) -> WorkerKind {
        let mut core = (self.create_fn)();
        *self.metrics.lock() = core.metrics().map(|a| a as Arc<_>);
        let kind = core.kind();

        xmtp_common::spawn(None, async move {
            loop {
                if let Err(err) = core.run_tasks().await {
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

pub struct WorkerRunner<Core> {
    pub metrics: Arc<Mutex<Option<Arc<dyn Any + Send + Sync>>>>,
    create_fn: Box<dyn Fn() -> Core + Send + Sync>,
    _core: PhantomData<Core>,
}

impl<Core> WorkerRunner<Core> {
    pub fn register_new_worker<ApiClient, Db, F>(
        context: &Arc<XmtpMlsLocalContext<ApiClient, Db>>,
        create_fn: F,
    ) where
        F: Fn() -> Core + Send + Sync + 'static,
        Core: Worker + 'static,
    {
        let create_fn = Box::new(create_fn);

        let metrics = Arc::new(Mutex::default());
        let runner = Box::new(WorkerRunner {
            metrics: metrics.clone(),
            create_fn,
            _core: PhantomData::<Core>,
        });

        let kind = runner.spawn();

        context.workers.lock().insert(kind, runner as Box<_>);
    }
}

#[async_trait::async_trait]
pub trait Worker
where
    Self: Send + Sync,
{
    type Error: NeedsDbReconnect + Debug + Send;

    fn kind(&self) -> WorkerKind;
    async fn run_tasks(&mut self) -> Result<(), Self::Error>;
    fn metrics(&self) -> Option<Arc<dyn Any + Send + Sync>> {
        None
    }
}

pub trait NeedsDbReconnect {
    fn needs_db_reconnect(&self) -> bool;
}
