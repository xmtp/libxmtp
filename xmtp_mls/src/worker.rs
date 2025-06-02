use crate::{
    configuration::WORKER_RESTART_DELAY,
    context::XmtpMlsLocalContext,
    groups::device_sync::{
        worker::{SyncMetric, SyncWorker},
        DeviceSyncError,
    },
};
use metrics::WorkerMetrics;
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

pub enum WorkerRunners<ApiClient, Db> {
    DeviceSync(WorkerRunner<SyncWorker<ApiClient, Db>, SyncMetric>),
}

pub struct WorkerRunner<Core, Metric> {
    pub metrics: Arc<WorkerMetrics<Metric>>,
    pub core: Core,
}

impl<Core, Metric> WorkerRunner<Core, Metric>
where
    Metric: PartialEq + Hash,
{
    fn spawn<ApiClient, Db, F>(context: &Arc<XmtpMlsLocalContext<ApiClient, Db>>, core_provider: F)
    where
        ApiClient: XmtpApi + Send + Sync + 'static,
        Db: XmtpDb + Send + Sync + 'static,
        Core: Worker<ApiClient, Db> + 'static,
        F: Fn(&Arc<XmtpMlsLocalContext<ApiClient, Db>>) -> Core + Send + 'static,
    {
        let mut core = core_provider(context);

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
pub trait Worker<ApiClient, Db>
where
    Self: Send,
    ApiClient: XmtpApi + Send + Sync + 'static,
    Db: xmtp_db::XmtpDb + Send + Sync + 'static,
{
    type Error: NeedsDbReconnect + Debug + Send;

    fn init(context: Arc<XmtpMlsLocalContext<ApiClient, Db>>) -> Self;

    async fn run_tasks(&mut self) -> Result<(), Self::Error>;
}

pub trait NeedsDbReconnect {
    fn needs_db_reconnect(&self) -> bool;
}
