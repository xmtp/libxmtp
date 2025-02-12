use serde::Serialize;

use super::StorageError;

pub fn db_serialize<T>(value: &T) -> Result<Vec<u8>, StorageError>
where
    T: ?Sized + Serialize,
{
    bincode::serialize(value).map_err(|_| StorageError::DbSerialize)
}

pub fn db_deserialize<T>(bytes: &[u8]) -> Result<T, StorageError>
where
    T: serde::de::DeserializeOwned,
{
    bincode::deserialize::<T>(bytes).map_err(|_| StorageError::DbDeserialize)
}
