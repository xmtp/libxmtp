//! The storage key layout: the bytes bincode 1.3 wrote, without the bincode crate.
//!
//! The store never decodes a key; it only compares key bytes. Keys did not
//! change when the value codec moved from bincode to CBOR. The golden vectors
//! below pin every key type the store writes.
//!
//! Layout (bincode 1.3 `bincode::serialize`: fixed-width integers, little endian):
//! - integers: fixed width, little endian;
//! - byte strings and sequences: a `u64` LE length, then the items;
//! - tuples, structs, tuple structs and newtype structs: the fields in order,
//!   with no names and no length.
//!
//! Every other serde shape is an error: `bool`, `char`, strings, floats,
//! `Option`, unit types, enums, maps and sequences without a length. A change to
//! a key type then fails loudly instead of writing a key no existing row matches.

use serde::{
    Serialize,
    ser::{self, Impossible},
};

/// A key that the key layout cannot encode.
#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    #[error("the key layout does not support {0}")]
    Unsupported(&'static str),
    #[error("key serialization failed: {0}")]
    Custom(String),
}

impl ser::Error for KeyError {
    fn custom<T: std::fmt::Display>(msg: T) -> Self {
        KeyError::Custom(msg.to_string())
    }
}

/// Encode `key` in the key layout.
pub fn to_vec<K: Serialize + ?Sized>(key: &K) -> Result<Vec<u8>, KeyError> {
    let mut out = Vec::new();
    key.serialize(&mut KeySerializer { out: &mut out })?;
    Ok(out)
}

/// Read a byte string in the key layout: a `u64` LE length, then exactly that
/// many bytes. `None` when the input has another shape.
///
/// The hash reference in `key_package_history.key_package_hash_ref` is the one
/// stored key that is decoded.
pub fn decode_bytes(key: &[u8]) -> Option<&[u8]> {
    let (len, rest) = key.split_first_chunk::<8>()?;
    let len = usize::try_from(u64::from_le_bytes(*len)).ok()?;
    (rest.len() == len).then_some(rest)
}

struct KeySerializer<'a> {
    out: &'a mut Vec<u8>,
}

impl KeySerializer<'_> {
    fn length(&mut self, len: usize) {
        self.out.extend_from_slice(&(len as u64).to_le_bytes());
    }
}

macro_rules! fixed_width {
    ($($method:ident: $ty:ty),* $(,)?) => {
        $(fn $method(self, v: $ty) -> Result<(), KeyError> {
            self.out.extend_from_slice(&v.to_le_bytes());
            Ok(())
        })*
    };
}

macro_rules! unsupported {
    ($($method:ident($($arg:ty),*)),* $(,)?) => {
        $(fn $method(self, $(_: $arg),*) -> Result<(), KeyError> {
            Err(KeyError::Unsupported(stringify!($method)))
        })*
    };
}

impl<'a> ser::Serializer for &mut KeySerializer<'a> {
    type Ok = ();
    type Error = KeyError;
    type SerializeSeq = Self;
    type SerializeTuple = Self;
    type SerializeTupleStruct = Self;
    type SerializeStruct = Self;
    type SerializeTupleVariant = Impossible<(), KeyError>;
    type SerializeMap = Impossible<(), KeyError>;
    type SerializeStructVariant = Impossible<(), KeyError>;

    fixed_width!(
        serialize_u8: u8, serialize_u16: u16, serialize_u32: u32, serialize_u64: u64,
        serialize_u128: u128, serialize_i8: i8, serialize_i16: i16, serialize_i32: i32,
        serialize_i64: i64, serialize_i128: i128,
    );

