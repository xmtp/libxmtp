//! Helpers for the [`ExternalInvitePayload`] proto.
//!
//! Centralises the small but easy-to-get-wrong pieces of building and
//! validating an external-invite payload:
//!
//! * fresh symmetric keys / nonces / external-group-ids from the workspace CSPRNG
//! * recognising / unwrapping the `oneof version { V1 v1 }` envelope
//! * a [`build_payload`] convenience constructor
//!
//! The actual encryption of the [`GroupInfo`] blob is performed by the
//! sibling `encrypted_group_info` module (which also owns the blob-side
//! expiry semantics, since `expires_at_ns` lives on the
//! [`EncryptedGroupInfoBlob`] envelope and not the payload).
//!
//! [`ExternalInvitePayload`]: xmtp_proto::xmtp::mls::message_contents::ExternalInvitePayload
//! [`EncryptedGroupInfoBlob`]: xmtp_proto::xmtp::mls::message_contents::EncryptedGroupInfoBlob
//! [`GroupInfo`]: openmls::messages::group_info::GroupInfo

use thiserror::Error;
use xmtp_proto::xmtp::mls::message_contents::{
    ExternalInvitePayload, ExternalInvitePayloadV1,
    external_invite_payload::Version as ExternalInvitePayloadVersion,
};

/// Length in bytes of the ChaCha20Poly1305 key used to wrap the encrypted
/// `GroupInfo` blob referenced by an [`ExternalInvitePayload`].
pub const SYMMETRIC_KEY_LEN: usize = 32;

/// Length in bytes of the ChaCha20Poly1305 nonce used alongside
/// [`SYMMETRIC_KEY_LEN`]-byte keys.
pub const NONCE_LEN: usize = 12;

/// Minimum length of `external_group_id`. The proto schema enforces this as
/// MUST; tiny services that don't need much collision resistance may pick
/// the floor, but `RECOMMENDED_EXTERNAL_GROUP_ID_LEN` random bytes is the
/// libxmtp default when no application-specific scheme is in use.
pub const MIN_EXTERNAL_GROUP_ID_LEN: usize = 4;

/// Recommended random length for `external_group_id` when callers don't
/// have an application-specific scheme. 16 bytes (128 bits) gives ample
/// collision resistance for any realistic single-service deployment.
pub const RECOMMENDED_EXTERNAL_GROUP_ID_LEN: usize = 16;

/// Errors returned when validating an [`ExternalInvitePayload`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum InvitePayloadError {
    /// The payload's `version` oneof carries a variant this build does not
    /// recognize, or is unset entirely.
    #[error("unsupported or missing external-invite payload version")]
    UnsupportedVersion,
    /// `external_group_id` was shorter than [`MIN_EXTERNAL_GROUP_ID_LEN`].
    #[error("external_group_id must be at least {min} bytes (got {len})", min = MIN_EXTERNAL_GROUP_ID_LEN)]
    InvalidExternalGroupIdLength {
        /// Observed length.
        len: usize,
    },
    /// `symmetric_key` was not exactly [`SYMMETRIC_KEY_LEN`] bytes.
    #[error("symmetric_key must be exactly {SYMMETRIC_KEY_LEN} bytes (got {0})")]
    InvalidSymmetricKeyLength(usize),
}

/// Generate a fresh 32-byte symmetric key from the workspace CSPRNG.
///
/// The key is intended for use with ChaCha20Poly1305 when wrapping the
/// encrypted GroupInfo blob referenced by the resulting
/// [`ExternalInvitePayload`].
pub fn generate_symmetric_key() -> [u8; SYMMETRIC_KEY_LEN] {
    xmtp_common::rand_array::<SYMMETRIC_KEY_LEN>()
}

/// Generate a fresh 12-byte nonce from the workspace CSPRNG.
///
/// Intended for use with ChaCha20Poly1305 alongside a key produced by
/// [`generate_symmetric_key`]. The nonce is *not* stored in the payload
/// itself — it lives next to the ciphertext in the encrypted GroupInfo blob.
pub fn generate_nonce() -> [u8; NONCE_LEN] {
    xmtp_common::rand_array::<NONCE_LEN>()
}

/// Generate a fresh random `external_group_id` of the recommended length
/// ([`RECOMMENDED_EXTERNAL_GROUP_ID_LEN`] bytes from the workspace CSPRNG).
///
/// Callers with application-specific identifier schemes (UUIDs, snowflakes,
/// short slot keys, …) should construct their own bytes instead — this
/// helper exists as the safe default.
pub fn generate_external_group_id() -> [u8; RECOMMENDED_EXTERNAL_GROUP_ID_LEN] {
    xmtp_common::rand_array::<RECOMMENDED_EXTERNAL_GROUP_ID_LEN>()
}

