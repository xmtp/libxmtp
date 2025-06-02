use crate::{configuration::WORKER_RESTART_DELAY, groups::device_sync::DeviceSyncError};
use std::fmt::Debug;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum WorkerError {
    #[error(transparent)]
    DeviceSync(#[from] DeviceSyncError),
}

pub struct Worker<Ctx, Core> {
    ctx: Ctx,
    core: Core,
}

impl<Ctx, Core> Worker<Ctx, Core>
where
    Core: WorkerCore + 'static,
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
