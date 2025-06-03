use crate::{
    configuration::WORKER_RESTART_DELAY,
    context::XmtpMlsLocalContext,
    groups::device_sync::{worker::SyncMetric, DeviceSyncError},
};
use metrics::WorkerMetrics;
use parking_lot::Mutex;
use std::{fmt::Debug, hash::Hash, marker::PhantomData, sync::Arc};
use thiserror::Error;
use xmtp_api::XmtpApi;
use xmtp_db::XmtpDb;

pub mod metrics;

#[derive(Error, Debug)]
pub enum WorkerError {
    #[error(transparent)]
    DeviceSync(#[from] DeviceSyncError),
}

#[derive(PartialEq, Eq, Hash)]
pub enum WorkerKind {
    DeviceSync,
}

pub(crate) trait WorkerManager: Send + Sync {
    fn sync_metrics(&self) -> Option<Arc<WorkerMetrics<SyncMetric>>>;
    fn spawn(&self);
}

impl<Core, Metric> WorkerManager for WorkerRunner<Core, Metric>
where
    Core: Send + Sync,
    Metric: Send + Sync + 'static,
{
    fn sync_metrics(&self) -> Option<Arc<WorkerMetrics<SyncMetric>>> {
        if std::any::TypeId::of::<Metric>() == std::any::TypeId::of::<SyncMetric>() {
            self.metrics.as_ref().lock().clone().map(|arc| {
                // This is safe because we verified Metric == SyncMetric
                unsafe { std::mem::transmute::<Arc<WorkerMetrics<Metric>>, Arc<WorkerMetrics<SyncMetric>>>(arc.clone()) }
            })
        } else {
            None
        }
    }

    fn spawn(&self) {
        (self.spawn_fn)();
    }
}

pub struct WorkerRunner<Core, Metric = SyncMetric> {
    pub metrics: Arc<Mutex<Option<Arc<WorkerMetrics<Metric>>>>>,
    spawn_fn: Box<dyn Fn() + Send + Sync>,
    _core: PhantomData<Core>,
}

impl<Core, Metric> WorkerRunner<Core, Metric>
where
    Metric: PartialEq + Hash + Send + Sync + 'static,
{
    pub fn register_new_worker<ApiClient, Db>(context: &Arc<XmtpMlsLocalContext<ApiClient, Db>>)
    where
        ApiClient: XmtpApi + Send + Sync + 'static,
        Db: XmtpDb + Send + Sync + 'static,
        Core: Worker<ApiClient, Db, Metric> + 'static,
    {
        let metrics = Arc::new(Mutex::default());
        let runner = WorkerRunner {
            metrics: metrics.clone(),
            spawn_fn: Box::new({
                let context = context.clone();
                let metrics = metrics.clone();
                move || {
                    Self::spawn_worker_internal(&context, &metrics);
                }
            }),
            _core: PhantomData::<Core>,
        };

        runner.spawn();
        let kind = Core::kind();
        let runner = Box::new(runner);
        context.workers.lock().insert(kind, runner as Box<_>);
    }

    pub(crate) fn spawn_worker_internal<ApiClient, Db>(
        context: &Arc<XmtpMlsLocalContext<ApiClient, Db>>,
        metrics: &Arc<Mutex<Option<Arc<WorkerMetrics<Metric>>>>>,
    ) where
        ApiClient: XmtpApi + Send + Sync + 'static,
        Db: XmtpDb + Send + Sync + 'static,
        Core: Worker<ApiClient, Db, Metric> + 'static,
    {
        let mut core = Core::init(context);
        *metrics.lock() = core.metrics();

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
    }
}

#[async_trait::async_trait]
pub trait Worker<ApiClient, Db, Metric>
where
    Self: Send + Sync,
    ApiClient: XmtpApi + Send + Sync + 'static,
    Db: xmtp_db::XmtpDb + Send + Sync + 'static,
    Metric: PartialEq + Hash + Send + Sync + 'static,
{
    type Error: NeedsDbReconnect + Debug + Send;

    fn kind() -> WorkerKind;
    fn init(context: &Arc<XmtpMlsLocalContext<ApiClient, Db>>) -> Self;
    fn metrics(&self) -> Option<Arc<WorkerMetrics<Metric>>>;
    async fn run_tasks(&mut self) -> Result<(), Self::Error>;
}

pub trait NeedsDbReconnect {
    fn needs_db_reconnect(&self) -> bool;
}
