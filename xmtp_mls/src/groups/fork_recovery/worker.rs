use crate::context::XmtpMlsLocalContext;
use crate::context::XmtpSharedContext;
use crate::worker::BoxedWorker;
use crate::worker::NeedsDbReconnect;
use crate::worker::WorkerResult;
use crate::worker::{Worker, WorkerFactory, WorkerKind};
use futures::StreamExt;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::OnceCell;
use xmtp_db::XmtpDb;
use xmtp_proto::api_client::trait_impls::XmtpApi;

pub const INTERVAL_DURATION: Duration = Duration::from_secs(30);

#[derive(Clone)]
pub struct Factory<ApiClient, Db> {
    context: Arc<XmtpMlsLocalContext<ApiClient, Db>>,
}

impl<ApiClient, Db> WorkerFactory for Factory<ApiClient, Db>
where
    ApiClient: XmtpApi + 'static,
    Db: XmtpDb + 'static,
{
    fn kind(&self) -> WorkerKind {
        WorkerKind::ForkRecovery
    }

    fn create(
        &self,
        metrics: Option<crate::worker::DynMetrics>,
    ) -> (BoxedWorker, Option<crate::worker::DynMetrics>) {
        (
            Box::new(ForkRecoveryWorker::new(self.context.clone())) as Box<_>,
            metrics,
        )
    }
}

#[derive(Debug, Error)]
pub enum ForkRecoveryError {
    #[error("storage error: {0}")]
    Storage(#[from] xmtp_db::StorageError),
    #[error("generic error: {0}")]
    Generic(String),
}

impl NeedsDbReconnect for ForkRecoveryError {
    fn needs_db_reconnect(&self) -> bool {
        match self {
            Self::Storage(s) => s.db_needs_connection(),
            Self::Generic(_) => false,
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<ApiClient, Db> Worker for ForkRecoveryWorker<ApiClient, Db>
where
    ApiClient: XmtpApi + 'static,
    Db: XmtpDb + 'static + Send,
{
    fn kind(&self) -> WorkerKind {
        WorkerKind::ForkRecovery
    }

    async fn run_tasks(&mut self) -> WorkerResult<()> {
        self.run().await.map_err(|e| Box::new(e) as Box<_>)
    }

    fn factory<C>(context: C) -> impl WorkerFactory + 'static
    where
        Self: Sized,
        C: XmtpSharedContext,
        <C as XmtpSharedContext>::Db: 'static,
        <C as XmtpSharedContext>::ApiClient: 'static,
    {
        let context = context.context_ref().clone();
        Factory { context }
    }
}

pub struct ForkRecoveryWorker<ApiClient, Db> {
    context: Arc<XmtpMlsLocalContext<ApiClient, Db>>,
    init: OnceCell<()>,
}

impl<ApiClient, Db> ForkRecoveryWorker<ApiClient, Db>
where
    ApiClient: XmtpApi + 'static,
    Db: XmtpDb + 'static,
{
    pub fn new(context: Arc<XmtpMlsLocalContext<ApiClient, Db>>) -> Self {
        Self {
            context,
            init: OnceCell::new(),
        }
    }
}

impl<ApiClient, Db> ForkRecoveryWorker<ApiClient, Db>
where
    ApiClient: XmtpApi + 'static,
    Db: XmtpDb + 'static,
{
    async fn run(&mut self) -> Result<(), ForkRecoveryError> {
        self.init().await?;

        let mut intervals = xmtp_common::time::interval_stream(INTERVAL_DURATION);
        while (intervals.next().await).is_some() {
            // TODO: Add fork detection and recovery logic here
        }
        Ok(())
    }

    async fn init(&mut self) -> Result<(), ForkRecoveryError> {
        let Self { ref init, .. } = self;

        init.get_or_try_init(|| async {
            tracing::info!(
                inbox_id = self.context.identity.inbox_id(),
                installation_id = hex::encode(self.context.installation_public_key()),
                "Initializing fork recovery worker..."
            );

            tracing::info!(
                inbox_id = self.context.identity.inbox_id(),
                installation_id = hex::encode(self.context.installation_public_key()),
                "Fork recovery worker initialized."
            );

            Ok(())
        })
        .await
        .copied()
    }
}
