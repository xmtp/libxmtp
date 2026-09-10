mod fetch;
mod keepalive;
mod output;
pub(crate) use output::StreamEnd;
mod registry;
mod session;
mod tailer;
#[cfg(test)]
mod tests;

use crate::{config::Config, error::Error};
pub(crate) use registry::Registry;
pub(crate) use session::{native, static_subscription};
use sqlx::PgPool;
use std::sync::Arc;
use tokio::{sync::Semaphore, task::JoinHandle};

const FETCH_TOPICS: usize = 256;
const FETCH_ROWS: i64 = 64;
pub(crate) const FETCH_WORKERS: usize = 8;
const OUTBOUND_FRAMES: usize = 64;
const ENVELOPE_OVERHEAD: usize = crate::config::ENVELOPE_METADATA_AND_FRAMING_BYTES;

pub(crate) struct StreamHub {
    pub registry: Arc<Registry>,
    pub read: PgPool,
    pub fetches: Arc<Semaphore>,
    worker: JoinHandle<()>,
}

impl StreamHub {
    /// Start recovery before accepting sessions. Workers share only disposable state;
    /// database errors prevent readiness until a fresh boundary is visible.
    pub async fn start(primary: PgPool, read: PgPool, config: &Config) -> Result<Arc<Self>, Error> {
        let registry = Arc::new(Registry::default());
        let worker = tailer::start(primary, read.clone(), registry.clone(), config).await?;
        Ok(Arc::new(Self {
            registry,
            read,
            fetches: Arc::new(Semaphore::new(FETCH_WORKERS)),
            worker,
        }))
    }

    /// Acquire one shared fetch worker and measure its queue wait.
    pub(super) async fn fetch_permit(
        &self,
    ) -> Result<tokio::sync::OwnedSemaphorePermit, tonic::Status> {
        let wait = crate::telemetry::FetchWait::start();
        let result = self
            .fetches
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| tonic::Status::unavailable("fetch service stopped"));
        drop(wait);
        result
    }

    /// Fail sessions before aborting workers, so no client can mistake shutdown for
    /// completed history. Unary drain remains the transport's responsibility.
    pub fn stop(&self) {
        self.registry.fail_all(
            tonic::Status::unavailable("stream service stopped"),
            StreamEnd::Shutdown,
        );
        self.worker.abort();
    }
}

impl Drop for StreamHub {
    fn drop(&mut self) {
        self.worker.abort();
    }
}
