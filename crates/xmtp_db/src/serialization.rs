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

/// Serde `with` helpers that store a libxmtp-owned byte field as a CBOR byte
/// string (major type 2): one header, then the bytes. Serde otherwise writes
/// a `Vec<u8>` as an array of integers, about twice the size and decoded one
/// byte at a time. Put one on every byte field of a libxmtp type stored
/// through [`db_serialize`] or the key store's value codec; types that
/// OpenMLS owns keep their own shape.
///
/// They read only the byte-string form. rc4 is the first release that stores
/// CBOR, so no stored row holds the integer-array form.
pub mod byte_string {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(bytes)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        serde_bytes::ByteBuf::deserialize(deserializer).map(serde_bytes::ByteBuf::into_vec)
    }
}

/// [`byte_string`] for a `Vec<Vec<u8>>`: an array of byte strings.
pub mod byte_strings {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(items: &[Vec<u8>], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(items.iter().map(|item| serde_bytes::Bytes::new(item)))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<Vec<u8>>, D::Error> {
        let items = Vec::<serde_bytes::ByteBuf>::deserialize(deserializer)?;
        Ok(items
            .into_iter()
            .map(serde_bytes::ByteBuf::into_vec)
            .collect())
    }
}

/// [`byte_strings`] for an `Option<Vec<Vec<u8>>>`: null, or an array of
/// byte strings. Use it with `#[serde(default)]`, so an absent field still
/// reads as `None`.
pub mod optional_byte_strings {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    struct List<'a>(&'a [Vec<u8>]);

    impl Serialize for List<'_> {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            super::byte_strings::serialize(self.0, serializer)
        }
    }

    pub fn serialize<S: Serializer>(
        items: &Option<Vec<Vec<u8>>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match items {
            Some(items) => serializer.serialize_some(&List(items)),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<Vec<Vec<u8>>>, D::Error> {
        let items = Option::<Vec<serde_bytes::ByteBuf>>::deserialize(deserializer)?;
        Ok(items.map(|items| {
            items
                .into_iter()
                .map(serde_bytes::ByteBuf::into_vec)
                .collect()
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::{db_deserialize, db_serialize};
    use serde::{Deserialize, Serialize};

    /// The `with` helpers store byte strings: a field, a list of fields, and
    /// an optional list, which also reads as `None` when the field is
    /// missing. The integer-array form of a `Vec<u8>` is an error.
    #[xmtp_common::test(unwrap_try = true)]
    fn byte_fields_are_byte_strings() {
        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        struct Row {
            #[serde(with = "super::byte_string")]
            a: Vec<u8>,
            #[serde(with = "super::byte_strings")]
            b: Vec<Vec<u8>>,
            #[serde(default, with = "super::optional_byte_strings")]
            c: Option<Vec<Vec<u8>>>,
        }
        let row = Row {
            a: vec![0x5a; 2],
            b: vec![vec![1], vec![]],
            c: Some(vec![vec![0x24; 24]]),
        };
        let bytes = db_serialize(&row)?;
        let expected = [
            // map(3) "a" bytes(2)
            &[0xa3, 0x61, b'a', 0x42, 0x5a, 0x5a][..],
            // "b" array(2) bytes(1) 1, bytes(0)
            &[0x61, b'b', 0x82, 0x41, 0x01, 0x40],
            // "c" array(1) bytes(24)
            &[0x61, b'c', 0x81, 0x58, 24],
            &[0x24; 24],
        ]
        .concat();
        assert_eq!(bytes, expected);
        assert_eq!(db_deserialize::<Row>(&bytes)?, row);

        let none = Row { c: None, ..row };
        let bytes = db_serialize(&none)?;
        assert_eq!(bytes[bytes.len() - 3..], [0x61, b'c', 0xf6]);
        assert_eq!(db_deserialize::<Row>(&bytes)?, none);
        // Without the field.
        let missing = [&[0xa2][..], &bytes[1..bytes.len() - 3]].concat();
        assert_eq!(db_deserialize::<Row>(&missing)?, none);

        #[derive(Serialize)]
        struct Integers {
            a: Vec<u8>,
            b: Vec<Vec<u8>>,
        }
        let integers = db_serialize(&Integers {
            a: vec![0x5a; 2],
            b: vec![],
        })?;
        assert!(db_deserialize::<Row>(&integers).is_err());
    }

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
