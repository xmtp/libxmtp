use crate::identity::IdentityError;
use crate::worker::{Worker, WorkerKind};
use crate::Client;
use crate::{client::ClientError, worker::NeedsDbReconnect};
use futures::StreamExt;
use openmls_traits::storage::StorageProvider;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::OnceCell;
use xmtp_db::{MlsProviderExt, StorageError, XmtpDb};
use xmtp_proto::api_client::trait_impls::XmtpApi;

/// Interval at which the KeyPackagesCleanerWorker runs to delete expired messages.
pub const INTERVAL_DURATION: Duration = Duration::from_secs(5);

#[derive(Debug, Error)]
pub enum KeyPackagesCleanerError {
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("client error: {0}")]
    Client(#[from] ClientError),
}

impl NeedsDbReconnect for KeyPackagesCleanerError {
    fn needs_db_reconnect(&self) -> bool {
        match self {
            Self::Storage(s) => s.db_needs_connection(),
            Self::Client(s) => s.db_needs_connection(),
        }
    }
}

#[async_trait::async_trait]
impl<ApiClient, Db> Worker<ApiClient, Db> for KeyPackagesCleanerWorker<ApiClient, Db>
where
    Self: Send + Sync,
    ApiClient: XmtpApi + Send + Sync + 'static,
    Db: xmtp_db::XmtpDb + Send + Sync + 'static,
{
    type Error = KeyPackagesCleanerError;

    fn kind() -> WorkerKind {
        WorkerKind::KeyPackageCleaner
    }

    fn init(client: &Client<ApiClient, Db>) -> Self {
        KeyPackagesCleanerWorker::new(client.clone())
    }

    async fn run_tasks(&mut self) -> Result<(), Self::Error> {
        let mut intervals = xmtp_common::time::interval_stream(INTERVAL_DURATION);
        while (intervals.next().await).is_some() {
            self.delete_expired_key_packages().await?;
            self.rotate_last_key_package_if_needed().await?;
        }
        Ok(())
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
                    conn.delete_key_package_history_up_to_id(max_id)?;
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
        let provider = self.client.mls_provider();
        let conn = provider.db();

        if conn.is_identity_needs_rotation()? {
            self.client.rotate_and_upload_key_package().await?;
            return Ok(());
        }

        Ok(())
    }
}
