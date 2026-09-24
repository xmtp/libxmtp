#![deny(missing_docs)]

//! A versioned, type-safe wrapper around a raw XMTP inbox id.
//!
//! ## Why a newtype instead of `[u8; 32]`?
//!
//! Plenty of 32-byte values fly around the codebase — installation ids,
//! commit hashes, TLS key hashes. Using `[u8; 32]` for inbox ids loses
//! the compiler's ability to catch accidental mixing, and forces every
//! site that serializes an inbox id to re-derive the wire format from
//! first principles.
//!
//! [`InboxId`] replaces the raw array everywhere an inbox id flows
//! through the AppData-dictionary wire format.
//!
//! ## Wire format
//!
//! `varint(version) || version_specific_payload`
//!
//! The version uses the QUIC variable-length integer encoding from RFC
//! 9000 §16 — the same scheme TLS-codec uses for collection length
//! prefixes. Version 0 fits in a single byte (`0x00`), so v0 `InboxId`
//! values are **33 bytes on the wire** (1 varint + 32 raw bytes).
//!
//! - **Version 0:** `32` raw bytes — the SHA-256 hash backing the
//!   hex-encoded string form (see `xmtp_id::associations::member::inbox_id`).
//!
//! Future versions can encode completely different payload shapes
//! (longer ids, different cryptographic schemes, …) without disturbing
//! on-the-wire compatibility of existing values.

use std::io::{Read, Write};

use tls_codec::{Deserialize, Serialize, Size};

/// Length in raw bytes of a v0 XMTP inbox id (the SHA-256 hash that
/// backs the hex-encoded string form).
pub const INBOX_ID_BYTE_LEN: usize = 32;

/// Current wire-format version written by [`InboxId::tls_serialize`].
pub const INBOX_ID_VERSION: u64 = 0;

/// Errors surfaced by [`InboxId`] construction from hex strings or
/// `Vec<u8>`.
#[derive(Debug, thiserror::Error)]
pub enum InboxIdError {
    /// The input wasn't valid hex at all (non-hex characters, odd
    /// length, etc.).
    #[error("invalid inbox id (hex decode): {0}")]
    InvalidHex(#[from] hex::FromHexError),
    /// The input was valid hex (or raw bytes) but the decoded byte
    /// length didn't match [`INBOX_ID_BYTE_LEN`].
    #[error("invalid inbox id length: expected {expected}, got {actual}")]
    InvalidLength {
        /// Expected length in raw bytes ([`INBOX_ID_BYTE_LEN`]).
        expected: usize,
        /// Actual length the caller supplied.
        actual: usize,
    },
}

/// A type-safe XMTP inbox id.
///
/// Backed by a `[u8; 32]` (v0 wire format). On the wire it is encoded
/// as `varint(version) || payload`; see the module-level docs for the
/// full wire-format contract.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InboxId([u8; INBOX_ID_BYTE_LEN]);

impl InboxId {
    /// Wrap a raw 32-byte inbox id.
    #[inline]
    pub const fn from_bytes(bytes: [u8; INBOX_ID_BYTE_LEN]) -> Self {
        Self(bytes)
    }

    /// Borrow the raw 32-byte payload.
    #[inline]
    pub const fn as_bytes(&self) -> &[u8; INBOX_ID_BYTE_LEN] {
        &self.0
    }

    /// Consume the wrapper and return the raw 32-byte payload.
    #[inline]
    pub const fn into_bytes(self) -> [u8; INBOX_ID_BYTE_LEN] {
        self.0
    }

    /// Decode a 64-character hex inbox id string into an [`InboxId`].
    pub fn from_hex(s: &str) -> Result<Self, InboxIdError> {
        let raw = hex::decode(s)?;
        let bytes: [u8; INBOX_ID_BYTE_LEN] =
            raw.try_into()
                .map_err(|v: Vec<u8>| InboxIdError::InvalidLength {
                    expected: INBOX_ID_BYTE_LEN,
                    actual: v.len(),
                })?;
        Ok(Self(bytes))
    }

