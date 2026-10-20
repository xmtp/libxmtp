//! Turns MLS storage keys and values into bytes.
//!
//! Every key part, value and list that `SqlKeyStore` encodes or decodes goes
//! through this module. So do the `xmtp_mls` sites that build key store bytes
//! by hand, and the `db_serialize` columns.
//!
//! # Layout
//!
//! - Most labels build the key with `build_key`, then `write` adds the label
//!   and version again: `label || label || encode_key(key) || version ||
//!   version`, where the version is a `u16` big endian. The resumption PSK
//!   store, queued proposals, epoch key pairs and the XMTP labels pass a key to
//!   `write` directly: `label || key || version`.
//! - [`encode_key`] uses the fixed layout in [`key`]: the bytes bincode 1.3
//!   wrote. Keys did not change when values moved to CBOR.
//! - A value is `encode_value(value)`: CBOR (RFC 8949), written by cbor4ii
//!   through serde.
//! - A list is `encode_list(items)`: a CBOR array of byte strings, where each
//!   item is `encode_value(item)`.
//! - `KEY_PACKAGE_REFERENCES` keys are not encoded here. They are the TLS
//!   encoding of an HPKE public key: the init key (`identity.rs`
//!   `store_key_package_references`) or the post-quantum wrapper key
//!   (`pq_key_package_references_key`). A Welcome names the key by its TLS
//!   bytes, so the lookup in `decrypted_welcome.rs` uses those bytes as they
//!   are.
//!
//! # The value codec
//!
//! Every value byte string is encoded by [`value_to_vec`] and decoded by
//! [`value_from_slice`].
//!
//! - CBOR stores struct field names and enum variant names. A field added
//!   with a serde default reads old rows. A renamed field or variant makes
//!   every stored row unreadable. The golden rows in `codec/golden_tests.rs`
//!   fail when an OpenMLS or libxmtp change does that.
//! - Wiping depends on exact-size buffers. [`value_to_vec`] allocates the
//!   final size once. A decoded `Vec<u8>` is sized exactly from the CBOR
//!   array length, which `XmtpMlsStorageProvider::wrapper_private_key`
//!   relies on. The heap canary tests in `tests/zeroize.rs` check both.
//! - Databases written with the earlier bincode values are rejected when they
//!   open (`StorageError::OldEncodingDatabase`), so no reader handles both.
//!
//! The legacy archive conversion (`xmtp_legacy_migration`) reads old
//! databases with its own reader. It does not use this module and must not
//! change with it.

use std::convert::Infallible;

use cbor4ii::core::{
    dec::{Decode as _, IgnoredAny, Read as _},
    enc,
    utils::SliceReader,
};
use cbor4ii::serde::{DecodeError, Deserializer, EncodeError, Serializer};
use serde::{
    Deserialize, Serialize,
    de::{DeserializeOwned, SeqAccess, Visitor},
};
use xmtp_configuration::OPENMLS_KV_TARGET;
use xmtp_cryptography::secret::Zeroizing;

use super::SqlKeyStoreError;

#[cfg(test)]
mod golden_tests;
pub mod key;

