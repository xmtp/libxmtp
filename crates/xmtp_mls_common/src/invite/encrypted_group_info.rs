//! Encryption/decryption helpers for GroupInfo blobs stored on an external
//! service as part of the QR-invite flow. The blob envelope is the proto
//! [`EncryptedGroupInfoBlob`]; the underlying AEAD is ChaCha20Poly1305 via
//! [`payload_encryption::wrap_payload_symmetric`].
//!
//! [`wrap_group_info`] is a builder that generates a fresh nonce for every
//! call by default — callers MUST NOT reuse a `(key, nonce)` pair across
//! distinct ciphertexts (the AEAD security argument collapses otherwise).
//! Tests and explicit nonce management scenarios may pass `.nonce(...)` to
//! override.
//!
//! The blob's cleartext metadata (epoch, group_state_hash, expires_at_ns)
//! is supplied by the caller because computing it requires the live
//! `MlsGroup` (epoch + tree hash) and an admin policy decision (expiry).
//! These fields are not derivable inside the pure-codec helpers. They travel
//! in the clear but are bound into the AEAD as associated data (see
//! [`blob_aad`]), so tampering with any of them — e.g. resetting
//! `expires_at_ns` to `0` to defeat expiry — is rejected at unwrap instead of
//! being silently trusted.
//!
//! [`payload_encryption::wrap_payload_symmetric`]: crate::mls_ext::payload_encryption::wrap_payload_symmetric
//! [`EncryptedGroupInfoBlob`]: xmtp_proto::xmtp::mls::message_contents::EncryptedGroupInfoBlob

use thiserror::Error;
use xmtp_proto::xmtp::mls::message_contents::{
    EncryptedGroupInfoBlob, EncryptedGroupInfoBlobV1, GroupStateHash,
    encrypted_group_info_blob::Version as EncryptedGroupInfoBlobVersion,
};

use crate::invite::payload::NONCE_LEN;
use crate::mls_ext::payload_encryption::{
    UnwrapPayloadError, WrapPayloadError, unwrap_payload_symmetric, wrap_payload_symmetric,
};

/// Length in bytes of `GroupStateHash.digest`: the output length of the hash
/// bound to the group's MLS ciphersuite (32 bytes under XMTP's current
/// ciphersuite). The submessage does not constrain length, so wrap and
/// unwrap both enforce it here.
pub const GROUP_STATE_HASH_LEN: usize = 32;