    /// Encode the raw bytes back to their canonical 64-character hex
    /// string form.
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

impl std::fmt::Debug for InboxId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("InboxId(")?;
        for byte in &self.0 {
            write!(f, "{byte:02x}")?;
        }
        f.write_str(")")
    }
}

impl std::fmt::Display for InboxId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for byte in &self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// Wire-format size of a v0 [`InboxId`].
///
/// The version prefix is a QUIC varint. For `INBOX_ID_VERSION = 0` it is
/// exactly one byte (`0x00`), so the total encoded length is fixed at
/// `1 + 32 = 33`. We pin this as a plain constant rather than reproduce
/// `tls_codec::quic_vec::length_encoding_bytes` (which isn't part of the
/// public API); the [`tests::size_matches_serialized_bytes`] proptest
/// guarantees the constant stays in sync with the actual serialized
/// bytes.
///
/// When a future [`INBOX_ID_VERSION`] no longer fits in a single
/// varint byte, this constant must be revisited.
const INBOX_ID_V0_SERIALIZED_LEN: usize = 1 + INBOX_ID_BYTE_LEN;

impl Size for InboxId {
    #[inline]
    fn tls_serialized_len(&self) -> usize {
        INBOX_ID_V0_SERIALIZED_LEN
    }
}

impl Serialize for InboxId {
    #[inline]
    fn tls_serialize<W: Write>(&self, writer: &mut W) -> Result<usize, tls_codec::Error> {
        // The version goes on the wire as a QUIC varint — the same
        // encoding tls_codec uses internally for collection length
        // prefixes. For version 0 this is exactly one byte (`0x00`).
        let v_len = tls_codec::vlen::write_length(writer, INBOX_ID_VERSION as usize)?;
        writer
            .write_all(&self.0)
            .map_err(|e| tls_codec::Error::EncodingError(e.to_string()))?;
        Ok(v_len + INBOX_ID_BYTE_LEN)
    }
}

