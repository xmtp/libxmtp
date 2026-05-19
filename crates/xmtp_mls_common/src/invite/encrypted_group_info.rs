//! Encryption/decryption helpers for GroupInfo blobs stored on an external
//! service as part of the QR-invite flow. The blob envelope is the proto
//! [`EncryptedGroupInfoBlob`]; the underlying AEAD is ChaCha20Poly1305 via
//! [`payload_encryption::wrap_payload_symmetric`].
//!
//! `wrap_group_info` generates a fresh nonce for every call — callers MUST
//! NOT reuse a `(key, nonce)` pair across distinct ciphertexts (the AEAD
//! security argument collapses otherwise). The caller-supplied-nonce variant
//! [`wrap_group_info_with_nonce`] is provided for tests and explicit nonce
//! management scenarios only.
//!
//! The blob's plaintext metadata (epoch, group_state_hash, expires_at_ns)
//! is supplied by the caller because computing it requires the live
//! `MlsGroup` (epoch + tree hash) and an admin policy decision (expiry).
//! These fields are not derivable inside the pure-codec helpers.
//!
//! [`payload_encryption::wrap_payload_symmetric`]: crate::mls_ext::payload_encryption::wrap_payload_symmetric
//! [`EncryptedGroupInfoBlob`]: xmtp_proto::xmtp::mls::message_contents::EncryptedGroupInfoBlob

use thiserror::Error;
use xmtp_proto::xmtp::mls::message_contents::{
    EncryptedGroupInfoBlob, EncryptedGroupInfoBlobV1,
    encrypted_group_info_blob::Version as EncryptedGroupInfoBlobVersion,
};

use crate::invite::payload::NONCE_LEN;
use crate::mls_ext::payload_encryption::{
    UnwrapPayloadError, WrapPayloadError, unwrap_payload_symmetric, wrap_payload_symmetric,
};

