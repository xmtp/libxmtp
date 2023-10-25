use openmls_traits::key_store::{MlsEntity, OpenMlsKeyStore};
use std::{collections::HashMap, sync::RwLock};

use super::StorageError;

#[derive(Debug, Default)]
pub struct InMemoryKeyStore {
    values: RwLock<HashMap<Vec<u8>, Vec<u8>>>,
}

impl OpenMlsKeyStore for InMemoryKeyStore {
    /// The error type returned by the [`OpenMlsKeyStore`].
    type Error = StorageError;

    /// Store a value `v` that implements the [`ToKeyStoreValue`] trait for
    /// serialization for ID `k`.
    ///
    /// Returns an error if storing fails.
    fn store<V: MlsEntity>(&self, k: &[u8], v: &V) -> Result<(), Self::Error> {
        let value = serde_json::to_vec(v).map_err(|_| StorageError::SerializationError)?;
        // We unwrap here, because this is the only function claiming a write
        // lock on `credential_bundles`. It only holds the lock very briefly and
        // should not panic during that period.
        let mut values = self.values.write().unwrap();
        values.insert(k.to_vec(), value);
        Ok(())
    }

    /// Read and return a value stored for ID `k` that implements the
    /// [`FromKeyStoreValue`] trait for deserialization.
    ///
    /// Returns [`None`] if no value is stored for `k` or reading fails.
    fn read<V: MlsEntity>(&self, k: &[u8]) -> Option<V> {
        // We unwrap here, because the two functions claiming a write lock on
        // `init_key_package_bundles` (this one and `generate_key_package_bundle`) only
        // hold the lock very briefly and should not panic during that period.
        let values = self.values.read().unwrap();
        if let Some(value) = values.get(k) {
            serde_json::from_slice(value).ok()
        } else {
            None
        }
    }

    /// Delete a value stored for ID `k`.
    ///
    /// Returns an error if storing fails.
    fn delete<V: MlsEntity>(&self, k: &[u8]) -> Result<(), Self::Error> {
        // We just delete both ...
        let mut values = self.values.write().unwrap();
        values.remove(k);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use openmls_basic_credential::SignatureKeyPair;
    use openmls_traits::key_store::OpenMlsKeyStore;

    use crate::configuration::CIPHERSUITE;

    use super::InMemoryKeyStore;

    #[test]
    fn store_read_delete() {
        let key_store = InMemoryKeyStore::default();
        let signature_keys = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm()).unwrap();
        let index = "index".as_bytes();
        assert!(key_store.read::<SignatureKeyPair>(index).is_none());
        key_store.store(index, &signature_keys).unwrap();
        assert!(key_store.read::<SignatureKeyPair>(index).is_some());
        key_store.delete::<SignatureKeyPair>(index).unwrap();
        assert!(key_store.read::<SignatureKeyPair>(index).is_none());
    }
}
