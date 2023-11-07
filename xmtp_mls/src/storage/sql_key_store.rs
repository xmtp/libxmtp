use std::borrow::Cow;

use log::{debug, error};
use openmls_traits::key_store::{MlsEntity, OpenMlsKeyStore};

use super::{
    encrypted_store::key_store_entry::StoredKeyStoreEntry,
    serialization::{db_deserialize, db_serialize},
    EncryptedMessageStore, StorageError,
};
use crate::{Delete, Fetch, Store};

#[derive(Debug)]
/// CRUD Operations for an [`EncryptedMessageStore`]
pub struct SqlKeyStore<'a> {
    store: Cow<'a, EncryptedMessageStore>,
}

impl Default for SqlKeyStore<'_> {
    fn default() -> Self {
        Self {
            store: Cow::Owned(EncryptedMessageStore::default()),
        }
    }
}

impl<'a> SqlKeyStore<'a> {
    pub fn new(store: &'a EncryptedMessageStore) -> Self {
        SqlKeyStore {
            store: Cow::Borrowed(store),
        }
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
        let entry = StoredKeyStoreEntry {
            key_bytes: k.to_vec(),
            value_bytes: db_serialize(v)?,
        };
        entry.store(&mut self.store.conn()?)?;
        Ok(())
    }

    /// Read and return a value stored for ID `k` that implements the
    /// [`FromKeyStoreValue`] trait for deserialization.
    ///
    /// Returns [`None`] if no value is stored for `k` or reading fails.
    fn read<V: MlsEntity>(&self, k: &[u8]) -> Option<V> {
        let conn_result = self.store.conn();
        if let Err(e) = conn_result {
            error!("Failed to get connection: {:?}", e);
            return None;
        }
        let mut conn = conn_result.unwrap();
        let fetch_result = conn.fetch(k.to_vec());
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
        let num_deleted = self.store.conn()?.delete(k.to_vec())?;
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
        utils::test::rand_string,
    };

    #[test]
    fn store_read_delete() {
        let db_path = format!("{}.db3", rand_string());
        let store = EncryptedMessageStore::new(
            StorageOption::Persistent(db_path),
            EncryptedMessageStore::generate_enc_key(),
        )
        .unwrap();
        let key_store = SqlKeyStore {
            store: (&store).into(),
        };
        let signature_keys = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm()).unwrap();
        let index = "index".as_bytes();
        assert!(key_store.read::<SignatureKeyPair>(index).is_none());
        key_store.store(index, &signature_keys).unwrap();
        assert!(key_store.read::<SignatureKeyPair>(index).is_some());
        key_store.delete::<SignatureKeyPair>(index).unwrap();
        assert!(key_store.read::<SignatureKeyPair>(index).is_none());
    }
}
