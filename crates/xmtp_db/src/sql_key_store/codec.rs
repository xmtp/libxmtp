//! The one place that turns MLS storage keys and values into bytes.
//!
//! Every key part, value and list that `SqlKeyStore` writes or reads goes
//! through this module, and so do the `xmtp_mls` sites that build key store
//! bytes by hand and the `db_serialize` columns.
//!
//! - A storage key is `label || encode_key(key) || u16 BE version`. Keys use
//!   the bincode 1.3 layout in [`key`], whatever the value codec.
//! - A value is `encode_value(value)`.
//! - A list is `encode_list(items)`, where each item is `encode_value(item)`.
//!
//! Every value byte string goes through one function pair,
//! [`value_to_vec`] and [`value_from_slice`]. A change of value codec is a
//! change to that pair. Today it is bincode 1.3 default options.

use serde::{Serialize, de::DeserializeOwned};
use xmtp_configuration::OPENMLS_KV_TARGET;
use xmtp_cryptography::secret::Zeroizing;

use super::SqlKeyStoreError;

pub mod key;

/// The error of the value codec.
pub(crate) type ValueError = bincode::Error;

/// Encode a value with the value codec. Every encoded value comes from here.
///
/// `bincode::serialize` allocates the exact size first, so encoding never
/// reallocates and frees a partial copy of a private key.
pub(crate) fn value_to_vec<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>, ValueError> {
    bincode::serialize(value)
}

/// Decode a value written by [`value_to_vec`]. Every decoded value comes from here.
pub(crate) fn value_from_slice<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, ValueError> {
    bincode::deserialize(bytes)
}

/// Encode the key part of a storage key: no label and no version.
pub fn encode_key<K: Serialize + ?Sized>(
    label: &[u8],
    key: &K,
) -> Result<Vec<u8>, SqlKeyStoreError> {
    let bytes = key::to_vec(key).map_err(|error| {
        tracing::error!(
            target: OPENMLS_KV_TARGET,
            label = %String::from_utf8_lossy(label),
            type_name = std::any::type_name::<K>(),
            %error,
            "key store key encode failed",
        );
        SqlKeyStoreError::SerializationError
    })?;
    #[cfg(any(test, feature = "test-utils"))]
    key::assert_bincode_layout(label, key, &bytes);
    Ok(bytes)
}

/// Encode a value, in a buffer that is wiped on drop.
///
/// Some values hold private keys, so every value is wiped, whatever its label.
pub fn encode_value<V: Serialize + ?Sized>(
    label: &[u8],
    value: &V,
) -> Result<Zeroizing<Vec<u8>>, SqlKeyStoreError> {
    value_to_vec(value).map(Zeroizing::new).map_err(|error| {
        tracing::error!(
            target: OPENMLS_KV_TARGET,
            label = %String::from_utf8_lossy(label),
            type_name = std::any::type_name::<V>(),
            %error,
            "key store value encode failed",
        );
        SqlKeyStoreError::SerializationError
    })
}

/// Decode a value written by [`encode_value`].
pub fn decode_value<V: DeserializeOwned>(
    label: &[u8],
    bytes: &[u8],
) -> Result<V, SqlKeyStoreError> {
    value_from_slice(bytes).map_err(|error| {
        log_decode_error::<V>(label, bytes, &error);
        SqlKeyStoreError::SerializationError
    })
}

/// Encode a list whose items are already encoded values, in a buffer that is
/// wiped on drop.
pub fn encode_list<I: AsRef<[u8]>>(
    label: &[u8],
    items: &[I],
) -> Result<Zeroizing<Vec<u8>>, SqlKeyStoreError> {
    let items: Vec<&[u8]> = items.iter().map(AsRef::as_ref).collect();
    encode_value(label, &items)
}

/// Decode a list written by [`encode_list`] into its encoded items, wiped on drop.
pub fn decode_list(
    label: &[u8],
    bytes: &[u8],
) -> Result<Zeroizing<Vec<Vec<u8>>>, SqlKeyStoreError> {
    decode_value(label, bytes).map(Zeroizing::new)
}

/// With the `deserialize-paths` feature and the `openmls_kv` target enabled,
/// decode with bincode again through `serde_path_to_error` and log the path of
/// the failing field.
#[cfg_attr(not(feature = "deserialize-paths"), allow(unused_variables))]
fn log_decode_error<V: DeserializeOwned>(label: &[u8], bytes: &[u8], error: &ValueError) {
    #[cfg(feature = "deserialize-paths")]
    if tracing::event_enabled!(target: OPENMLS_KV_TARGET, tracing::Level::ERROR) {
        use bincode::Options;
        let opts = bincode::DefaultOptions::new()
            .with_fixint_encoding()
            .allow_trailing_bytes();
        let mut de = bincode::de::Deserializer::from_slice(bytes, opts);
        match serde_path_to_error::deserialize::<_, V>(&mut de) {
            Ok(_) => {
                // The second pass succeeded: a bincode option mismatch. Log
                // the error without a path.
                tracing::error!(
                    target: OPENMLS_KV_TARGET,
                    label = %String::from_utf8_lossy(label),
                    type_name = std::any::type_name::<V>(),
                    bytes_len = bytes.len(),
                    error = %error,
                    "bincode deserialize failed (no path captured)",
                );
            }
            Err(e) => {
                let path = e.path().to_string();
                let inner = e.into_inner();
                tracing::error!(
                    target: OPENMLS_KV_TARGET,
                    label = %String::from_utf8_lossy(label),
                    path = %path,
                    type_name = std::any::type_name::<V>(),
                    bytes_len = bytes.len(),
                    error = %inner,
                    "bincode deserialize failed",
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{decode_list, decode_value, encode_list, encode_value};
    use crate::sql_key_store::SqlKeyStoreError;

    const LABEL: &[u8] = b"CodecTest";

    /// Values and lists keep today's bincode bytes.
    #[xmtp_common::test(unwrap_try = true)]
    fn values_and_lists_keep_bincode_bytes() {
        let value = (7u32, vec![1u8, 2, 3], [9u8; 4]);
        let encoded = encode_value(LABEL, &value)?;
        assert_eq!(*encoded, bincode::serialize(&value)?);
        assert_eq!(
            decode_value::<(u32, Vec<u8>, [u8; 4])>(LABEL, &encoded)?,
            value
        );

        let items = [b"first".to_vec(), vec![0xab; 300]];
        let list = encode_list(LABEL, &items)?;
        assert_eq!(*list, bincode::serialize(&items.to_vec())?);
        let slices: Vec<&[u8]> = items.iter().map(Vec::as_slice).collect();
        assert_eq!(*encode_list(LABEL, &slices)?, *list);
        assert_eq!(*decode_list(LABEL, &list)?, items.to_vec());
        assert_eq!(
            *encode_list::<Vec<u8>>(LABEL, &[])?,
            bincode::serialize(&Vec::<Vec<u8>>::new())?
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn decode_errors_are_serialization_errors() {
        assert!(matches!(
            decode_value::<Vec<u8>>(LABEL, &[0xff; 3]),
            Err(SqlKeyStoreError::SerializationError)
        ));
        assert!(matches!(
            decode_list(LABEL, &[0x05]),
            Err(SqlKeyStoreError::SerializationError)
        ));
    }
}
