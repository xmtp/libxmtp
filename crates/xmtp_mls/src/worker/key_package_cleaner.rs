use crate::identity::IdentityError;
use crate::worker::NeedsDbReconnect;
use thiserror::Error;
use xmtp_db::StorageError;

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

/// Holds the local key-package deletion helper used outside the (now removed)
/// polling worker loop. The recurring delete/rotate maintenance runs on the
/// TaskRunner via [`KeyPackageMaintenance`](super::key_package_maintenance);
/// this struct survives only for [`Self::delete_key_package`], which is used by
/// a test in `identity.rs` — hence `#[cfg(test)]`.
#[cfg(test)]
use crate::{context::XmtpSharedContext, identity::pq_key_package_references_key};
#[cfg(test)]
use openmls_traits::storage::StorageProvider;
#[cfg(test)]
use xmtp_db::{
    MlsProviderExt, XmtpMlsStorageProvider,
    sql_key_store::{KEY_PACKAGE_REFERENCES, KEY_PACKAGE_WRAPPER_PRIVATE_KEY},
};

#[cfg(test)]
pub struct KeyPackagesCleanerWorker<Context> {
    context: Context,
}

#[cfg(test)]
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
}
