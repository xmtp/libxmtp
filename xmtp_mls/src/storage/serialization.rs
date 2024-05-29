use serde::Serialize;

use super::StorageError;

pub fn db_serialize<T>(value: &T) -> Result<Vec<u8>, StorageError>
where
    T: ?Sized + Serialize,
{
    serde_json::to_vec(value)
        .map_err(|_| StorageError::Serialization("Failed to db_serialize".to_string()))
}

pub fn db_deserialize<T>(bytes: &[u8]) -> Result<T, StorageError>
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_slice(bytes)
        .map_err(|_| StorageError::Deserialization("Failed to db_deserialize".to_string()))
}
