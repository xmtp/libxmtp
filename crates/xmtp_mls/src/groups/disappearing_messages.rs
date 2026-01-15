use crate::context::XmtpSharedContext;
use crate::messages::decoded_message::DecodedMessage;
use crate::worker::{BoxedWorker, NeedsDbReconnect, Worker, WorkerFactory};
use crate::worker::{WorkerKind, WorkerResult};
use futures::{StreamExt, TryFutureExt};
use std::time::Duration;
use thiserror::Error;
use tokio::sync::OnceCell;
use xmtp_db::{StorageError, prelude::*};

/// Interval at which the DisappearingMessagesCleanerWorker runs to delete expired messages.
pub const INTERVAL_DURATION: Duration = Duration::from_secs(1);

#[derive(Debug, Error)]
pub enum DisappearingMessagesCleanerError {
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
}

impl NeedsDbReconnect for DisappearingMessagesCleanerError {
    fn needs_db_reconnect(&self) -> bool {
        match self {
            Self::Storage(s) => s.db_needs_connection(),
        }
    }
}

pub struct DisappearingMessagesWorker<Context> {
    context: Context,
    #[allow(dead_code)]
    init: OnceCell<()>,
}

struct Factory<Context> {
    context: Context,
}

impl<Context> WorkerFactory for Factory<Context>
where
    Context: XmtpSharedContext + 'static,
{
    fn create(
        &self,
        metrics: Option<crate::worker::DynMetrics>,
    ) -> (BoxedWorker, Option<crate::worker::DynMetrics>) {
        let worker = Box::new(DisappearingMessagesWorker::new(self.context.clone())) as Box<_>;
        (worker, metrics)
    }

    fn kind(&self) -> WorkerKind {
        WorkerKind::DisappearingMessages
    }
}

#[xmtp_common::async_trait]
impl<Context> Worker for DisappearingMessagesWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    fn kind(&self) -> WorkerKind {
        WorkerKind::DisappearingMessages
    }

    async fn run_tasks(&mut self) -> WorkerResult<()> {
        self.run().map_err(|e| Box::new(e) as Box<_>).await
    }

    fn factory<C>(context: C) -> impl WorkerFactory + 'static
    where
        Self: Sized,
        C: XmtpSharedContext + 'static,
    {
        Factory { context }
    }
}

impl<Context> DisappearingMessagesWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    pub fn new(context: Context) -> Self {
        Self {
            context,
            init: OnceCell::new(),
        }
    }
}

impl<Context> DisappearingMessagesWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    async fn run(&mut self) -> Result<(), DisappearingMessagesCleanerError> {
        let mut intervals = xmtp_common::time::interval_stream(INTERVAL_DURATION);
        while (intervals.next().await).is_some() {
            self.delete_expired_messages().await?;
        }
        Ok(())
    }

    /// Iterate on the list of groups and delete expired messages
    async fn delete_expired_messages(&mut self) -> Result<(), DisappearingMessagesCleanerError> {
        let db = self.context.db();
        match db.delete_expired_messages() {
            Ok(deleted_messages) if !deleted_messages.is_empty() => {
                tracing::info!(
                    "Successfully deleted {} expired messages",
                    deleted_messages.len()
                );

                // Emit an event for each deleted message
                for message in deleted_messages {
                    let message_id = message.id.clone();
                    // Try to convert to DecodedMessage, log warning if conversion fails
                    match DecodedMessage::try_from(message) {
                        Ok(decoded_message) => {
                            let _ = self.context.local_events().send(
                                crate::subscriptions::LocalEvents::MessageDeleted(Box::new(
                                    decoded_message,
                                )),
                            );
                        }
                        Err(e) => {
                            tracing::warn!(
                                message_id = hex::encode(&message_id),
                                error = ?e,
                                "Failed to decode expired message for deletion event"
                            );
                        }
                    }
                }
            }
            Ok(_) => {}
            Err(e) => {
                tracing::error!("Failed to delete expired messages, error: {:?}", e);
            }
        }
        Ok(())
    }
}
