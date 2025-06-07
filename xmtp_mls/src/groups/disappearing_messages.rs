use crate::worker::{NeedsDbReconnect, Worker};
use crate::Client;
use crate::{client::ClientError, worker::WorkerKind};
use futures::StreamExt;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::OnceCell;
use xmtp_db::{StorageError, XmtpDb};
use xmtp_proto::api_client::trait_impls::XmtpApi;

/// Interval at which the DisappearingMessagesCleanerWorker runs to delete expired messages.
pub const INTERVAL_DURATION: Duration = Duration::from_secs(1);

#[derive(Debug, Error)]
pub enum DisappearingMessagesCleanerError {
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("client error: {0}")]
    Client(#[from] ClientError),
}

impl NeedsDbReconnect for DisappearingMessagesCleanerError {
    fn needs_db_reconnect(&self) -> bool {
        match self {
            Self::Storage(s) => s.db_needs_connection(),
            Self::Client(s) => s.db_needs_connection(),
        }
    }
}

pub struct DisappearingMessagesWorker<ApiClient, Db> {
    client: Client<ApiClient, Db>,
    #[allow(dead_code)]
    init: OnceCell<()>,
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl<ApiClient, Db> Worker for DisappearingMessagesWorker<ApiClient, Db>
where
    ApiClient: XmtpApi + 'static,
    Db: xmtp_db::XmtpDb + 'static,
{
    type Error = DisappearingMessagesCleanerError;

    fn kind(&self) -> WorkerKind {
        WorkerKind::DisappearingMessages
    }

    async fn run_tasks(&mut self) -> Result<(), Self::Error> {
        let mut intervals = xmtp_common::time::interval_stream(INTERVAL_DURATION);
        while (intervals.next().await).is_some() {
            self.delete_expired_messages().await?;
        }
        Ok(())
    }
}

impl<ApiClient, Db> DisappearingMessagesWorker<ApiClient, Db>
where
    ApiClient: XmtpApi + 'static,
    Db: XmtpDb + 'static,
{
    pub fn new(client: Client<ApiClient, Db>) -> Self {
        Self {
            client,
            init: OnceCell::new(),
        }
    }
}

impl<ApiClient, Db> DisappearingMessagesWorker<ApiClient, Db>
where
    ApiClient: XmtpApi + 'static,
    Db: XmtpDb + 'static,
{
    /// Iterate on the list of groups and delete expired messages
    async fn delete_expired_messages(&mut self) -> Result<(), DisappearingMessagesCleanerError> {
        let provider = self.client.mls_provider();
        match provider.db().delete_expired_messages() {
            Ok(deleted_count) if deleted_count > 0 => {
                tracing::info!("Successfully deleted {} expired messages", deleted_count);
            }
            Ok(_) => {}
            Err(e) => {
                tracing::error!("Failed to delete expired messages, error: {:?}", e);
            }
        }
        Ok(())
    }
}