/// Errors returned by [`wrap_group_info`], [`wrap_group_info_with_nonce`], and
/// [`unwrap_group_info`].
#[derive(Debug, Error)]
pub enum EncryptedGroupInfoError {
    /// The blob's `version` oneof carries a variant this build does not
    /// recognize, or is unset entirely.
    #[error("unsupported or missing EncryptedGroupInfoBlob version")]
    UnsupportedVersion,
    /// The blob's `nonce` field had a length other than [`NONCE_LEN`] bytes.
    #[error("nonce must be exactly {NONCE_LEN} bytes (got {0})")]
    InvalidNonceLength(usize),
    /// The blob's `expires_at_ns` is non-zero and `<= now_ns`.
    #[error("blob expired at {expires_at_ns} ns; current time {now_ns} ns")]
    Expired {
        /// Wall-clock expiry encoded in the blob.
        expires_at_ns: u64,
        /// Wall-clock time the caller used for the check.
        now_ns: u64,
    },
    /// The underlying AEAD wrap step failed.
    #[error("wrap failed: {0}")]
    Wrap(#[from] WrapPayloadError),
    /// The underlying AEAD unwrap step failed (wrong key, tampered ciphertext,
    /// etc.).
    #[error("unwrap failed: {0}")]
    Unwrap(#[from] UnwrapPayloadError),
}

/// Wrap plaintext bytes (a TLS-serialized `MlsMessageOut(GroupInfo)`) into an
/// [`EncryptedGroupInfoBlob`] using the provided symmetric key and a freshly
/// generated nonce (ChaCha20Poly1305 nonce-uniqueness requirement).
///
/// * `epoch` — current MLS epoch of the GroupInfo being wrapped. Service
///   uses for total-order on uploads.
/// * `group_state_hash` — tree hash (or equivalent group-state digest) of
///   the wrapped GroupInfo. Service uses for fork detection at equal epoch.
/// * `expires_at_ns` — wall-clock blob expiry. `0` means no expiry.
///
/// [`EncryptedGroupInfoBlob`]: xmtp_proto::xmtp::mls::message_contents::EncryptedGroupInfoBlob
pub fn wrap_group_info(
    plaintext: &[u8],
    key: &[u8; 32],
    epoch: u64,
    group_state_hash: Vec<u8>,
    expires_at_ns: u64,
) -> Result<EncryptedGroupInfoBlob, EncryptedGroupInfoError> {
    let nonce = crate::invite::payload::generate_nonce();
    wrap_group_info_with_nonce(
        plaintext,
        key,
        &nonce,
        epoch,
        group_state_hash,
        expires_at_ns,
    )
}

/// Same as [`wrap_group_info`] but with a caller-supplied nonce. Use only when
/// explicit nonce management is required (e.g. tests). Reusing a `(key,
/// nonce)` pair across distinct ciphertexts breaks ChaCha20Poly1305's security
/// argument.
pub fn wrap_group_info_with_nonce(
    plaintext: &[u8],
    key: &[u8; 32],
    nonce: &[u8; NONCE_LEN],
    epoch: u64,
    group_state_hash: Vec<u8>,
    expires_at_ns: u64,
) -> Result<EncryptedGroupInfoBlob, EncryptedGroupInfoError> {
    let ciphertext = wrap_payload_symmetric(
        plaintext,
        openmls::prelude::AeadType::ChaCha20Poly1305,
        key,
        nonce,
    )?;

    Ok(EncryptedGroupInfoBlob {
        version: Some(EncryptedGroupInfoBlobVersion::V1(
            EncryptedGroupInfoBlobV1 {
                nonce: nonce.to_vec(),
                ciphertext,
                epoch,
                group_state_hash,
                expires_at_ns,
            },
        )),
    })
}

/// Unwrap an [`EncryptedGroupInfoBlob`] using the symmetric key. Verifies the
/// envelope version, nonce length, and (when `now_ns` is `Some`) wall-clock
/// expiry before decryption.
///
/// Returns the plaintext + a borrowed reference to the unwrapped V1 envelope
/// so callers can inspect `epoch` / `group_state_hash` after decryption.
///
/// * [`EncryptedGroupInfoError::UnsupportedVersion`] for an unset or
///   unrecognised version oneof.
/// * [`EncryptedGroupInfoError::InvalidNonceLength`] if `nonce.len() != NONCE_LEN`.
/// * [`EncryptedGroupInfoError::Expired`] when `now_ns` is supplied and the
///   blob's `expires_at_ns` is non-zero and `<= now_ns`.
/// * [`EncryptedGroupInfoError::Unwrap`] for any AEAD-level failure.
pub fn unwrap_group_info<'a>(
    blob: &'a EncryptedGroupInfoBlob,
    key: &[u8; 32],
    now_ns: Option<u64>,
) -> Result<(Vec<u8>, &'a EncryptedGroupInfoBlobV1), EncryptedGroupInfoError> {
    let v1 = match &blob.version {
        Some(EncryptedGroupInfoBlobVersion::V1(v1)) => v1,
        None => return Err(EncryptedGroupInfoError::UnsupportedVersion),
    };
    if v1.nonce.len() != NONCE_LEN {
        return Err(EncryptedGroupInfoError::InvalidNonceLength(v1.nonce.len()));
    }
    if let Some(now) = now_ns
        && v1.expires_at_ns != 0
        && now >= v1.expires_at_ns
    {
        return Err(EncryptedGroupInfoError::Expired {
            expires_at_ns: v1.expires_at_ns,
            now_ns: now,
        });
    }

    let plaintext = unwrap_payload_symmetric(
        &v1.ciphertext,
        openmls::prelude::AeadType::ChaCha20Poly1305,
        key,
        &v1.nonce,
    )?;
    Ok((plaintext, v1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_wrap(key: &[u8; 32], plaintext: &[u8]) -> EncryptedGroupInfoBlob {
        wrap_group_info(plaintext, key, 1, b"state-hash".to_vec(), 0).unwrap()
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn round_trip_default_nonce() {
        let key = [0x11u8; 32];
        let plaintext = b"the quick brown fox jumps over the lazy dog";

        let blob = fixture_wrap(&key, plaintext);
        let (recovered, v1) = unwrap_group_info(&blob, &key, None)?;
        assert_eq!(recovered.as_slice(), plaintext.as_slice());
        assert_eq!(v1.epoch, 1);
        assert_eq!(v1.group_state_hash, b"state-hash");
        assert_eq!(v1.expires_at_ns, 0);
        assert_eq!(v1.nonce.len(), NONCE_LEN);
        assert_ne!(v1.ciphertext.as_slice(), plaintext.as_slice());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn round_trip_explicit_nonce() {
        let key = [0x22u8; 32];
        let nonce = [0x33u8; NONCE_LEN];
        let plaintext = b"explicit nonce path";

        let blob = wrap_group_info_with_nonce(plaintext, &key, &nonce, 7, vec![], 0)?;
        let (recovered, v1) = unwrap_group_info(&blob, &key, None)?;
        assert_eq!(v1.nonce.as_slice(), nonce.as_slice());
        assert_eq!(v1.epoch, 7);
        assert_eq!(recovered.as_slice(), plaintext.as_slice());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn missing_version_rejected() {
        let blob = EncryptedGroupInfoBlob { version: None };
        let err = unwrap_group_info(&blob, &[0u8; 32], None).unwrap_err();
        assert!(matches!(err, EncryptedGroupInfoError::UnsupportedVersion));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn nonce_too_short_rejected() {
        let key = [0x55u8; 32];
        let plaintext = b"short-nonce payload";

        let mut blob = fixture_wrap(&key, plaintext);
        if let Some(EncryptedGroupInfoBlobVersion::V1(ref mut v1)) = blob.version {
            v1.nonce.truncate(NONCE_LEN - 1);
        }
        let err = unwrap_group_info(&blob, &key, None).unwrap_err();
        match err {
            EncryptedGroupInfoError::InvalidNonceLength(len) => {
                assert_eq!(len, NONCE_LEN - 1);
            }
            other => panic!("expected InvalidNonceLength, got {other:?}"),
        }

        let mut blob = fixture_wrap(&key, plaintext);
        if let Some(EncryptedGroupInfoBlobVersion::V1(ref mut v1)) = blob.version {
            v1.nonce.push(0);
        }
        let err = unwrap_group_info(&blob, &key, None).unwrap_err();
        match err {
            EncryptedGroupInfoError::InvalidNonceLength(len) => {
                assert_eq!(len, NONCE_LEN + 1);
            }
            other => panic!("expected InvalidNonceLength, got {other:?}"),
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn wrong_key_fails_unwrap() {
        let key_a = [0x66u8; 32];
        let key_b = [0x77u8; 32];
        let plaintext = b"key A wrote me";

        let blob = fixture_wrap(&key_a, plaintext);
        let err = unwrap_group_info(&blob, &key_b, None).unwrap_err();
        assert!(
            matches!(err, EncryptedGroupInfoError::Unwrap(_)),
            "expected Unwrap, got {err:?}"
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn tampered_ciphertext_fails_unwrap() {
        let key = [0x88u8; 32];
        let plaintext = b"do not tamper with me, monkey";

        let mut blob = fixture_wrap(&key, plaintext);
        if let Some(EncryptedGroupInfoBlobVersion::V1(ref mut v1)) = blob.version {
            assert!(!v1.ciphertext.is_empty());
            v1.ciphertext[0] ^= 0x01;
        }
        let err = unwrap_group_info(&blob, &key, None).unwrap_err();
        assert!(
            matches!(err, EncryptedGroupInfoError::Unwrap(_)),
            "expected Unwrap, got {err:?}"
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn fresh_nonces_differ() {
        let key = [0x99u8; 32];
        let plaintext = b"same plaintext, different nonces please";

        let blob1 = fixture_wrap(&key, plaintext);
        let blob2 = fixture_wrap(&key, plaintext);

        let (_, v1_a) = unwrap_group_info(&blob1, &key, None)?;
        let (_, v1_b) = unwrap_group_info(&blob2, &key, None)?;
        assert_ne!(v1_a.nonce, v1_b.nonce, "fresh nonces must differ");
        assert_ne!(
            v1_a.ciphertext, v1_b.ciphertext,
            "ciphertexts must differ when nonces differ"
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn expired_blob_rejected_when_now_supplied() {
        let key = [0xaau8; 32];
        let plaintext = b"expires at 100";
        let blob = wrap_group_info(plaintext, &key, 1, vec![], 100)?;

        // Not yet expired.
        assert!(unwrap_group_info(&blob, &key, Some(99)).is_ok());

        // At and after expiry.
        for now in [100u64, 101, u64::MAX] {
            let err = unwrap_group_info(&blob, &key, Some(now)).unwrap_err();
            match err {
                EncryptedGroupInfoError::Expired {
                    expires_at_ns,
                    now_ns,
                } => {
                    assert_eq!(expires_at_ns, 100);
                    assert_eq!(now_ns, now);
                }
                other => panic!("expected Expired, got {other:?}"),
            }
        }

        // None bypasses expiry enforcement entirely.
        assert!(unwrap_group_info(&blob, &key, None).is_ok());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn zero_expiry_means_no_expiry() {
        let key = [0xbbu8; 32];
        let plaintext = b"never expires";
        let blob = wrap_group_info(plaintext, &key, 1, vec![], 0)?;

        // Even with a `now_ns` supplied, an explicit zero expiry never fails.
        assert!(unwrap_group_info(&blob, &key, Some(0)).is_ok());
        assert!(unwrap_group_info(&blob, &key, Some(u64::MAX)).is_ok());
    }
}