/// Errors returned by [`wrap_group_info`] and [`unwrap_group_info`].
#[derive(Debug, Error)]
pub enum EncryptedGroupInfoError {
    /// The blob's `version` oneof carries a variant this build does not
    /// recognize, or is unset entirely.
    #[error("unsupported or missing EncryptedGroupInfoBlob version")]
    UnsupportedVersion,
    /// The blob's `nonce` field had a length other than [`NONCE_LEN`] bytes.
    #[error("nonce must be exactly {NONCE_LEN} bytes (got {0})")]
    InvalidNonceLength(usize),
    /// The blob's `group_state_hash` submessage was absent.
    #[error("group_state_hash is required on an EncryptedGroupInfoBlob")]
    MissingGroupStateHash,
    /// `group_state_hash.digest` had a length other than
    /// [`GROUP_STATE_HASH_LEN`] bytes.
    #[error("group_state_hash.digest must be exactly {GROUP_STATE_HASH_LEN} bytes (got {0})")]
    InvalidGroupStateHashLength(usize),
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

/// Canonical associated-data encoding binding the blob's cleartext metadata
/// (`epoch`, `expires_at_ns`, `group_state_hash`) to the ciphertext. The same
/// bytes are fed to the AEAD at wrap and unwrap time, so tampering with any of
/// these envelope fields makes [`unwrap_group_info`] reject the blob.
///
/// The layout is pinned by the proto / XIP-82: `epoch` and `expires_at_ns` as
/// 8-byte big-endian, then the digest bytes. The fixed-width fields come
/// first and the digest is the remainder, so the encoding is unambiguous
/// without a length prefix (digest length is itself enforced to
/// [`GROUP_STATE_HASH_LEN`]).
fn blob_aad(epoch: u64, expires_at_ns: u64, group_state_hash: &[u8]) -> Vec<u8> {
    let mut aad = Vec::with_capacity(8 + 8 + group_state_hash.len());
    aad.extend_from_slice(&epoch.to_be_bytes());
    aad.extend_from_slice(&expires_at_ns.to_be_bytes());
    aad.extend_from_slice(group_state_hash);
    aad
}

/// Compute a blob's **effective** `expires_at_ns` from the two policy bounds
/// that apply at wrap time: the earlier of the policy's absolute campaign
/// expiry (`policy_expires_at_ns`) and the staleness deadline
/// (`epoch_began_at_ns + expire_in_ns`, saturating). A bound of `0` means
/// "no bound" and drops out of the min; the result is `0` only when neither
/// bound is set.
///
/// Folding the staleness bound into the blob's single expiry field means the
/// joiner's one expiry check also skips candidates that validators would
/// reject as stale (no "zombie joins"), and the service's TTL-based GC
/// naturally collects staleness-dead blobs.
///
/// * `policy_expires_at_ns` — `EXTERNAL_COMMIT_POLICY.expires_at_ns`.
/// * `epoch_began_at_ns` — delivery-service envelope timestamp of the commit
///   that began the wrapped GroupInfo's epoch.
/// * `expire_in_ns` — `EXTERNAL_COMMIT_POLICY.expire_in_ns`.
pub fn effective_expires_at_ns(
    policy_expires_at_ns: u64,
    epoch_began_at_ns: u64,
    expire_in_ns: u64,
) -> u64 {
    let staleness_deadline = if expire_in_ns == 0 {
        0
    } else {
        epoch_began_at_ns.saturating_add(expire_in_ns)
    };
    match (policy_expires_at_ns, staleness_deadline) {
        (0, deadline) => deadline,
        (campaign, 0) => campaign,
        (campaign, deadline) => campaign.min(deadline),
    }
}

/// Wrap plaintext bytes (a TLS-serialized `MlsMessageOut(GroupInfo)`) into an
/// [`EncryptedGroupInfoBlob`] using the provided symmetric key.
///
/// `epoch` and `group_state_hash` are required envelope metadata; both are
/// `u64`s by-name to make a swap with `expires_at_ns` a builder-method error
/// rather than a silent positional mistake. `nonce` defaults to a fresh
/// CSPRNG-generated nonce per call (the ChaCha20Poly1305 nonce-uniqueness
/// requirement); tests with deterministic-nonce needs may override via
/// `.nonce(...)`. `expires_at_ns` defaults to `0` (no expiry).
///
/// * `epoch` — current MLS epoch of the GroupInfo being wrapped. Joiner-side
///   metadata (prefer the freshest candidate; consistency-check the decrypted
///   GroupInfo) — a conformant service never orders or evicts by it.
/// * `group_state_hash` — digest of the wrapped GroupInfo's epoch state
///   (`digest(GroupContext)` under the group's ciphersuite); exactly
///   [`GROUP_STATE_HASH_LEN`] bytes. The joiner verifies it against the
///   decrypted GroupInfo; a member uses it to recognise an idempotent
///   re-upload.
/// * `expires_at_ns` — the blob's *effective* wall-clock expiry. `0` means no
///   expiry. Callers fold the policy's staleness bound in via
///   [`effective_expires_at_ns`].
/// * `nonce` — explicit nonce. ChaCha20Poly1305 security requires that no
///   `(key, nonce)` pair ever encrypts two distinct ciphertexts. Default
///   behavior calls [`crate::invite::payload::generate_nonce`] per call.
///
/// # Example
///
/// ```ignore
/// // Default usage — fresh nonce, no expiry:
/// let blob = wrap_group_info()
///     .plaintext(&group_info_bytes)
///     .key(&symmetric_key)
///     .epoch(group.epoch())
///     .group_state_hash(group.epoch_authenticator()?.to_vec())
///     .call()?;
///
/// // With expiry + deterministic nonce (tests):
/// let blob = wrap_group_info()
///     .plaintext(&group_info_bytes)
///     .key(&symmetric_key)
///     .nonce(deterministic_nonce)
///     .epoch(7)
///     .group_state_hash(state_hash)
///     .expires_at_ns(deadline_ns)
///     .call()?;
/// ```
///
/// [`EncryptedGroupInfoBlob`]: xmtp_proto::xmtp::mls::message_contents::EncryptedGroupInfoBlob
#[bon::builder]
pub fn wrap_group_info(
    plaintext: &[u8],
    key: &[u8; 32],
    #[builder(default = crate::invite::payload::generate_nonce())] nonce: [u8; NONCE_LEN],
    epoch: u64,
    group_state_hash: Vec<u8>,
    #[builder(default = 0)] expires_at_ns: u64,
) -> Result<EncryptedGroupInfoBlob, EncryptedGroupInfoError> {
    if group_state_hash.len() != GROUP_STATE_HASH_LEN {
        return Err(EncryptedGroupInfoError::InvalidGroupStateHashLength(
            group_state_hash.len(),
        ));
    }
    let aad = blob_aad(epoch, expires_at_ns, &group_state_hash);
    let ciphertext = wrap_payload_symmetric()
        .data(plaintext)
        .aead_type(openmls::prelude::AeadType::ChaCha20Poly1305)
        .symmetric_key(key)
        .nonce(&nonce)
        .aad(&aad)
        .call()?;

    Ok(EncryptedGroupInfoBlob {
        version: Some(EncryptedGroupInfoBlobVersion::V1(
            EncryptedGroupInfoBlobV1 {
                nonce: nonce.to_vec(),
                ciphertext,
                epoch,
                group_state_hash: Some(GroupStateHash {
                    digest: group_state_hash,
                }),
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
    let digest = &v1
        .group_state_hash
        .as_ref()
        .ok_or(EncryptedGroupInfoError::MissingGroupStateHash)?
        .digest;
    if digest.len() != GROUP_STATE_HASH_LEN {
        return Err(EncryptedGroupInfoError::InvalidGroupStateHashLength(
            digest.len(),
        ));
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

    let aad = blob_aad(v1.epoch, v1.expires_at_ns, digest);
    let plaintext = unwrap_payload_symmetric()
        .data(&v1.ciphertext)
        .aead_type(openmls::prelude::AeadType::ChaCha20Poly1305)
        .symmetric_key(key)
        .nonce(&v1.nonce)
        .aad(&aad)
        .call()?;
    Ok((plaintext, v1))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic 32-byte digest fixture.
    fn digest(fill: u8) -> Vec<u8> {
        vec![fill; GROUP_STATE_HASH_LEN]
    }

    fn fixture_wrap(key: &[u8; 32], plaintext: &[u8]) -> EncryptedGroupInfoBlob {
        wrap_group_info()
            .plaintext(plaintext)
            .key(key)
            .epoch(1)
            .group_state_hash(digest(0xD1))
            .call()
            .unwrap()
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn round_trip_default_nonce() {
        let key = [0x11u8; 32];
        let plaintext = b"the quick brown fox jumps over the lazy dog";

        let blob = fixture_wrap(&key, plaintext);
        let (recovered, v1) = unwrap_group_info(&blob, &key, None)?;
        assert_eq!(recovered.as_slice(), plaintext.as_slice());
        assert_eq!(v1.epoch, 1);
        assert_eq!(
            v1.group_state_hash,
            Some(GroupStateHash {
                digest: digest(0xD1)
            })
        );
        assert_eq!(v1.expires_at_ns, 0);
        assert_eq!(v1.nonce.len(), NONCE_LEN);
        assert_ne!(v1.ciphertext.as_slice(), plaintext.as_slice());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn round_trip_explicit_nonce() {
        let key = [0x22u8; 32];
        let nonce = [0x33u8; NONCE_LEN];
        let plaintext = b"explicit nonce path";

        let blob = wrap_group_info()
            .plaintext(plaintext)
            .key(&key)
            .nonce(nonce)
            .epoch(7)
            .group_state_hash(digest(0xD2))
            .call()?;
        let (recovered, v1) = unwrap_group_info(&blob, &key, None)?;
        assert_eq!(v1.nonce.as_slice(), nonce.as_slice());
        assert_eq!(v1.epoch, 7);
        assert_eq!(recovered.as_slice(), plaintext.as_slice());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn wrap_rejects_wrong_digest_length() {
        let err = wrap_group_info()
            .plaintext(b"short digest")
            .key(&[0xc0u8; 32])
            .epoch(1)
            .group_state_hash(vec![0xD3; GROUP_STATE_HASH_LEN - 1])
            .call()
            .unwrap_err();
        assert!(
            matches!(
                err,
                EncryptedGroupInfoError::InvalidGroupStateHashLength(len)
                    if len == GROUP_STATE_HASH_LEN - 1
            ),
            "expected InvalidGroupStateHashLength, got {err:?}"
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn unwrap_rejects_missing_or_short_digest() {
        let key = [0xc4u8; 32];
        let mut blob = fixture_wrap(&key, b"digest checks");
        if let Some(EncryptedGroupInfoBlobVersion::V1(ref mut v1)) = blob.version {
            v1.group_state_hash = None;
        }
        let err = unwrap_group_info(&blob, &key, None).unwrap_err();
        assert!(matches!(
            err,
            EncryptedGroupInfoError::MissingGroupStateHash
        ));

        let mut blob = fixture_wrap(&key, b"digest checks");
        if let Some(EncryptedGroupInfoBlobVersion::V1(ref mut v1)) = blob.version {
            v1.group_state_hash = Some(GroupStateHash {
                digest: vec![0xD4; GROUP_STATE_HASH_LEN + 1],
            });
        }
        let err = unwrap_group_info(&blob, &key, None).unwrap_err();
        assert!(matches!(
            err,
            EncryptedGroupInfoError::InvalidGroupStateHashLength(len)
                if len == GROUP_STATE_HASH_LEN + 1
        ));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn effective_expiry_math() {
        // Neither bound set.
        assert_eq!(effective_expires_at_ns(0, 1_000, 0), 0);
        // Campaign bound only.
        assert_eq!(effective_expires_at_ns(5_000, 1_000, 0), 5_000);
        // Staleness bound only.
        assert_eq!(effective_expires_at_ns(0, 1_000, 250), 1_250);
        // Both: earlier wins, in either order.
        assert_eq!(effective_expires_at_ns(5_000, 1_000, 250), 1_250);
        assert_eq!(effective_expires_at_ns(1_100, 1_000, 250), 1_100);
        // Saturation: a huge expire_in_ns must not wrap around to a tiny
        // (already-passed) deadline.
        assert_eq!(
            effective_expires_at_ns(0, u64::MAX - 10, 250),
            u64::MAX,
            "staleness deadline saturates instead of wrapping"
        );
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
    fn tampered_epoch_fails_unwrap() {
        let key = [0xc1u8; 32];
        let mut blob = fixture_wrap(&key, b"epoch is authenticated");
        if let Some(EncryptedGroupInfoBlobVersion::V1(ref mut v1)) = blob.version {
            v1.epoch ^= 0xff;
        }
        let err = unwrap_group_info(&blob, &key, None).unwrap_err();
        assert!(
            matches!(err, EncryptedGroupInfoError::Unwrap(_)),
            "expected Unwrap, got {err:?}"
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn tampered_group_state_hash_fails_unwrap() {
        let key = [0xc2u8; 32];
        let mut blob = fixture_wrap(&key, b"state hash is authenticated");
        if let Some(EncryptedGroupInfoBlobVersion::V1(ref mut v1)) = blob.version
            && let Some(ref mut hash) = v1.group_state_hash
        {
            hash.digest[0] ^= 0x01;
        }
        let err = unwrap_group_info(&blob, &key, None).unwrap_err();
        assert!(
            matches!(err, EncryptedGroupInfoError::Unwrap(_)),
            "expected Unwrap, got {err:?}"
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn tampered_expiry_fails_unwrap() {
        let key = [0xc3u8; 32];
        // Wrapped with a real deadline...
        let mut blob = wrap_group_info()
            .plaintext(b"expiry is authenticated")
            .key(&key)
            .epoch(1)
            .group_state_hash(digest(0xD5))
            .expires_at_ns(100)
            .call()?;
        // ...which an attacker resets to 0 ("never expires") to bypass it.
        if let Some(EncryptedGroupInfoBlobVersion::V1(ref mut v1)) = blob.version {
            v1.expires_at_ns = 0;
        }
        // The pre-decryption expiry check now passes (0 = no expiry), but the AAD
        // no longer matches the original `expires_at_ns`, so unwrap rejects it.
        let err = unwrap_group_info(&blob, &key, Some(1_000)).unwrap_err();
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
        let blob = wrap_group_info()
            .plaintext(plaintext)
            .key(&key)
            .epoch(1)
            .group_state_hash(digest(0xD6))
            .expires_at_ns(100)
            .call()?;

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
        let blob = wrap_group_info()
            .plaintext(plaintext)
            .key(&key)
            .epoch(1)
            .group_state_hash(digest(0xD7))
            .call()?;

        // Even with a `now_ns` supplied, an explicit zero expiry never fails.
        assert!(unwrap_group_info(&blob, &key, Some(0)).is_ok());
        assert!(unwrap_group_info(&blob, &key, Some(u64::MAX)).is_ok());
    }
}
