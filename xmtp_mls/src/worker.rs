use crate::{configuration::WORKER_RESTART_DELAY, groups::device_sync::DeviceSyncError};
use metrics::WorkerMetrics;
use std::{fmt::Debug, hash::Hash, sync::Arc};
use thiserror::Error;

pub mod metrics;

#[derive(Error, Debug)]
pub enum WorkerError {
    #[error(transparent)]
    DeviceSync(#[from] DeviceSyncError),
}

pub enum WorkerKind {
    DeviceSync,
    MessageDeletion,
}

pub struct Worker<Core, Metric>
where
    Metric: PartialEq + Hash,
{
    core: Core,
    metrics: Arc<WorkerMetrics<Metric>>,
}

impl<Core, Metric> Worker<Core, Metric>
where
    Core: WorkerCore + 'static,
    Metric: PartialEq + Hash,
{
    pub fn spawn(mut self) {
        xmtp_common::spawn(None, async move {
            loop {
                if let Err(err) = self.core.run().await {
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
pub trait WorkerCore
where
    Self: Send,
{
    const NAME: &str;
    type Error: NeedsDbReconnect + Debug + Send;

    async fn run(&mut self) -> Result<(), Self::Error>;
}

pub trait NeedsDbReconnect {
    fn needs_db_reconnect(&self) -> bool;
}
