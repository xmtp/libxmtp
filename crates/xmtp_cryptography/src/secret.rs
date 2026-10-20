//! Secret bytes and the zeroize helpers.
//!
//! This module is the one import path for zeroization in libxmtp:
//!
//! - [`Secret`] owns secret bytes in a struct field, a return value or a
//!   long-lived binding. It is wiped on drop and its `Debug` shows only the
//!   length.
//! - [`Zeroizing`] wraps a transient buffer (a serialization output, a
//!   `to_vec()`, an FFI argument) at the point where it is created. Its
//!   `Debug` prints the inner value, so never format one.
//! - [`Zeroize`] and [`ZeroizeOnDrop`] are for hand-written secret holders.
//!
//! With the `test-utils` feature, `heap_canary` provides the test allocator
//! that proves a secret is wiped before its heap block is freed.

use std::fmt;

use serde::de::{SeqAccess, Visitor};
use subtle::ConstantTimeEq;
pub use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

#[cfg(all(feature = "test-utils", unix))]
pub mod heap_canary;

/// Capacity reserved before a [`Secret`] is deserialized.
///
/// Every key libxmtp holds as a `Secret` fits, so decoding one never reallocates
/// and frees a partial copy.
const DESERIALIZE_CAPACITY: usize = 64;

/// Owned secret bytes. Wiped on drop; `Debug` shows only the length.
///
/// Serializes as a byte string (`serialize_bytes`, CBOR major type 2), and
/// deserializes one into a wiping buffer with a single copy. A type whose
/// serde shape stores its secret as a sequence of `u8`, as OpenMLS's
/// `SignatureKeyPair` does, reads it with [`deserialize_seq`]. Equality is
/// constant time.
#[derive(Clone)]
pub struct Secret(Zeroizing<Vec<u8>>);

impl Secret {
    /// Take ownership of `bytes`. The buffer is wiped when the `Secret` drops.
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(Zeroizing::new(bytes))
    }

    /// Borrow the secret bytes.
    pub fn as_slice(&self) -> &[u8] {
        self.0.as_slice()
    }
}

impl From<Vec<u8>> for Secret {
    fn from(bytes: Vec<u8>) -> Self {
        Self::new(bytes)
    }
}

impl From<Secret> for Zeroizing<Vec<u8>> {
    fn from(secret: Secret) -> Self {
        secret.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Secret(<{} bytes>)", self.0.len())
    }
}

impl PartialEq for Secret {
    fn eq(&self, other: &Self) -> bool {
        self.as_slice().ct_eq(other.as_slice()).into()
    }
}

impl Eq for Secret {}

impl ZeroizeOnDrop for Secret {}

/// A secret is stored in the MLS key store with the same bytes as a
/// `Vec<u8>`, and reading it through `Secret` wipes a partly decoded value.
impl openmls_traits::storage::Entity<{ openmls_traits::storage::CURRENT_VERSION }> for Secret {}

impl serde::Serialize for Secret {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(self.as_slice())
    }
}

impl<'de> serde::Deserialize<'de> for Secret {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_bytes(SecretVisitor)
    }
}

/// Read a [`Secret`] stored as a sequence of `u8`, the serde shape of a
/// `Vec<u8>`, into a wiping buffer. For a type whose shape another crate
/// owns, such as OpenMLS's `SignatureKeyPair`.
pub fn deserialize_seq<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Secret, D::Error> {
    deserializer.deserialize_seq(SecretVisitor)
}

/// Reads secret bytes into a wiping buffer. Borrowed bytes are copied once
/// into a buffer of their exact size; an owned buffer moves in without a
/// copy; a sequence (from a format that has no byte strings, or from
/// [`deserialize_seq`]) is pushed into a buffer that grows only through
/// wiped copies. A decode that fails partway frees nothing unwiped.
struct SecretVisitor;

impl<'de> Visitor<'de> for SecretVisitor {
    type Value = Secret;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("secret bytes")
    }

    fn visit_bytes<E: serde::de::Error>(self, bytes: &[u8]) -> Result<Secret, E> {
        let mut secret = Zeroizing::new(Vec::with_capacity(bytes.len()));
        secret.extend_from_slice(bytes);
        Ok(Secret(secret))
    }

    fn visit_byte_buf<E: serde::de::Error>(self, bytes: Vec<u8>) -> Result<Secret, E> {
        Ok(Secret(Zeroizing::new(bytes)))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Secret, A::Error> {
        let mut bytes = Zeroizing::new(Vec::with_capacity(DESERIALIZE_CAPACITY));
        while let Some(byte) = seq.next_element::<u8>()? {
            push_zeroizing(&mut bytes, byte);
        }
        Ok(Secret(bytes))
    }
}

