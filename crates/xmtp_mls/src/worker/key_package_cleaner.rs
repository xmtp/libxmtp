use crate::context::XmtpSharedContext;
use crate::identity::IdentityError;
use crate::identity::pq_key_package_references_key;
use crate::worker::NeedsDbReconnect;
use openmls_traits::storage::StorageProvider;
use thiserror::Error;
use xmtp_db::prelude::*;
use xmtp_db::{
    MlsProviderExt, StorageError,
    sql_key_store::{KEY_PACKAGE_REFERENCES, KEY_PACKAGE_WRAPPER_PRIVATE_KEY},
};

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

/// Not a registered worker anymore: holds the local key-package deletion
/// helpers the TaskRunner's `KpDeletion` arm calls via `sweep_expired`.
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

    /// Delete all expired key packages. Late execution is harmless — deletion
    /// is local-only; the network copy expires independently.
    pub(crate) fn delete_expired_key_packages(&self) -> Result<(), KeyPackagesCleanerError> {
        let conn = self.context.db();

        // Propagate (don't swallow) so the supervisor's reconnect path can fire.
        let expired_kps = conn
            .get_expired_key_packages()
            .map_err(KeyPackagesCleanerError::Fetch)?;
        if expired_kps.is_empty() {
            return Ok(());
        }

        tracing::info!("Deleting {} expired key packages", expired_kps.len());
        for kp in &expired_kps {
            self.delete_key_package(
                kp.key_package_hash_ref.clone(),
                kp.post_quantum_public_key.clone(),
            )
            .map_err(KeyPackagesCleanerError::DeleteKeyPackage)?;
        }

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
}