/// Validate that `payload.version` carries a recognised variant and that
/// the V1 fields meet their length requirements.
///
/// Currently the only recognised variant is V1. Future versions extend the
/// oneof; unknown variants are rejected (fail closed).
pub fn validate(
    payload: &ExternalInvitePayload,
) -> Result<&ExternalInvitePayloadV1, InvitePayloadError> {
    let v1 = match &payload.version {
        Some(ExternalInvitePayloadVersion::V1(v1)) => v1,
        None => return Err(InvitePayloadError::UnsupportedVersion),
    };
    if v1.external_group_id.len() < MIN_EXTERNAL_GROUP_ID_LEN {
        return Err(InvitePayloadError::InvalidExternalGroupIdLength {
            len: v1.external_group_id.len(),
        });
    }
    if v1.symmetric_key.len() != SYMMETRIC_KEY_LEN {
        return Err(InvitePayloadError::InvalidSymmetricKeyLength(
            v1.symmetric_key.len(),
        ));
    }
    Ok(v1)
}

/// Build an [`ExternalInvitePayload`] wrapping a [`ExternalInvitePayloadV1`]
/// with the supplied fields.
///
/// * `service_pointer` — application-defined opaque bytes describing where
///   the encrypted GroupInfo blob can be fetched.
/// * `external_group_id` — service-slot identifier carried on the wire and
///   verified by the joiner against the group's
///   `EXTERNAL_COMMIT_POLICY.external_group_id` after joining. MUST be at
///   least [`MIN_EXTERNAL_GROUP_ID_LEN`] bytes.
/// * `symmetric_key` — typically the output of [`generate_symmetric_key`].
pub fn build_payload(
    service_pointer: Vec<u8>,
    external_group_id: Vec<u8>,
    symmetric_key: [u8; SYMMETRIC_KEY_LEN],
) -> ExternalInvitePayload {
    ExternalInvitePayload {
        version: Some(ExternalInvitePayloadVersion::V1(ExternalInvitePayloadV1 {
            service_pointer,
            external_group_id,
            symmetric_key: symmetric_key.to_vec(),
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn well_formed_payload() -> ExternalInvitePayload {
        build_payload(
            b"https://invites.example/abc".to_vec(),
            generate_external_group_id().to_vec(),
            [0x42u8; SYMMETRIC_KEY_LEN],
        )
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn key_nonce_and_id_are_random() {
        let k1 = generate_symmetric_key();
        let k2 = generate_symmetric_key();
        assert_eq!(k1.len(), SYMMETRIC_KEY_LEN);
        assert_eq!(k2.len(), SYMMETRIC_KEY_LEN);
        assert_ne!(k1, k2, "two CSPRNG-generated keys must differ");
        assert_ne!(k1, [0u8; SYMMETRIC_KEY_LEN], "key must not be all-zero");

        let n1 = generate_nonce();
        let n2 = generate_nonce();
        assert_eq!(n1.len(), NONCE_LEN);
        assert_eq!(n2.len(), NONCE_LEN);
        assert_ne!(n1, n2, "two CSPRNG-generated nonces must differ");

        let id1 = generate_external_group_id();
        let id2 = generate_external_group_id();
        assert_eq!(id1.len(), RECOMMENDED_EXTERNAL_GROUP_ID_LEN);
        assert_ne!(
            id1, id2,
            "two CSPRNG-generated external_group_ids must differ"
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn validate_accepts_well_formed_v1() {
        let payload = well_formed_payload();
        let v1 = validate(&payload)?;
        assert_eq!(v1.symmetric_key.len(), SYMMETRIC_KEY_LEN);
        assert!(v1.external_group_id.len() >= MIN_EXTERNAL_GROUP_ID_LEN);
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn validate_rejects_missing_version() {
        let payload = ExternalInvitePayload { version: None };
        assert_eq!(
            validate(&payload),
            Err(InvitePayloadError::UnsupportedVersion)
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn validate_rejects_short_external_group_id() {
        let payload = build_payload(
            b"svc".to_vec(),
            vec![0u8; MIN_EXTERNAL_GROUP_ID_LEN - 1],
            [0x42u8; SYMMETRIC_KEY_LEN],
        );
        assert_eq!(
            validate(&payload),
            Err(InvitePayloadError::InvalidExternalGroupIdLength {
                len: MIN_EXTERNAL_GROUP_ID_LEN - 1
            })
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn validate_rejects_wrong_symmetric_key_length() {
        let mut payload = well_formed_payload();
        if let Some(ExternalInvitePayloadVersion::V1(ref mut v1)) = payload.version {
            v1.symmetric_key = vec![0u8; SYMMETRIC_KEY_LEN - 1];
        }
        assert_eq!(
            validate(&payload),
            Err(InvitePayloadError::InvalidSymmetricKeyLength(
                SYMMETRIC_KEY_LEN - 1
            ))
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn build_payload_round_trip() {
        let service_pointer = b"https://invites.example/abc".to_vec();
        let external_group_id = generate_external_group_id().to_vec();
        let key = [0x42u8; SYMMETRIC_KEY_LEN];

        let payload = build_payload(service_pointer.clone(), external_group_id.clone(), key);
        let v1 = validate(&payload)?;
        assert_eq!(v1.service_pointer, service_pointer);
        assert_eq!(v1.external_group_id, external_group_id);
        assert_eq!(v1.symmetric_key, key.to_vec());
    }
}