    unsupported!(
        serialize_bool(bool),
        serialize_f32(f32),
        serialize_f64(f64),
        serialize_char(char),
        serialize_str(&str),
        serialize_none(),
        serialize_unit(),
        serialize_unit_struct(&'static str),
        serialize_unit_variant(&'static str, u32, &'static str),
    );

    fn serialize_bytes(self, v: &[u8]) -> Result<(), KeyError> {
        self.length(v.len());
        self.out.extend_from_slice(v);
        Ok(())
    }

    fn serialize_some<T: Serialize + ?Sized>(self, _value: &T) -> Result<(), KeyError> {
        Err(KeyError::Unsupported("serialize_some"))
    }

    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<(), KeyError> {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        _index: u32,
        _variant: &'static str,
        _value: &T,
    ) -> Result<(), KeyError> {
        Err(KeyError::Unsupported("serialize_newtype_variant"))
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<Self, KeyError> {
        let len = len.ok_or(KeyError::Unsupported("a sequence without a length"))?;
        self.length(len);
        Ok(self)
    }

    fn serialize_tuple(self, _len: usize) -> Result<Self, KeyError> {
        Ok(self)
    }

    fn serialize_tuple_struct(self, _name: &'static str, _len: usize) -> Result<Self, KeyError> {
        Ok(self)
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant, KeyError> {
        Err(KeyError::Unsupported("serialize_tuple_variant"))
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap, KeyError> {
        Err(KeyError::Unsupported("serialize_map"))
    }

    fn serialize_struct(self, _name: &'static str, _len: usize) -> Result<Self, KeyError> {
        Ok(self)
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant, KeyError> {
        Err(KeyError::Unsupported("serialize_struct_variant"))
    }

    fn is_human_readable(&self) -> bool {
        false
    }
}

macro_rules! compound {
    ($($trait:ident :: $method:ident),* $(,)?) => {
        $(impl<'a> ser::$trait for &mut KeySerializer<'a> {
            type Ok = ();
            type Error = KeyError;
            fn $method<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), KeyError> {
                value.serialize(&mut **self)
            }
            fn end(self) -> Result<(), KeyError> {
                Ok(())
            }
        })*
    };
}

compound!(
    SerializeSeq::serialize_element,
    SerializeTuple::serialize_element,
    SerializeTupleStruct::serialize_field,
);

impl<'a> ser::SerializeStruct for &mut KeySerializer<'a> {
    type Ok = ();
    type Error = KeyError;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        _key: &'static str,
        value: &T,
    ) -> Result<(), KeyError> {
        value.serialize(&mut **self)
    }
    fn end(self) -> Result<(), KeyError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    //! Golden key vectors: the exact bytes of each key type the key store
    //! writes, as bincode 1.3 wrote them.
    use super::{KeyError, decode_bytes, to_vec};
    use crate::sql_key_store::codec::encode_key;
    use openmls::prelude::tls_codec::{Serialize as _, VLBytes};
    use openmls::{
        group::GroupId,
        prelude::{GroupEpoch, KeyPackageRef as HashReference, tls_codec::Deserialize as _},
        treesync::EncryptionKey,
    };
    use openmls_basic_credential::StorageId;
    use serde::Serialize;

    const LABEL: &[u8] = b"GoldenKey";

    /// Encode through the codec, as the key store does.
    fn key<K: Serialize + ?Sized>(k: &K) -> Vec<u8> {
        encode_key(LABEL, k).expect("a supported key shape")
    }

    fn hash_ref(bytes: &[u8]) -> HashReference {
        let tls = VLBytes::new(bytes.to_vec())
            .tls_serialize_detached()
            .unwrap();
        HashReference::tls_deserialize_exact(tls).unwrap()
    }

    /// A `u64` LE length, then `len` copies of `byte`.
    fn length_prefixed(len: usize, byte: u8) -> Vec<u8> {
        let mut out = (len as u64).to_le_bytes().to_vec();
        out.resize(8 + len, byte);
        out
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn group_id() {
        assert_eq!(
            hex::encode(key(&GroupId::from_slice(b"group"))),
            "050000000000000067726f7570"
        );
        assert_eq!(
            hex::encode(key(&GroupId::from_slice(&[]))),
            "0000000000000000"
        );
    }

    /// Lengths over 255 and over 65,535 still use a `u64` length.
    #[xmtp_common::test(unwrap_try = true)]
    fn long_group_ids() {
        for (len, prefix) in [
            (300, "2c01000000000000"),
            (65_536, "0000010000000000"),
            (70_000, "7011010000000000"),
        ] {
            let bytes = key(&GroupId::from_slice(&vec![0xab; len]));
            assert_eq!(hex::encode(&bytes[..8]), prefix);
            assert_eq!(bytes, length_prefixed(len, 0xab));
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn hash_reference() {
        let bytes = key(&hash_ref(&[0x11; 32]));
        assert_eq!(
            hex::encode(&bytes),
            format!("2000000000000000{}", "11".repeat(32))
        );
        assert_eq!(key(&hash_ref(&[0x22; 300])), length_prefixed(300, 0x22));
        assert_eq!(
            key(&hash_ref(&[0x23; 70_000])),
            length_prefixed(70_000, 0x23)
        );
    }

    /// The queued proposal key: a `(GroupId, ProposalRef)` tuple has no framing.
    #[xmtp_common::test(unwrap_try = true)]
    fn group_id_and_proposal_ref() {
        let group_id = GroupId::from_slice(b"g");
        let proposal_ref = hash_ref(&[0x33; 4]);
        assert_eq!(
            hex::encode(key(&(&group_id, &proposal_ref))),
            "010000000000000067040000000000000033333333"
        );
        // Long parts: each keeps its own `u64` length, back to back.
        for (group_len, ref_len) in [(300, 70_000), (70_000, 300)] {
            let group_id = GroupId::from_slice(&vec![0x34; group_len]);
            let proposal_ref = hash_ref(&vec![0x35; ref_len]);
            assert_eq!(
                key(&(&group_id, &proposal_ref)),
                [
                    length_prefixed(group_len, 0x34),
                    length_prefixed(ref_len, 0x35)
                ]
                .concat()
            );
        }
    }

    /// The epoch key pairs key: `(GroupId, GroupEpoch, u32 leaf index)`.
    #[xmtp_common::test(unwrap_try = true)]
    fn epoch_key_pairs_tuple() {
        let group_id = GroupId::from_slice(b"g");
        let epoch = GroupEpoch::from(0x0102_0304_0506_0708);
        assert_eq!(
            hex::encode(key(&(&group_id, &epoch, 0x0a0b_0c0du32))),
            "0100000000000000670807060504030201".to_owned() + "0d0c0b0a"
        );
        assert_eq!(hex::encode(key(&epoch)), "0807060504030201");
        for len in [300, 70_000] {
            let group_id = GroupId::from_slice(&vec![0x36; len]);
            assert_eq!(
                key(&(&group_id, &epoch, 0x0a0b_0c0du32)),
                [
                    length_prefixed(len, 0x36),
                    hex::decode("08070605040302010d0c0b0a")?
                ]
                .concat()
            );
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn signature_public_key() {
        for len in [32, 300, 70_000] {
            let id = StorageId::from(vec![0x44; len]);
            assert_eq!(key(&id), length_prefixed(len, 0x44));
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn encryption_key() {
        for len in [32, 300, 70_000] {
            let id = EncryptionKey::from(vec![0x55; len]);
            assert_eq!(key(&id), length_prefixed(len, 0x55));
        }
    }

    /// The commit log signer key: a group id as a byte slice, and the XMTP
    /// `GroupId`, which serializes as the same slice.
    #[xmtp_common::test(unwrap_try = true)]
    fn byte_slices() {
        let raw = [0x66u8; 16];
        assert_eq!(key(raw.as_slice()), length_prefixed(16, 0x66));
        let group_id = xmtp_proto::types::GroupId::from(raw);
        assert_eq!(key(&group_id), length_prefixed(16, 0x66));
        assert_eq!(key(&vec![0x67u8; 300]), length_prefixed(300, 0x67));
        assert_eq!(key(&vec![0x67u8; 65_536]), length_prefixed(65_536, 0x67));
        assert_eq!(key(&vec![0x67u8; 70_000]), length_prefixed(70_000, 0x67));
    }

    /// Fixed-size arrays are tuples: no length.
    #[xmtp_common::test(unwrap_try = true)]
    fn arrays_and_integers() {
        assert_eq!(hex::encode(key(&[1u8, 2, 3])), "010203");
        assert_eq!(hex::encode(key(&7u64)), "0700000000000000");
        assert_eq!(hex::encode(key(&7usize)), "0700000000000000");
        assert_eq!(hex::encode(key(&-2i16)), "feff");
        assert_eq!(hex::encode(key(&0x0102u16)), "0201");
        assert_eq!(hex::encode(key(&0x0102_0304u32)), "04030201");
        assert_eq!(hex::encode(key(&-1i8)), "ff");
        assert_eq!(hex::encode(key(&-2i32)), "feffffff");
        assert_eq!(hex::encode(key(&-2i64)), "feffffffffffffff");
        assert_eq!(hex::encode(key(&1u128)), format!("01{}", "00".repeat(15)));
        assert_eq!(hex::encode(key(&-1i128)), "ff".repeat(16));
        assert_eq!(
            hex::encode(key(&serde_bytes_like())),
            "0200000000000000abcd"
        );
    }

    /// A type that calls `serialize_bytes`, as `serde_bytes` does.
    fn serde_bytes_like() -> impl Serialize {
        struct Bytes;
        impl Serialize for Bytes {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_bytes(&[0xab, 0xcd])
            }
        }
        Bytes
    }

    /// Structs, tuple structs and newtype structs write their fields in order,
    /// with no names and no length.
    #[xmtp_common::test(unwrap_try = true)]
    fn struct_shapes() {
        #[derive(Serialize)]
        struct Tuple(u8, u16);
        #[derive(Serialize)]
        struct Named {
            a: u8,
            b: Vec<u8>,
        }
        #[derive(Serialize)]
        struct Newtype(u32);
        assert_eq!(hex::encode(key(&Tuple(1, 0x0203))), "010302");
        assert_eq!(
            hex::encode(key(&Named { a: 1, b: vec![9] })),
            "01010000000000000009"
        );
        assert_eq!(hex::encode(key(&Newtype(5))), "05000000");
        assert_eq!(
            hex::encode(key(&(Newtype(5), Tuple(1, 0x0203)))),
            "05000000010302"
        );
    }

    /// A sequence whose length is unknown up front, as from a filtered iterator.
    struct UnknownLength;
    impl Serialize for UnknownLength {
        fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            s.collect_seq([1u8, 2, 3].into_iter().filter(|b| *b != 2))
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn enum_variants_and_unknown_lengths_are_errors() {
        #[derive(Serialize)]
        enum Variant {
            Newtype(u8),
            Tuple(u8, u8),
            Struct { a: u8 },
        }
        for variant in [
            Variant::Newtype(1),
            Variant::Tuple(1, 2),
            Variant::Struct { a: 1 },
        ] {
            assert!(matches!(to_vec(&variant), Err(KeyError::Unsupported(_))));
        }
        assert!(matches!(
            to_vec(&UnknownLength),
            Err(KeyError::Unsupported(_))
        ));
        assert!(matches!(to_vec(&1.5f32), Err(KeyError::Unsupported(_))));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn other_shapes_are_errors() {
        #[derive(Serialize)]
        enum Kind {
            A,
        }
        #[derive(Serialize)]
        struct Unit;
        assert!(matches!(to_vec(&true), Err(KeyError::Unsupported(_))));
        assert!(matches!(to_vec("text"), Err(KeyError::Unsupported(_))));
        assert!(matches!(to_vec(&1.5f64), Err(KeyError::Unsupported(_))));
        assert!(matches!(to_vec(&'c'), Err(KeyError::Unsupported(_))));
        assert!(matches!(to_vec(&Some(1u8)), Err(KeyError::Unsupported(_))));
        assert!(matches!(to_vec(&None::<u8>), Err(KeyError::Unsupported(_))));
        assert!(matches!(to_vec(&()), Err(KeyError::Unsupported(_))));
        assert!(matches!(to_vec(&Unit), Err(KeyError::Unsupported(_))));
        assert!(matches!(to_vec(&Kind::A), Err(KeyError::Unsupported(_))));
        let map = std::collections::BTreeMap::from([(1u8, 2u8)]);
        assert!(matches!(to_vec(&map), Err(KeyError::Unsupported(_))));
        // A shape nested in a supported one is still an error.
        assert!(matches!(
            to_vec(&(GroupId::from_slice(b"g"), true)),
            Err(KeyError::Unsupported(_))
        ));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn decode_bytes_reads_one_length_prefixed_string() {
        let encoded = key(&hash_ref(&[0x77; 32]));
        assert_eq!(decode_bytes(&encoded), Some([0x77; 32].as_slice()));
        assert_eq!(decode_bytes(&length_prefixed(0, 0)), Some([].as_slice()));
        // Too short, too long, and a short length field are not keys.
        assert_eq!(decode_bytes(&encoded[..encoded.len() - 1]), None);
        assert_eq!(decode_bytes(&[encoded.as_slice(), &[0]].concat()), None);
        assert_eq!(decode_bytes(&[1, 0, 0]), None);
    }
}
