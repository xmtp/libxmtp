use crate::client::ClientError;
use crate::configuration::WORKER_RESTART_DELAY;
use crate::storage::StorageError;
use crate::Client;
use futures::StreamExt;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::OnceCell;
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
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

impl DisappearingMessagesCleanerError {
    fn db_needs_connection(&self) -> bool {
        match self {
            Self::Storage(s) => s.db_needs_connection(),
            Self::Client(s) => s.db_needs_connection(),
        }
    }
}

pub struct DisappearingMessagesCleanerWorker<ApiClient, V> {
    client: Client<ApiClient, V>,
    #[allow(dead_code)]
    init: OnceCell<()>,
}
impl<ApiClient, V> DisappearingMessagesCleanerWorker<ApiClient, V>
where
    ApiClient: XmtpApi + Send + Sync + 'static,
    V: SmartContractSignatureVerifier + Send + Sync + 'static,
{
    pub fn new(client: Client<ApiClient, V>) -> Self {
        Self {
            client,
            init: OnceCell::new(),
        }
    }
    pub(crate) fn spawn_worker(mut self) {
        xmtp_common::spawn(None, async move {
            let inbox_id = self.client.inbox_id().to_string();
            let installation_id = hex::encode(self.client.installation_public_key());
            while let Err(err) = self.run().await {
                tracing::info!("Running worker..");
                if err.db_needs_connection() {
                    tracing::warn!(
                        inbox_id,
                        installation_id,
                        "Pool disconnected. task will restart on reconnect"
                    );
                    break;
                } else {
                    tracing::error!(inbox_id, installation_id, "sync worker error {err}");
                    // Wait 2 seconds before restarting.
                    xmtp_common::time::sleep(WORKER_RESTART_DELAY).await;
                }
            }
        });
    }
}

impl<ApiClient, V> DisappearingMessagesCleanerWorker<ApiClient, V>
where
    ApiClient: XmtpApi + Send + Sync + 'static,
    V: SmartContractSignatureVerifier + Send + Sync + 'static,
{
    /// Iterate on the list of groups and delete expired messages
    async fn delete_expired_messages(&mut self) -> Result<(), DisappearingMessagesCleanerError> {
        let provider = self.client.mls_provider()?;
        match provider.conn_ref().delete_expired_messages() {
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

    async fn run(&mut self) -> Result<(), DisappearingMessagesCleanerError> {
        let mut intervals = xmtp_common::time::interval_stream(INTERVAL_DURATION);
        while (intervals.next().await).is_some() {
            self.delete_expired_messages().await?;
        }
        Ok(())
    }
}
