use crate::client::ClientError;
use crate::configuration::WORKER_RESTART_DELAY;
use crate::identity::IdentityError;
use crate::Client;
use futures::StreamExt;
use openmls_traits::storage::StorageProvider;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::OnceCell;
use tracing::instrument;
use xmtp_db::{MlsProviderExt, StorageError, XmtpDb};
use xmtp_proto::api_client::trait_impls::XmtpApi;

/// Interval at which the KeyPackagesCleanerWorker runs to delete expired messages.
pub const INTERVAL_DURATION: Duration = Duration::from_secs(1);

#[derive(Debug, Error)]
pub enum KeyPackagesCleanerError {
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("client error: {0}")]
    Client(#[from] ClientError),
}

impl KeyPackagesCleanerError {
    fn db_needs_connection(&self) -> bool {
        match self {
            Self::Storage(s) => s.db_needs_connection(),
            Self::Client(s) => s.db_needs_connection(),
        }
    }
}

impl<ApiClient, Db> Client<ApiClient, Db>
where
    ApiClient: XmtpApi + Send + Sync + 'static,
    Db: xmtp_db::XmtpDb + 'static,
{
    #[instrument(level = "trace", skip_all)]
    pub fn start_key_packages_cleaner_worker(&self) {
        let client = self.clone();
        tracing::trace!(
            inbox_id = client.inbox_id(),
            installation_id = hex::encode(client.installation_public_key()),
            "starting key package cleaner worker"
        );

        let worker = KeyPackagesCleanerWorker::new(client);
        worker.spawn_worker();
    }
}

pub struct KeyPackagesCleanerWorker<ApiClient, Db> {
    client: Client<ApiClient, Db>,
    #[allow(dead_code)]
    init: OnceCell<()>,
}

impl<ApiClient, Db> KeyPackagesCleanerWorker<ApiClient, Db>
where
    ApiClient: XmtpApi + Send + Sync + 'static,
    Db: XmtpDb + Send + Sync + 'static,
{
    pub fn new(client: Client<ApiClient, Db>) -> Self {
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

impl<ApiClient, Db> KeyPackagesCleanerWorker<ApiClient, Db>
where
    ApiClient: XmtpApi + Send + Sync + 'static,
    Db: XmtpDb + Send + Sync + 'static,
{
    /// Delete a key package from the local database.
    pub(crate) fn delete_key_package(&self, hash_ref: Vec<u8>) -> Result<(), IdentityError> {
        let openmls_hash_ref = crate::identity::deserialize_key_package_hash_ref(&hash_ref)?;
        self.client
            .mls_provider()
            .key_store()
            .delete_key_package(&openmls_hash_ref)?;

        Ok(())
    }

    /// Delete all the expired keys
    async fn delete_expired_key_packages(&mut self) -> Result<(), KeyPackagesCleanerError> {
        let provider = self.client.mls_provider();
        let conn = provider.db();

        match conn.get_expired_key_packages() {
            Ok(expired_kps) if !expired_kps.is_empty() => {
                // Delete from local db
                for kp in &expired_kps {
                    if let Err(err) = self.delete_key_package(kp.key_package_hash_ref.clone()) {
                        tracing::error!("Couldn't delete KeyPackage: {:?}", err);
                    }
                }

                // Delete from database using the max expired ID
                if let Some(max_id) = expired_kps.iter().map(|kp| kp.id).max() {
                    conn.delete_key_package_history_entries_before_id(max_id)?;
                    tracing::info!(
                        "Deleted {} expired key packages (up to ID {}) from local DB and state",
                        expired_kps.len(),
                        max_id
                    );
                }
            }
            Ok(_) => {
                tracing::debug!("No expired key packages to delete");
            }
            Err(e) => {
                tracing::error!("Failed to fetch expired key packages: {:?}", e);
            }
        }

        Ok(())
    }

    /// Check if we need to rotate the keys and upload new keypackage if the las one rotate in has passed
    async fn rotate_last_key_package_if_needed(&mut self) -> Result<(), KeyPackagesCleanerError> {
        self.client.rotate_and_upload_key_package().await?;
        Ok(())
    }

    async fn run(&mut self) -> Result<(), KeyPackagesCleanerError> {
        let mut intervals = xmtp_common::time::interval_stream(INTERVAL_DURATION);
        while (intervals.next().await).is_some() {
            self.delete_expired_key_packages().await?;
            self.rotate_last_key_package_if_needed().await?;
        }
        Ok(())
    }
}
