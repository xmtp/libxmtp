use log::{debug, error};
use openmls_traits::key_store::{MlsEntity, OpenMlsKeyStore};

use super::{
    encrypted_store::{
        key_store_entry::StoredKeyStoreEntry, xmtp_db_connection::XmtpDbConnection,
    },
    serialization::{db_deserialize, db_serialize}, StorageError,
};
use crate::{Delete, Fetch};

/// CRUD Operations for an [`EncryptedMessageStore`]
#[derive(Debug)]
pub struct SqlKeyStore<'a> {
    conn: &'a XmtpDbConnection<'a>,
}

impl<'a> SqlKeyStore<'a> {
    pub fn new(conn: &'a XmtpDbConnection<'a>) -> Self {
        Self { conn }
    }

    pub fn conn(&self) -> &XmtpDbConnection<'a> {
        self.conn
    }
}

impl OpenMlsKeyStore for SqlKeyStore<'_> {
    /// The error type returned by the [`OpenMlsKeyStore`].
    type Error = StorageError;

    /// Store a value `v` that implements the [`ToKeyStoreValue`] trait for
    /// serialization for ID `k`.
    ///
    /// Returns an error if storing fails.
    fn store<V: MlsEntity>(&self, k: &[u8], v: &V) -> Result<(), Self::Error> {
        self.conn()
            .insert_or_update_key_store_entry(k.to_vec(), db_serialize(v)?)?;
        Ok(())
    }

    /// Read and return a value stored for ID `k` that implements the
    /// [`FromKeyStoreValue`] trait for deserialization.
    ///
    /// Returns [`None`] if no value is stored for `k` or reading fails.
    fn read<V: MlsEntity>(&self, k: &[u8]) -> Option<V> {
        let fetch_result = self.conn().fetch(&k.to_vec());

        if let Err(e) = fetch_result {
            error!("Failed to fetch key: {:?}", e);
            return None;
        }
        let entry_option: Option<StoredKeyStoreEntry> = fetch_result.unwrap();

        if entry_option.is_none() {
            debug!("No entry to read for key {:?}", k);
            return None;
        }
        db_deserialize(&entry_option.unwrap().value_bytes).ok()
    }

    /// Delete a value stored for ID `k`.
    ///
    /// Interface is unclear on expected behavior when item is already deleted -
    /// we choose to not surface an error if this is the case.
    fn delete<V: MlsEntity>(&self, k: &[u8]) -> Result<(), Self::Error> {
        let conn: &dyn Delete<StoredKeyStoreEntry, Key = Vec<u8>> = self.conn();
        let num_deleted = conn.delete(k.to_vec())?;
        if num_deleted == 0 {
            debug!("No entry to delete for key {:?}", k);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use openmls_basic_credential::SignatureKeyPair;
    use openmls_traits::key_store::OpenMlsKeyStore;

    use super::SqlKeyStore;
    use crate::{
        configuration::CIPHERSUITE,
        storage::{EncryptedMessageStore, StorageOption},
        utils::test::tmp_path,
    };

    #[test]
    fn store_read_delete() {
        let db_path = tmp_path();
        let store = EncryptedMessageStore::new(
            StorageOption::Persistent(db_path),
            EncryptedMessageStore::generate_enc_key(),
        )
        .unwrap();
        let conn = &store.conn().unwrap();
        let key_store = SqlKeyStore::new(conn);
        let signature_keys = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm()).unwrap();
        let index = "index".as_bytes();
        assert!(key_store.read::<SignatureKeyPair>(index).is_none());
        key_store.store(index, &signature_keys).unwrap();
        assert!(key_store.read::<SignatureKeyPair>(index).is_some());
        key_store.delete::<SignatureKeyPair>(index).unwrap();
        assert!(key_store.read::<SignatureKeyPair>(index).is_none());
    }
}
