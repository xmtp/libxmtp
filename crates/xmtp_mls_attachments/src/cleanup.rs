use super::*;
use xmtp_mls_core::worker::{
    BoxedWorker, NewWorker, Worker, WorkerFactory, WorkerKind, WorkerResult,
};

/// Periodically deletes expired pending attachments and their files.
pub struct AttachmentCleanup<Context> {
    context: Context,
}
struct Factory<Context> {
    context: Context,
}

impl<Context: AttachmentContext + 'static> WorkerFactory for Factory<Context> {
    fn kind(&self) -> WorkerKind {
        WorkerKind::AttachmentCleanup
    }
    fn create(&self) -> BoxedWorker {
        Box::new(AttachmentCleanup {
            context: self.context.clone(),
        })
    }
}

impl<Context: AttachmentContext + 'static> NewWorker<Context> for AttachmentCleanup<Context> {
    fn factory(context: Context) -> impl WorkerFactory + 'static {
        Factory { context }
    }
}

#[xmtp_common::async_trait]
impl<Context: AttachmentContext + 'static> Worker for AttachmentCleanup<Context> {
    fn kind(&self) -> WorkerKind {
        WorkerKind::AttachmentCleanup
    }
    async fn run_tasks(&mut self) -> WorkerResult<()> {
        loop {
            let (interval, _) = self
                .context
                .worker_interval(WorkerKind::AttachmentCleanup, Duration::from_secs(3600));
            xmtp_common::time::sleep(interval.min(Duration::from_secs(3600))).await;
            let runtime = self.context.attachment_runtime();
            if let Err(error) = runtime.sweep(&self.context).await {
                tracing::warn!(%error, "attachment expiry sweep failed");
            }
            if let Err(error) = runtime.reconcile(&self.context).await {
                tracing::warn!(%error, "attachment reconciliation failed");
            }
        }
    }
}