/// Push `byte`, growing through a new zeroizing buffer when `bytes` is full.
///
/// `Vec::push` would reallocate and free the old block without wiping it.
fn push_zeroizing(bytes: &mut Zeroizing<Vec<u8>>, byte: u8) {
    if bytes.len() == bytes.capacity() {
        let mut grown = Zeroizing::new(Vec::with_capacity(bytes.capacity().max(1) * 2));
        grown.extend_from_slice(bytes);
        // The old buffer is wiped when `grown` replaces it.
        *bytes = grown;
    }
    bytes.push(byte);
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: [u8; 32] = [
        0x8f, 0x13, 0x2a, 0x55, 0xc0, 0x01, 0x7e, 0x9d, 0x42, 0xbb, 0x06, 0xe1, 0x3c, 0x77, 0xa4,
        0x18, 0xd2, 0x5b, 0x90, 0x6f, 0x24, 0xe8, 0x31, 0xcd, 0x0a, 0x99, 0x4e, 0xf7, 0x63, 0xb5,
        0x2c, 0x81,
    ];

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn debug_shows_only_the_length() {
        let debug = format!("{:?}", Secret::new(KEY.to_vec()));
        assert_eq!(debug, "Secret(<32 bytes>)");
        assert!(!debug.contains(&hex::encode(KEY)));
        assert!(!debug.contains(&format!("{:?}", KEY.as_slice())));
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn serde_is_a_byte_string() {
        for len in [0, 1, 23, 24, 32, 64, 65, 200] {
            let bytes: Vec<u8> = (0..len).map(|i| i as u8).collect();
            let secret = Secret::new(bytes.clone());

            // CBOR, the client database's codec: a byte string, so the raw
            // bytes follow the header.
            let cbor_secret = cbor4ii::serde::to_vec(Vec::new(), &secret).unwrap();
            let header = match u8::try_from(len).unwrap() {
                short @ 0..=23 => vec![0x40 | short],
                long => vec![0x58, long],
            };
            assert_eq!(cbor_secret, [header.as_slice(), &bytes].concat());
            let decoded: Secret = cbor4ii::serde::from_slice(&cbor_secret).unwrap();
            assert_eq!(decoded.as_slice(), bytes.as_slice());

            // A secret in a type whose shape is a `Vec<u8>` reads with
            // `deserialize_seq`.
            let cbor_vec = cbor4ii::serde::to_vec(Vec::new(), &bytes).unwrap();
            let mut reader = cbor4ii::core::utils::SliceReader::new(&cbor_vec);
            let decoded =
                deserialize_seq(&mut cbor4ii::serde::Deserializer::new(&mut reader)).unwrap();
            assert_eq!(decoded.as_slice(), bytes.as_slice());

            // JSON has no byte strings: an array of numbers, like a `Vec<u8>`.
            let json_secret = serde_json::to_vec(&secret).unwrap();
            assert_eq!(json_secret, serde_json::to_vec(&bytes).unwrap());
            let decoded: Secret = serde_json::from_slice(&json_secret).unwrap();
            assert_eq!(decoded.as_slice(), bytes.as_slice());
        }
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn serde_rejects_what_is_not_secret_bytes() {
        for json in ["[256]", "[-1]", "[1,", "{}", "null", "7"] {
            assert!(serde_json::from_str::<Secret>(json).is_err(), "{json}");
        }
        // CBOR: a byte string longer than the input, the `Vec<u8>` form (an
        // array of integers), and a text string.
        let mut truncated = cbor4ii::serde::to_vec(Vec::new(), &Secret::new(KEY.to_vec())).unwrap();
        truncated.truncate(truncated.len() - 1);
        let integers = cbor4ii::serde::to_vec(Vec::new(), &KEY.to_vec()).unwrap();
        for cbor in [&truncated[..], &integers, &[0x62, b'a', b'b']] {
            assert!(
                cbor4ii::serde::from_slice::<Secret>(cbor).is_err(),
                "{cbor:02x?}"
            );
        }
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn equality_compares_bytes() {
        assert_eq!(Secret::new(KEY.to_vec()), Secret::from(KEY.to_vec()));
        assert_ne!(Secret::new(KEY.to_vec()), Secret::new(KEY[..31].to_vec()));
        let mut other = KEY;
        other[31] ^= 1;
        assert_ne!(Secret::new(KEY.to_vec()), Secret::new(other.to_vec()));
    }
}