impl Deserialize for InboxId {
    #[inline]
    fn tls_deserialize<R: Read>(bytes: &mut R) -> Result<Self, tls_codec::Error>
    where
        Self: Sized,
    {
        let (version, consumed) = tls_codec::vlen::read_length(bytes)?;
        // Two guards, belt-and-suspenders:
        //
        // 1. `consumed == 1` — for `INBOX_ID_VERSION = 0` the varint on the
        //    wire is exactly one byte (`0x00`). Any multi-byte varint is
        //    definitively not v0, regardless of the decoded numeric value.
        //    This closes a 32-bit-target concern: `read_length` returns
        //    `usize`, and a peer-sent QUIC varint encoding a value larger
        //    than `usize::MAX` on wasm32 could be truncated by the decoder
        //    to a value that happens to be 0. Checking the consumed byte
        //    count sidesteps any reliance on upstream overflow handling.
        //
        // 2. `version as u64 == INBOX_ID_VERSION` — the semantic check,
        //    kept as a defensive duplicate. The `as u64` is a widening
        //    cast (lossless), not the truncating cast it might look like.
        //
        // When a future `INBOX_ID_VERSION` no longer fits in a single
        // varint byte (i.e., > 0x3f), guard 1 must be updated in step
        // with the new constant.
        if consumed != 1 || version as u64 != INBOX_ID_VERSION {
            return Err(tls_codec::Error::DecodingError(format!(
                "unsupported InboxId version: varint={version}, bytes_consumed={consumed}"
            )));
        }
        let mut buf = [0u8; INBOX_ID_BYTE_LEN];
        bytes
            .read_exact(&mut buf)
            .map_err(|e| tls_codec::Error::DecodingError(e.to_string()))?;
        Ok(Self(buf))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use tls_codec::{Deserialize, Serialize};

    // --- Fixed-shape tests ---------------------------------------------------
    //
    // Pin the wire format concretely: a future refactor that accidentally
    // swaps the version byte, the byte order, or the payload length would
    // break these regardless of what random inputs the proptests generate.

    #[xmtp_common::test]
    fn test_tls_serialize_writes_version_prefix_then_payload() {
        let id = InboxId::from_bytes([0xAB; 32]);
        let bytes = id.tls_serialize_detached().unwrap();
        assert_eq!(bytes.len(), 33);
        // QUIC varint for 0 is a single `0x00` byte.
        assert_eq!(bytes[0], 0x00);
        assert_eq!(&bytes[1..], &[0xAB; 32]);
    }

    #[xmtp_common::test]
    fn test_from_hex_non_hex_input() {
        let err = InboxId::from_hex("not_hex").unwrap_err();
        assert!(matches!(err, InboxIdError::InvalidHex(_)));
    }

    /// A multi-byte QUIC varint that decodes to the value `0` must still be
    /// rejected: v0 is defined as a single-byte `0x00` varint, so a 2-byte
    /// form (`0x40 0x00`) or 4/8-byte forms are non-minimal encodings and
    /// not valid v0 on the wire. Without the `consumed == 1` guard, a
    /// 32-bit-target `read_length` implementation that truncates a large
    /// varint to `usize::MAX`-fits-mod-0 could spoof v0.
    ///
    /// Two rejection paths are accepted because tls_codec's own
    /// minimality check (`check_min_length`) only runs when the `mls`
    /// feature is enabled: with the feature on, tls_codec surfaces
    /// `InvalidVectorLength` before we see the decode; with it off, our
    /// `consumed == 1` guard surfaces `DecodingError`.
    #[xmtp_common::test]
    fn test_tls_deserialize_rejects_non_minimal_version_zero() {
        // 2-byte QUIC varint: prefix 0b01 → `0x40 0x00` decodes to 0.
        let mut bytes = vec![0x40, 0x00];
        bytes.extend_from_slice(&[0xAB; INBOX_ID_BYTE_LEN]);
        let err = InboxId::tls_deserialize_exact(&bytes).unwrap_err();
        assert!(
            matches!(
                err,
                tls_codec::Error::DecodingError(_) | tls_codec::Error::InvalidVectorLength
            ),
            "got {err:?}"
        );
    }

    // --- Property tests ------------------------------------------------------

    proptest! {
        /// Round-tripping through hex is lossless for every 32-byte input.
        #[test]
        fn hex_round_trip(raw in any::<[u8; INBOX_ID_BYTE_LEN]>()) {
            let id = InboxId::from_bytes(raw);
            let hex = id.to_hex();
            prop_assert_eq!(hex.len(), 2 * INBOX_ID_BYTE_LEN);
            let back = InboxId::from_hex(&hex).unwrap();
            prop_assert_eq!(id, back);
        }

        /// Round-tripping through TLS codec is lossless for every 32-byte input.
        #[test]
        fn tls_round_trip(raw in any::<[u8; INBOX_ID_BYTE_LEN]>()) {
            let id = InboxId::from_bytes(raw);
            let bytes = id.tls_serialize_detached().unwrap();
            let restored = InboxId::tls_deserialize_exact(&bytes).unwrap();
            prop_assert_eq!(id, restored);
        }

        /// `tls_serialized_len` matches the actual serialized byte count.
        /// Pins the hardcoded [`INBOX_ID_V0_SERIALIZED_LEN`] against the
        /// real encoding — if tls_codec's varint sizing ever changes, or
        /// a future version bumps the prefix to multi-byte, this catches
        /// the drift immediately.
        #[test]
        fn size_matches_serialized_bytes(raw in any::<[u8; INBOX_ID_BYTE_LEN]>()) {
            let id = InboxId::from_bytes(raw);
            let bytes = id.tls_serialize_detached().unwrap();
            prop_assert_eq!(bytes.len(), id.tls_serialized_len());
            prop_assert_eq!(bytes.len(), INBOX_ID_V0_SERIALIZED_LEN);
        }

        /// Hex-decoding any 64-char valid-hex string yields the same bytes
        /// as direct construction.
        #[test]
        fn hex_and_from_bytes_agree(raw in any::<[u8; INBOX_ID_BYTE_LEN]>()) {
            let via_bytes = InboxId::from_bytes(raw);
            let via_hex = InboxId::from_hex(&hex::encode(raw)).unwrap();
            prop_assert_eq!(via_bytes, via_hex);
            prop_assert_eq!(via_bytes.as_bytes(), &raw);
        }

        /// `InboxId` ordering follows lexicographic byte ordering — the
        /// contract TlsSet/TlsMap rely on for deterministic serialization.
        #[test]
        fn ord_matches_byte_order(
            a in any::<[u8; INBOX_ID_BYTE_LEN]>(),
            b in any::<[u8; INBOX_ID_BYTE_LEN]>(),
        ) {
            let ia = InboxId::from_bytes(a);
            let ib = InboxId::from_bytes(b);
            prop_assert_eq!(ia.cmp(&ib), a.cmp(&b));
        }

        /// Any hex string whose decoded byte length isn't 32 is rejected
        /// with [`InboxIdError::InvalidLength`]. The even-length constraint
        /// on `len` keeps the hex string valid — we're exercising the
        /// length check, not the hex parser.
        #[test]
        fn from_hex_rejects_wrong_length(byte_len in (0usize..=64).prop_filter(
            "byte_len must be != INBOX_ID_BYTE_LEN",
            |n| *n != INBOX_ID_BYTE_LEN,
        )) {
            // 2 hex chars per byte — `byte_len` bytes of hex input.
            let s = "ab".repeat(byte_len);
            let err = InboxId::from_hex(&s).unwrap_err();
            let matched = match err {
                InboxIdError::InvalidLength { expected, actual } =>
                    expected == INBOX_ID_BYTE_LEN && actual == byte_len,
                _ => false,
            };
            prop_assert!(matched);
        }

        /// Any varint-encoded version byte other than 0 is rejected by the
        /// deserializer. Sweeping the single-byte varint range (0..=0x3f)
        /// and skipping 0 covers every v0 rejection path reachable without
        /// varint-encoder gymnastics.
        #[test]
        fn tls_deserialize_rejects_unsupported_version(v in 1u8..=0x3f) {
            let mut bytes = vec![v];
            bytes.extend_from_slice(&[0xFF; INBOX_ID_BYTE_LEN]);
            let err = InboxId::tls_deserialize_exact(&bytes).unwrap_err();
            prop_assert!(matches!(err, tls_codec::Error::DecodingError(_)));
        }

        /// Debug and Display both produce exactly the hex form with no
        /// trailing allocation-smells — and Debug wraps Display inside
        /// `InboxId(...)`.
        #[test]
        fn formatting_is_hex(raw in any::<[u8; INBOX_ID_BYTE_LEN]>()) {
            let id = InboxId::from_bytes(raw);
            let displayed = format!("{id}");
            let debugged = format!("{id:?}");
            prop_assert_eq!(displayed.clone(), hex::encode(raw));
            prop_assert_eq!(debugged, format!("InboxId({displayed})"));
        }

        /// `TlsSet<InboxId>` round-trips through tls_codec for any set of
        /// random 32-byte entries.
        #[test]
        fn tls_set_round_trip(
            raws in proptest::collection::btree_set(
                any::<[u8; INBOX_ID_BYTE_LEN]>(),
                0..16,
            ),
        ) {
            use crate::tls_set::TlsSet;

            let set: TlsSet<InboxId> =
                raws.iter().copied().map(InboxId::from_bytes).collect();
            let bytes = set.tls_serialize_detached().unwrap();
            let restored = TlsSet::<InboxId>::tls_deserialize_exact(&bytes).unwrap();
            prop_assert_eq!(set, restored);
        }
    }
}
