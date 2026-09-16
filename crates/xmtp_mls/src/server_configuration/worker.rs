//! The hourly refresh (CFG-046 to CFG-050).
//!
//! A run rewrites the stored row. It never changes the snapshot the client is
//! holding — that is fixed at build (CFG-030) — so the only thing a refresh can
//! change about a running client is to latch a failure it must not ignore: a
//! different deployment answering (CFG-051) or a minimum version this build no
//! longer meets (CFG-061).

use xmtp_common::{RetryableError, time::Duration};
use xmtp_configuration::{
    CONFIGURATION_REFRESH_ATTEMPTS, CONFIGURATION_REFRESH_BACKOFF, CONFIGURATION_REFRESH_INTERVAL,
    CONFIGURATION_REFRESH_JITTER,
};

use crate::client::ClientError;
use crate::context::XmtpSharedContext;
use crate::worker::{
    BoxedWorker, NeedsDbReconnect, Worker, WorkerFactory, WorkerKind, WorkerResult,
};

use super::{ConfigurationLatch, check_minimum_version, fetch_and_store};

#[derive(Clone)]
pub struct Factory<Context> {
    context: Context,
}

impl<Context> WorkerFactory for Factory<Context>
where
    Context: XmtpSharedContext + 'static,
{
    fn kind(&self) -> WorkerKind {
        WorkerKind::ConfigurationRefresh
    }

    fn create(
        &self,
        metrics: Option<crate::worker::DynMetrics>,
    ) -> (BoxedWorker, Option<crate::worker::DynMetrics>) {
        (
            Box::new(ConfigurationWorker::new(self.context.clone())) as Box<_>,
            metrics,
        )
    }
}

/// The refresh worker never surfaces an error: CFG-047 says a failed run logs
/// and leaves the stored copy alone. This exists only to satisfy the worker
/// trait's error contract.
#[derive(Debug, thiserror::Error)]
#[error("the configuration refresh worker stopped")]
pub struct ConfigurationWorkerError;

impl NeedsDbReconnect for ConfigurationWorkerError {
    fn needs_db_reconnect(&self) -> bool {
        false
    }
}

pub struct ConfigurationWorker<Context> {
    context: Context,
}

impl<Context> ConfigurationWorker<Context> {
    pub fn new(context: Context) -> Self {
        Self { context }
    }
}

#[xmtp_common::async_trait]
impl<Context> Worker for ConfigurationWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    fn kind(&self) -> WorkerKind {
        WorkerKind::ConfigurationRefresh
    }

    async fn run_tasks(&mut self) -> WorkerResult<()> {
        self.run().await;
        Ok(())
    }

    fn factory<C>(context: C) -> impl WorkerFactory + 'static
    where
        C: XmtpSharedContext + 'static,
    {
        Factory { context }
    }
}

impl<Context> ConfigurationWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    async fn run(&mut self) {
        loop {
            let (base, jitter) = self.schedule();
            xmtp_common::time::sleep(base + xmtp_common::time::rand_offset(jitter)).await;
            // A latched client has nothing left to learn from the backend.
            if self.context.server_configuration().latched().is_some() {
                return;
            }
            self.tick().await;
            // CFG-051 and CFG-061: a latch closes every open stream. Cancelling
            // is what closes them; the streams read the latch to report why.
            if self.context.server_configuration().latched().is_some() {
                self.context.cancellation_token().cancel();
                return;
            }
        }
    }

    /// `(base, jitter)`. A per-worker override wins; otherwise the compiled
    /// hourly cadence and its 360-second spread.
    fn schedule(&self) -> (Duration, Duration) {
        let kind = WorkerKind::ConfigurationRefresh;
        let (base, jitter) = self
            .context
            .worker_interval(kind, CONFIGURATION_REFRESH_INTERVAL);
        let jitter = if self
            .context
            .worker_config()
            .jitter_overrides
            .contains_key(&kind)
        {
            jitter
        } else {
            CONFIGURATION_REFRESH_JITTER
        };
        (base, jitter)
    }

    /// One run: up to three attempts, then give up until the next run.
    #[tracing::instrument(
        skip_all,
        fields(worker = "ConfigurationRefresh", operation = "worker_turn")
    )]
    pub(crate) async fn tick(&mut self) {
        for attempt in 1..=CONFIGURATION_REFRESH_ATTEMPTS {
            match self.attempt().await {
                Ok(()) => return,
                Err(error) => {
                    // CFG-049: a server error never triggers another refresh.
                    // The run's own schedule is the only thing driving this.
                    tracing::warn!(
                        attempt,
                        %error,
                        "server configuration refresh attempt failed; keeping the stored copy"
                    );
                    if !error.is_retryable() {
                        return;
                    }
                    match CONFIGURATION_REFRESH_BACKOFF.get(attempt - 1) {
                        Some(wait) => xmtp_common::time::sleep(*wait).await,
                        None => return,
                    }
                }
            }
        }
    }

    async fn attempt(&mut self) -> Result<(), ClientError> {
        let handle = self.context.server_configuration();
        let db = self.context.db();
        let fetched = fetch_and_store(self.context.api(), &db, handle).await?;

        // CFG-061: the copy is stored either way, and the client stops.
        if let Err(ClientError::ClientVersionTooOld { client, minimum }) =
            check_minimum_version(&fetched, self.context.version_info().pkg_semver().semver())
        {
            tracing::error!(
                %client,
                %minimum,
                "the backend now requires a newer libxmtp than this client"
            );
            handle.latch(ConfigurationLatch::ClientVersionTooOld { client, minimum });
        }
        Ok(())
    }
}
