use serde::Serialize;

use super::StorageError;
use crate::sql_key_store::codec::{value_from_slice, value_to_vec};

/// Encode a value for a serialized database column as CBOR, with the key
/// store's value codec.
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

    /// Serialized columns are CBOR, and bad bytes are a typed error.
    #[xmtp_common::test(unwrap_try = true)]
    fn columns_are_cbor() {
        let value = (3u8, vec![1u8, 2, 3], Some("name".to_owned()));
        let bytes = db_serialize(&value)?;
        // array(3), 3, array(3) 1 2 3, "name"
        assert_eq!(hex::encode(&bytes), "830383010203646e616d65");
        assert_eq!(
            db_deserialize::<(u8, Vec<u8>, Option<String>)>(&bytes)?,
            value
        );
        for bad in [&[0xff; 3][..], &bytes[..bytes.len() - 1], &[]] {
            assert!(matches!(
                db_deserialize::<(u8, Vec<u8>, Option<String>)>(bad),
                Err(crate::StorageError::DbDeserialize)
            ));
        }
    }
}
