use crate::context::XmtpSharedContext;
use crate::identity::IdentityError;
use crate::identity::pq_key_package_references_key;
use crate::worker::BoxedWorker;
use crate::worker::NeedsDbReconnect;
use crate::worker::WorkerResult;
use crate::worker::{Worker, WorkerFactory, WorkerKind};
use futures::StreamExt;
use futures::TryFutureExt;
use openmls_traits::storage::StorageProvider;
use std::time::Duration;
use thiserror::Error;
use xmtp_configuration::CREATE_PQ_KEY_PACKAGE_EXTENSION;
use xmtp_db::prelude::*;
use xmtp_db::{
    MlsProviderExt, StorageError,
    sql_key_store::{KEY_PACKAGE_REFERENCES, KEY_PACKAGE_WRAPPER_PRIVATE_KEY},
};

/// Interval at which the KeyPackagesCleanerWorker runs to delete expired messages.
pub const INTERVAL_DURATION: Duration = Duration::from_secs(5);

#[derive(Clone)]
pub struct Factory<Context> {
    context: Context,
}

impl<Context> WorkerFactory for Factory<Context>
where
    Context: XmtpSharedContext + 'static,
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
    #[error("generic storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("generic identity error: {0}")]
    Identity(#[from] IdentityError),
    #[error("metadata error: {0}")]
    Metadata(StorageError),
    #[error("failed to fetch expired key packages: {0}")]
    Fetch(StorageError),
    #[error("failed to delete key package: {0}")]
    DeleteKeyPackage(IdentityError),
    #[error("deletion error: {0}")]
    Deletion(StorageError),
    #[error("rotation error: {0}")]
    Rotation(IdentityError),
}

impl NeedsDbReconnect for KeyPackagesCleanerError {
    fn needs_db_reconnect(&self) -> bool {
        match self {
            Self::Storage(s) => s.db_needs_connection(),
            Self::Identity(s) => s.needs_db_reconnect(),
            Self::Metadata(s) => s.db_needs_connection(),
            Self::Fetch(s) => s.db_needs_connection(),
            Self::DeleteKeyPackage(s) => s.needs_db_reconnect(),
            Self::Deletion(s) => s.db_needs_connection(),
            Self::Rotation(s) => s.needs_db_reconnect(),
        }
    }
}

#[xmtp_common::async_trait]
impl<Context> Worker for KeyPackagesCleanerWorker<Context>
where
    Context: XmtpSharedContext + 'static,
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
        C: XmtpSharedContext + 'static,
    {
        Factory { context }
    }
}

pub struct KeyPackagesCleanerWorker<Context> {
    context: Context,
}

impl<Context> KeyPackagesCleanerWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    pub fn new(context: Context) -> Self {
        Self { context }
    }
}

impl<Context> KeyPackagesCleanerWorker<Context>
where
    Context: XmtpSharedContext + 'static,
{
    async fn run(&mut self) -> Result<(), KeyPackagesCleanerError> {
        let (base, jitter) = self
            .context
            .worker_interval(WorkerKind::KeyPackageCleaner, INTERVAL_DURATION);
        let mut intervals = xmtp_common::time::jittered_interval_stream(base, jitter);
        while (intervals.next().await).is_some() {
            self.delete_expired_key_packages()?;
            self.rotate_last_key_package_if_needed().await?;
        }
        Ok(())
    }

    /// Delete a key package from the local database.
    pub(crate) fn delete_key_package(
        &self,
        hash_ref: Vec<u8>,
        pq_pub_key: Option<Vec<u8>>,
    ) -> Result<(), IdentityError> {
        let openmls_hash_ref = crate::identity::deserialize_key_package_hash_ref(&hash_ref)?;
        let mls_provider = self.context.mls_provider();
        let key_store = mls_provider.key_store();

        key_store.delete_key_package(&openmls_hash_ref)?;

        if let Some(pq_pub_key) = pq_pub_key {
            key_store.delete(
                KEY_PACKAGE_REFERENCES,
                pq_key_package_references_key(&pq_pub_key)?.as_slice(),
            )?;
            key_store.delete(KEY_PACKAGE_WRAPPER_PRIVATE_KEY, &hash_ref)?;
        }

        Ok(())
    }

    /// Delete all the expired keys
    fn delete_expired_key_packages(&mut self) -> Result<(), KeyPackagesCleanerError> {
        let conn = self.context.db();

        // Propagate (don't swallow): a swallowed fetch error never triggered the supervisor's
        // reconnect path, so a pool outage retried silently every 5s.
        let expired_kps = conn
            .get_expired_key_packages()
            .map_err(KeyPackagesCleanerError::Fetch)?;
        if expired_kps.is_empty() {
            return Ok(());
        }

        tracing::info!("Deleting {} expired key packages", expired_kps.len());
        // Delete from local db
        for kp in &expired_kps {
            self.delete_key_package(
                kp.key_package_hash_ref.clone(),
                kp.post_quantum_public_key.clone(),
            )
            .map_err(KeyPackagesCleanerError::DeleteKeyPackage)?;
        }

        // Delete from database using the max expired ID
        if let Some(max_id) = expired_kps.iter().map(|kp| kp.id).max() {
            conn.delete_key_package_history_up_to_id(max_id)
                .map_err(KeyPackagesCleanerError::Deletion)?;
            tracing::info!(
                "Deleted {} expired key packages (up to ID {}) from local DB and state",
                expired_kps.len(),
                max_id
            );
        }

        Ok(())
    }

    /// Check if we need to rotate the keys and upload new keypackage if the las one rotate in has passed
    async fn rotate_last_key_package_if_needed(&mut self) -> Result<(), KeyPackagesCleanerError> {
        let conn = self.context.db();

        if conn
            .is_identity_needs_rotation()
            .map_err(KeyPackagesCleanerError::Metadata)?
        {
            tracing::info!("Rotating key package");
            self.context
                .identity()
                .rotate_and_upload_key_package(
                    self.context.api(),
                    self.context.mls_storage(),
                    CREATE_PQ_KEY_PACKAGE_EXTENSION,
                )
                .await
                .map_err(KeyPackagesCleanerError::Rotation)?;
            tracing::info!("Key package rotation successful");
            return Ok(());
        }

        Ok(())
    }
}
