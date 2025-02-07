use crate::client::ClientError;
use crate::storage::StorageError;
use crate::Client;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::OnceCell;
use xmtp_common::time::Interval;
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_proto::api_client::trait_impls::XmtpApi;

/// Duration to wait before restarting the worker in case of an error.
pub const WORKER_RESTART_DELAY: Duration = Duration::from_secs(1);

/// Interval at which the DisappearingMessagesCleanerWorker runs to delete expired messages.
pub const INTERVAL_DURATION: Duration = Duration::from_secs(1);

#[derive(Debug, Error)]
pub enum DisappearingMessagesCleanerError {
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("client error: {0}")]
    Client(#[from] ClientError),
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
        crate::spawn(None, async move {
            let inbox_id = self.client.inbox_id().to_string();
            let installation_id = hex::encode(self.client.installation_public_key());
            while let Err(err) = self.run().await {
                tracing::info!("Running worker..");
                match err {
                    DisappearingMessagesCleanerError::Client(ClientError::Storage(
                        StorageError::PoolNeedsConnection,
                    )) => {
                        tracing::warn!(
                            inbox_id,
                            installation_id,
                            "Pool disconnected. task will restart on reconnect"
                        );
                        break;
                    }
                    _ => {
                        tracing::error!(inbox_id, installation_id, "sync worker error {err}");
                        xmtp_common::time::sleep(WORKER_RESTART_DELAY).await;
                    }
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
        self.client
            .mls_provider()?
            .conn_ref()
            .delete_expired_messages()?;
        Ok(())
    }
    async fn run(&mut self) -> Result<(), DisappearingMessagesCleanerError> {
        let mut interval = Interval::new(INTERVAL_DURATION);

        loop {
            interval.tick().await;
            self.delete_expired_messages().await?;
        }
    }
}
