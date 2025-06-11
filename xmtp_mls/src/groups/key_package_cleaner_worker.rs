use crate::context::XmtpContextProvider;
use crate::context::XmtpMlsLocalContext;
use crate::context::XmtpSharedContext;
use crate::identity::IdentityError;
use crate::worker::BoxedWorker;
use crate::worker::NeedsDbReconnect;
use crate::worker::WorkerResult;
use crate::worker::{Worker, WorkerFactory, WorkerKind};
use futures::StreamExt;
use futures::TryFutureExt;
use openmls_traits::storage::StorageProvider;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::OnceCell;
use xmtp_db::{MlsProviderExt, StorageError, XmtpDb};
use xmtp_proto::api_client::trait_impls::XmtpApi;

/// Interval at which the KeyPackagesCleanerWorker runs to delete expired messages.
pub const INTERVAL_DURATION: Duration = Duration::from_secs(5);

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
        WorkerKind::KeyPackageCleaner
    }

    fn create(
        &self,
        metrics: Option<crate::worker::DynMetrics>,
    ) -> (BoxedWorker, Option<crate::worker::DynMetrics>) {
        (
            Box::new(KeyPackagesCleanerWorker::new(self.context.clone())) as Box<_>,
            metrics,
        )
    }
}

#[derive(Debug, Error)]
pub enum KeyPackagesCleanerError {
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("identity error: {0}")]
    Identity(#[from] IdentityError),
}

impl NeedsDbReconnect for KeyPackagesCleanerError {
    fn needs_db_reconnect(&self) -> bool {
        match self {
            Self::Storage(s) => s.db_needs_connection(),
            Self::Identity(s) => s.needs_db_reconnect(),
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<ApiClient, Db> Worker for KeyPackagesCleanerWorker<ApiClient, Db>
where
    ApiClient: XmtpApi + 'static,
    Db: XmtpDb + 'static + Send,
{
    fn kind(&self) -> WorkerKind {
        WorkerKind::KeyPackageCleaner
    }

    async fn run_tasks(&mut self) -> WorkerResult<()> {
        self.run().map_err(|e| Box::new(e) as Box<_>).await
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

pub struct KeyPackagesCleanerWorker<ApiClient, Db> {
    context: Arc<XmtpMlsLocalContext<ApiClient, Db>>,
    #[allow(dead_code)]
    init: OnceCell<()>,
}

impl<ApiClient, Db> KeyPackagesCleanerWorker<ApiClient, Db>
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

impl<ApiClient, Db> KeyPackagesCleanerWorker<ApiClient, Db>
where
    ApiClient: XmtpApi + 'static,
    Db: XmtpDb + 'static,
{
    async fn run(&mut self) -> Result<(), KeyPackagesCleanerError> {
        let mut intervals = xmtp_common::time::interval_stream(INTERVAL_DURATION);
        while (intervals.next().await).is_some() {
            self.delete_expired_key_packages()?;
            self.rotate_last_key_package_if_needed().await?;
        }
        Ok(())
    }

    /// Delete a key package from the local database.
    pub(crate) fn delete_key_package(&self, hash_ref: Vec<u8>) -> Result<(), IdentityError> {
        let openmls_hash_ref = crate::identity::deserialize_key_package_hash_ref(&hash_ref)?;
        self.context
            .mls_provider()
            .key_store()
            .delete_key_package(&openmls_hash_ref)?;

        Ok(())
    }

    /// Delete all the expired keys
    fn delete_expired_key_packages(&mut self) -> Result<(), KeyPackagesCleanerError> {
        let provider = self.context.mls_provider();
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
        let provider = self.context.mls_provider();
        let conn = provider.db();

        if conn.is_identity_needs_rotation()? {
            self.context
                .identity()
                .rotate_and_upload_key_package(&provider, self.context.api())
                .await?;
            return Ok(());
        }

        Ok(())
    }
}