/// The error of the value codec.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ValueError {
    #[error("CBOR encode failed: {0}")]
    Encode(#[from] EncodeError<Infallible>),
    #[error("CBOR decode failed: {0}")]
    Decode(#[from] DecodeError<Infallible>),
    #[error("{0} bytes follow the value")]
    TrailingBytes(usize),
}

impl ValueError {
    /// The kind of failure and the CBOR item it concerns, for logs.
    ///
    /// cbor4ii's error text can quote the header byte it did not expect, and
    /// a byte below 24 is its own header: one byte of the value. serde's text
    /// can quote a decoded number or string. So logs carry only these static
    /// names.
    fn kind(&self) -> (&'static str, &'static str) {
        use cbor4ii::core::error::DecodeError as Core;
        match self {
            Self::Encode(_) => ("encode", ""),
            Self::TrailingBytes(_) => ("trailing bytes", ""),
            Self::Decode(DecodeError::Custom(_)) => ("serde", ""),
            Self::Decode(DecodeError::Core(error)) => match error {
                Core::Mismatch { name, .. } => ("mismatch", **name),
                Core::Unsupported { name, .. } => ("unsupported", **name),
                Core::Eof { name, .. } => ("eof", **name),
                Core::RequireLength { name, .. } => ("require length", **name),
                Core::RequireBorrowed { name } => ("require borrowed", **name),
                Core::RequireUtf8 { name } => ("require utf-8", **name),
                Core::LengthOverflow { name, .. } => ("length overflow", **name),
                Core::CastOverflow { name } => ("cast overflow", **name),
                Core::ArithmeticOverflow { name, .. } => ("arithmetic overflow", **name),
                Core::DepthOverflow { name } => ("depth overflow", **name),
                Core::Custom { name, .. } => ("custom", **name),
                _ => ("other", ""),
            },
        }
    }
}

/// Counts the bytes of an encoding.
struct ByteCount(usize);

impl enc::Write for ByteCount {
    type Error = Infallible;

    fn push(&mut self, input: &[u8]) -> Result<(), Infallible> {
        self.0 += input.len();
        Ok(())
    }
}

/// Appends to a buffer that already has room for the whole encoding.
struct Buffer(Vec<u8>);

impl enc::Write for Buffer {
    type Error = Infallible;

    fn push(&mut self, input: &[u8]) -> Result<(), Infallible> {
        self.0.extend_from_slice(input);
        Ok(())
    }
}

/// Encode a value with the value codec. Every encoded value comes from here.
///
/// It measures the encoding first and then writes it into a buffer of that
/// exact size, so encoding never reallocates and frees a partial copy of a
/// private key.
pub(crate) fn value_to_vec<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>, ValueError> {
    let mut count = Serializer::new(ByteCount(0));
    value.serialize(&mut count)?;
    let mut buffer = Serializer::new(Buffer(Vec::with_capacity(count.into_inner().0)));
    value.serialize(&mut buffer)?;
    Ok(buffer.into_inner().0)
}

/// Decode one value written by [`value_to_vec`]. Every decoded value comes
/// from here. Bytes after the value are an error.
///
/// It finds where the value ends before it decodes it, so a row with bytes
/// after the value is rejected without building the value: a private key in
/// it would be dropped unwiped.
pub(crate) fn value_from_slice<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, ValueError> {
    match bytes.len() - value_len(bytes)? {
        0 => {
            let mut reader = SliceReader::new(bytes);
            Ok(T::deserialize(&mut Deserializer::new(&mut reader))?)
        }
        trailing => Err(ValueError::TrailingBytes(trailing)),
    }
}

/// The length of the one CBOR item at the start of `bytes`. Skipping an item
/// copies none of it.
fn value_len(bytes: &[u8]) -> Result<usize, ValueError> {
    let mut reader = SliceReader::new(bytes);
    IgnoredAny::decode(&mut reader).map_err(DecodeError::from)?;
    let Ok(rest) = reader.fill(bytes.len());
    Ok(bytes.len() - rest.as_ref().len())
}

/// Whether `item` holds the value whose encoding is `encoded`: the same bytes,
/// or bytes that decode as a `V` and encode back to them. A value can have
/// more than one valid encoding, for example after a field's serde shape
/// changes but still reads the old form.
pub(crate) fn same_value<V: Serialize + DeserializeOwned>(item: &[u8], encoded: &[u8]) -> bool {
    item == encoded
        || value_from_slice::<V>(item)
            .and_then(|value| value_to_vec(&value).map(Zeroizing::new))
            .is_ok_and(|bytes| *bytes == *encoded)
}

/// Encode the key part of a storage key: no label and no version.
pub fn encode_key<K: Serialize + ?Sized>(
    label: &[u8],
    key: &K,
) -> Result<Vec<u8>, SqlKeyStoreError> {
    key::to_vec(key).map_err(|error| {
        tracing::error!(
            target: OPENMLS_KV_TARGET,
            label = %String::from_utf8_lossy(label),
            type_name = std::any::type_name::<K>(),
            %error,
            "key store key encode failed",
        );
        SqlKeyStoreError::SerializationError
    })
}

/// Encode a value, in a buffer that is wiped on drop.
///
/// Some values hold private keys, so every value is wiped, whatever its label.
pub fn encode_value<V: Serialize + ?Sized>(
    label: &[u8],
    value: &V,
) -> Result<Zeroizing<Vec<u8>>, SqlKeyStoreError> {
    value_to_vec(value).map(Zeroizing::new).map_err(|error| {
        let (kind, item) = error.kind();
        tracing::error!(
            target: OPENMLS_KV_TARGET,
            label = %String::from_utf8_lossy(label),
            type_name = std::any::type_name::<V>(),
            error_kind = kind,
            error_item = item,
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
    let items: Vec<&serde_bytes::Bytes> = items
        .iter()
        .map(|item| serde_bytes::Bytes::new(item.as_ref()))
        .collect();
    encode_value(label, &items)
}

/// Decode a list written by [`encode_list`] into its encoded items, wiped on drop.
pub fn decode_list(
    label: &[u8],
    bytes: &[u8],
) -> Result<Zeroizing<Vec<Vec<u8>>>, SqlKeyStoreError> {
    decode_value::<ListItems>(label, bytes).map(|items| items.0)
}

/// The items of a list. Each item goes straight into a buffer that is wiped on
/// drop, also when a later item fails to decode.
struct ListItems(Zeroizing<Vec<Vec<u8>>>);

impl<'de> Deserialize<'de> for ListItems {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ItemsVisitor;

        impl<'de> Visitor<'de> for ItemsVisitor {
            type Value = ListItems;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an array of byte strings")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<ListItems, A::Error> {
                let mut items = ListItems(Zeroizing::new(Vec::new()));
                while let Some(item) = seq.next_element::<serde_bytes::ByteBuf>()? {
                    items.0.push(item.into_vec());
                }
                Ok(items)
            }
        }

        deserializer.deserialize_seq(ItemsVisitor)
    }
}

/// Log a failed decode: the label, the type, the byte length, the kind of
/// failure and the path of the failing field. Never the value bytes, which
/// can hold a private key, and never the codec's error text (see
/// [`ValueError::kind`]).
///
/// The path comes from a second decode through `serde_path_to_error`, which
/// runs only when the `openmls_kv` target logs errors. A row with bytes after
/// its value has no failing field, and is not decoded again: the decode would
/// build the whole value.
fn log_decode_error<V: DeserializeOwned>(label: &[u8], bytes: &[u8], error: &ValueError) {
    if !tracing::event_enabled!(target: OPENMLS_KV_TARGET, tracing::Level::ERROR) {
        return;
    }
    let path = match error {
        ValueError::TrailingBytes(_) => String::new(),
        _ => {
            let mut reader = SliceReader::new(bytes);
            match serde_path_to_error::deserialize::<_, V>(&mut Deserializer::new(&mut reader)) {
                Err(error) => error.path().to_string(),
                Ok(_) => String::new(),
            }
        }
    };
    let (kind, item) = error.kind();
    tracing::error!(
        target: OPENMLS_KV_TARGET,
        label = %String::from_utf8_lossy(label),
        type_name = std::any::type_name::<V>(),
        bytes_len = bytes.len(),
        path = %path,
        error_kind = kind,
        error_item = item,
        "key store value decode failed",
    );
}

/// The forms in which a log line or a span could hold `secret`: its raw hex,
/// the hex of its CBOR array items (how a stored value holds a byte vector),
/// and the `Debug` text of both, without the brackets. Negative log and span
/// checks look for every form.
#[cfg(test)]
pub(crate) fn logged_forms(secret: &[u8]) -> [String; 4] {
    let cbor_items: Vec<u8> = secret
        .iter()
        .flat_map(|&byte| {
            if byte >= 0x18 {
                vec![0x18, byte]
            } else {
                vec![byte]
            }
        })
        .collect();
    let debug = |bytes: &[u8]| format!("{bytes:?}").replace(['[', ']'], "");
    [
        hex::encode(secret),
        hex::encode(&cbor_items),
        debug(secret),
        debug(&cbor_items),
    ]
}

#[cfg(test)]
mod tests {
    use super::{
        ValueError, decode_list, decode_value, encode_list, encode_value, value_from_slice,
        value_to_vec,
    };
    use crate::sql_key_store::SqlKeyStoreError;
    use serde::{Deserialize, Serialize};

    const LABEL: &[u8] = b"CodecTest";

    /// A value encodes as plain cbor4ii CBOR. The exact-size buffer changes
    /// no byte.
    #[xmtp_common::test(unwrap_try = true)]
    fn values_are_cbor() {
        let value = (7u32, vec![1u8, 2, 3], [9u8; 4]);
        let encoded = encode_value(LABEL, &value)?;
        assert_eq!(*encoded, cbor4ii::serde::to_vec(Vec::new(), &value)?);
        assert_eq!(
            hex::encode(&*encoded),
            // array(3), 7, array(3) 1 2 3, array(4) 9 9 9 9
            "8307830102038409090909"
        );
        assert_eq!(encoded.capacity(), encoded.len());
        assert_eq!(
            decode_value::<(u32, Vec<u8>, [u8; 4])>(LABEL, &encoded)?,
            value
        );
    }

    /// A decoded `Vec<u8>` has exactly its length as capacity, so it moves into
    /// `Zeroizing` with no unwiped spare copy. `wrapper_private_key` relies on
    /// this.
    #[xmtp_common::test(unwrap_try = true)]
    fn decoded_byte_vectors_are_sized_exactly() {
        for len in [0, 1, 32, 2_400, 70_000] {
            let encoded = encode_value(LABEL, vec![0x5au8; len].as_slice())?;
            let decoded: Vec<u8> = decode_value(LABEL, &encoded)?;
            assert_eq!(decoded.len(), len);
            assert_eq!(decoded.capacity(), len);
        }
    }

    /// A struct is a map keyed by field names, and an enum variant is its name.
    #[xmtp_common::test(unwrap_try = true)]
    fn structs_and_enums_store_names() {
        #[derive(Serialize)]
        struct Row {
            id: u8,
            kind: Kind,
        }
        #[derive(Serialize)]
        enum Kind {
            Unit,
        }
        assert_eq!(
            hex::encode(value_to_vec(&Row {
                id: 1,
                kind: Kind::Unit
            })?),
            // map(2) "id" 1 "kind" "Unit"
            "a262696401646b696e6464556e6974"
        );
    }

    /// A field added later with a default reads rows written before it, and a
    /// reader ignores a field it does not know.
    #[xmtp_common::test(unwrap_try = true)]
    fn added_and_unknown_fields() {
        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        struct Before {
            id: u8,
        }
        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        struct After {
            id: u8,
            #[serde(default)]
            added: Option<u8>,
        }
        let old = value_to_vec(&Before { id: 1 })?;
        assert_eq!(
            value_from_slice::<After>(&old)?,
            After { id: 1, added: None }
        );
        let new = value_to_vec(&After {
            id: 2,
            added: Some(3),
        })?;
        assert_eq!(value_from_slice::<Before>(&new)?, Before { id: 2 });
    }

    /// A list is an array of byte strings.
    #[xmtp_common::test(unwrap_try = true)]
    fn lists_are_arrays_of_byte_strings() {
        let items = [b"ab".to_vec(), vec![0xcc; 300]];
        let list = encode_list(LABEL, &items)?;
        let expected = [
            &[0x82, 0x42, b'a', b'b', 0x59, 0x01, 0x2c][..],
            &[0xcc; 300][..],
        ]
        .concat();
        assert_eq!(*list, expected);
        let slices: Vec<&[u8]> = items.iter().map(Vec::as_slice).collect();
        assert_eq!(*encode_list(LABEL, &slices)?, *list);
        assert_eq!(*decode_list(LABEL, &list)?, items.to_vec());
        assert_eq!(*encode_list::<Vec<u8>>(LABEL, &[])?, [0x80]);
        assert!(decode_list(LABEL, &[0x80])?.is_empty());
    }

    /// Bytes that are not one whole value are an error, never a panic: an
    /// empty row, a cut row, a row with bytes after the value, a row of the
    /// wrong type, and a corrupted header.
    #[xmtp_common::test(unwrap_try = true)]
    fn bad_rows_are_errors() {
        let value = (7u32, vec![1u8; 40], "text".to_owned());
        let encoded = value_to_vec(&value)?;
        for end in 0..encoded.len() {
            assert!(
                value_from_slice::<(u32, Vec<u8>, String)>(&encoded[..end]).is_err(),
                "a row cut at {end} decoded"
            );
        }
        let longer = [encoded.as_slice(), &[0]].concat();
        assert!(matches!(
            value_from_slice::<(u32, Vec<u8>, String)>(&longer),
            Err(ValueError::TrailingBytes(1))
        ));
        assert!(value_from_slice::<u64>(&encoded).is_err());
        for header in [0xff, 0x1f, 0x5f, 0xfe] {
            let mut corrupt = encoded.clone();
            corrupt[0] = header;
            assert!(value_from_slice::<(u32, Vec<u8>, String)>(&corrupt).is_err());
        }
        // A length far past the end of the row.
        assert!(value_from_slice::<Vec<u8>>(&[0x9b, 0xff, 0xff, 0xff, 0xff, 0, 0, 0, 0]).is_err());
        assert!(value_from_slice::<serde_bytes::ByteBuf>(&[0x5a, 0xff, 0xff, 0xff, 0xff]).is_err());
    }

    /// A failed decode logs the label, the type, the byte length and the path
    /// of the failing field, and never the value bytes.
    #[cfg(not(target_arch = "wasm32"))]
    #[xmtp_common::test(unwrap_try = true)]
    fn decode_failures_log_the_field_path_and_no_value_bytes() {
        #[derive(Serialize, Deserialize)]
        struct Inner {
            secret: Vec<u8>,
            count: u32,
        }
        #[derive(Serialize, Deserialize)]
        struct Outer {
            inner: Inner,
        }
        #[derive(Serialize)]
        struct WrongInner {
            secret: Vec<u8>,
            count: &'static str,
        }
        #[derive(Serialize)]
        struct WrongOuter {
            inner: WrongInner,
        }
        let secret = vec![0xa7; 32];
        let bytes = value_to_vec(&WrongOuter {
            inner: WrongInner {
                secret: secret.clone(),
                count: "not a number",
            },
        })?;

        let (result, spans) = xmtp_logging::test_logging::with_trace_layer(true, || {
            tracing::error_span!("decode")
                .in_scope(|| decode_value::<Outer>(LABEL, &bytes).map(drop))
        });
        assert!(matches!(result, Err(SqlKeyStoreError::SerializationError)));
        let logged = format!("{spans:?}");
        assert!(logged.contains("key store value decode failed"), "{logged}");
        assert!(logged.contains("CodecTest"));
        assert!(logged.contains("Outer"));
        assert!(logged.contains("inner.count"));
        assert!(logged.contains(&bytes.len().to_string()));
        assert!(!logged.contains(&hex::encode(&bytes)));
        for form in super::logged_forms(&secret) {
            assert!(!logged.contains(&form), "the log holds the secret: {form}");
        }
    }

    /// cbor4ii's error text quotes the header byte it did not expect, and a
    /// byte below 24 is its own header. The log names the kind of failure
    /// and the item instead.
    #[cfg(not(target_arch = "wasm32"))]
    #[xmtp_common::test(unwrap_try = true)]
    fn decode_failures_log_no_byte_of_the_value() {
        let bytes = value_to_vec(&vec![5u8; 32])?;
        let (result, spans) = xmtp_logging::test_logging::with_trace_layer(true, || {
            tracing::error_span!("decode").in_scope(|| decode_value::<Vec<bool>>(LABEL, &bytes))
        });
        assert!(matches!(result, Err(SqlKeyStoreError::SerializationError)));
        let logged = format!("{spans:?}");
        assert!(logged.contains("key store value decode failed"), "{logged}");
        assert!(logged.contains("mismatch"), "{logged}");
        assert!(!logged.contains("found"), "{logged}");
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn decode_errors_are_serialization_errors() {
        assert!(matches!(
            decode_value::<Vec<u8>>(LABEL, &[0xff; 3]),
            Err(SqlKeyStoreError::SerializationError)
        ));
        assert!(matches!(
            decode_list(LABEL, &[0x81, 0x01]),
            Err(SqlKeyStoreError::SerializationError)
        ));
        assert!(matches!(
            decode_list(LABEL, &[0x82, 0x41]),
            Err(SqlKeyStoreError::SerializationError)
        ));
    }
}
