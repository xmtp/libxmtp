use serde::Serialize;

use super::StorageError;
use crate::sql_key_store::codec::{value_from_slice, value_to_vec};

/// Encode a value for a serialized database column, with the key store's
/// value codec.
pub fn db_serialize<T>(value: &T) -> Result<Vec<u8>, StorageError>
where
    T: ?Sized + Serialize,
{
    value_to_vec(value).map_err(|_| StorageError::DbSerialize)
}

/// Decode a value written by [`db_serialize`].
pub fn db_deserialize<T>(bytes: &[u8]) -> Result<T, StorageError>
where
    T: serde::de::DeserializeOwned,
{
    value_from_slice(bytes).map_err(|_| StorageError::DbDeserialize)
}

#[cfg(test)]
mod tests {
    use super::{db_deserialize, db_serialize};

    /// Serialized columns keep today's bincode bytes.
    #[xmtp_common::test(unwrap_try = true)]
    fn columns_keep_bincode_bytes() {
        let value = (3u8, vec![1u8, 2, 3], Some("name".to_owned()));
        let bytes = db_serialize(&value)?;
        assert_eq!(bytes, bincode::serialize(&value)?);
        assert_eq!(
            db_deserialize::<(u8, Vec<u8>, Option<String>)>(&bytes)?,
            value
        );
        assert!(matches!(
            db_deserialize::<Vec<u8>>(&[0xff; 3]),
            Err(crate::StorageError::DbDeserialize)
        ));
    }
}
