//! Helpers for the [`ExternalInvitePayload`] proto.
//!
//! Centralises the small but easy-to-get-wrong pieces of building and
//! validating an external-invite payload:
//!
//! * fresh symmetric keys / nonces / external-group-ids from the workspace CSPRNG
//! * recognising / unwrapping the `oneof version { V1 v1 }` envelope
//! * validating the typed fields ([`SymmetricKey`] length, [`ServicePointer`]
//!   shape and `https` scheme)
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
    ExternalInvitePayload, ExternalInvitePayloadV1, ServicePointer, SymmetricKey,
    external_invite_payload::Version as ExternalInvitePayloadVersion,
    service_pointer::Location as ServiceLocation,
};

/// Length in bytes of the ChaCha20Poly1305 key used to wrap the encrypted
/// `GroupInfo` blob referenced by an [`ExternalInvitePayload`]. The
/// [`SymmetricKey`] submessage does not constrain length, so validators and
/// setters enforce it here.
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
    /// `symmetric_key` was absent. A payload without the key cannot decrypt
    /// anything; absence is only a legal encoding on the *policy* component
    /// (where it means "no active invite"), never on the payload.
    #[error("symmetric_key is required on an external-invite payload")]
    MissingSymmetricKey,
    /// `symmetric_key.material` was not exactly [`SYMMETRIC_KEY_LEN`] bytes.
    #[error("symmetric_key.material must be exactly {SYMMETRIC_KEY_LEN} bytes (got {0})")]
    InvalidSymmetricKeyLength(usize),
    /// `service_pointer` was present but its `location` oneof was unset.
    /// A pointer with no location gives the joiner no fetch target: fail
    /// closed, like an unrecognized version. (A payload with the field
    /// entirely ABSENT is fine — that means application-resolved.)
    #[error("service_pointer is present but carries no location (fail closed)")]
    EmptyServicePointer,
    /// `service_pointer.https_url` failed to parse as a URL, or its scheme
    /// was not `https`.
    #[error("service_pointer.https_url is invalid: {0}")]
    InvalidHttpsUrl(String),
}

/// Generate a fresh 32-byte symmetric key from the workspace CSPRNG.
///
/// The key is intended for use with ChaCha20Poly1305 when wrapping the
/// encrypted GroupInfo blob referenced by the resulting
/// [`ExternalInvitePayload`]. Uniform randomness is also what guarantees a
/// re-enabled invite never revives a previously-used key — there is no
/// key-history tracking anywhere.
pub fn generate_symmetric_key() -> [u8; SYMMETRIC_KEY_LEN] {
    xmtp_common::rand_array::<SYMMETRIC_KEY_LEN>()
}

/// Generate a fresh 12-byte nonce from the workspace CSPRNG.
///
/// Intended for use with ChaCha20Poly1305 alongside a key produced by
/// [`generate_symmetric_key`]. The nonce is *not* stored in the payload
/// itself — it lives next to the ciphertext in the encrypted GroupInfo blob.
/// Nonces MUST come from this (or an equivalent CSPRNG) source on every
/// encryption: many independent writers encrypt under the same long-lived
/// key, so deterministic (counter) schemes would collide across writers and
/// reuse a nonce.
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

/// Build a [`ServicePointer`] from an `https` URL, validating it parses and
/// carries the `https` scheme.
pub fn https_service_pointer(url: &str) -> Result<ServicePointer, InvitePayloadError> {
    validate_https_url(url)?;
    Ok(ServicePointer {
        location: Some(ServiceLocation::HttpsUrl(url.to_string())),
    })
}

/// Build a [`ServicePointer`] from application-defined opaque bytes (NFC
/// tags, custom resolver schemes, …). Opaque to libxmtp; no validation
/// beyond carrying *a* location.
pub fn opaque_service_pointer(bytes: Vec<u8>) -> ServicePointer {
    ServicePointer {
        location: Some(ServiceLocation::Opaque(bytes)),
    }
}

fn validate_https_url(raw: &str) -> Result<(), InvitePayloadError> {
    let parsed =
        url::Url::parse(raw).map_err(|e| InvitePayloadError::InvalidHttpsUrl(e.to_string()))?;
    if parsed.scheme() != "https" {
        return Err(InvitePayloadError::InvalidHttpsUrl(format!(
            "scheme must be https (got {})",
            parsed.scheme()
        )));
    }
    Ok(())
}

/// Validate a [`ServicePointer`]: exactly one `location` variant must be
/// set, and an `https_url` location must parse with the `https` scheme.
///
/// Note the asymmetry with the *field* being absent on a payload: an absent
/// `service_pointer` means the application resolves the service out-of-band
/// and is legal; a present-but-empty pointer is a parse failure.
pub fn validate_service_pointer(pointer: &ServicePointer) -> Result<(), InvitePayloadError> {
    match &pointer.location {
        None => Err(InvitePayloadError::EmptyServicePointer),
        Some(ServiceLocation::HttpsUrl(raw)) => validate_https_url(raw),
        Some(ServiceLocation::Opaque(_)) => Ok(()),
    }
}

/// Validate that `payload.version` carries a recognised variant and that
/// the V1 fields meet their requirements:
///
/// * `service_pointer` — absent is legal (application-resolved); present
///   requires a location variant ([`validate_service_pointer`]).
/// * `external_group_id` — at least [`MIN_EXTERNAL_GROUP_ID_LEN`] bytes.
/// * `symmetric_key` — present with exactly [`SYMMETRIC_KEY_LEN`] bytes of
///   `material`.
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
    if let Some(pointer) = &v1.service_pointer {
        validate_service_pointer(pointer)?;
    }
    if v1.external_group_id.len() < MIN_EXTERNAL_GROUP_ID_LEN {
        return Err(InvitePayloadError::InvalidExternalGroupIdLength {
            len: v1.external_group_id.len(),
        });
    }
    let key = v1
        .symmetric_key
        .as_ref()
        .ok_or(InvitePayloadError::MissingSymmetricKey)?;
    if key.material.len() != SYMMETRIC_KEY_LEN {
        return Err(InvitePayloadError::InvalidSymmetricKeyLength(
            key.material.len(),
        ));
    }
    Ok(v1)
}

/// Build an [`ExternalInvitePayload`] wrapping a [`ExternalInvitePayloadV1`]
/// with the supplied fields.
///
/// * `service_pointer` — where the encrypted GroupInfo blob can be fetched
///   ([`https_service_pointer`] / [`opaque_service_pointer`]). `None` means
///   application-resolved: the scanning app already knows how to reach its
///   service, and the QR carries no fetch target at all.
/// * `external_group_id` — service-slot identifier carried on the wire and
///   verified by the joiner against the group's
///   `EXTERNAL_COMMIT_POLICY.external_group_id` after joining. MUST be at
///   least [`MIN_EXTERNAL_GROUP_ID_LEN`] bytes; checked at construction so
///   callers surface the error at the build site rather than at
///   [`validate`] time.
/// * `symmetric_key` — typically the output of [`generate_symmetric_key`].
///   Length is type-enforced.
pub fn build_payload(
    service_pointer: Option<ServicePointer>,
    external_group_id: Vec<u8>,
    symmetric_key: [u8; SYMMETRIC_KEY_LEN],
) -> Result<ExternalInvitePayload, InvitePayloadError> {
    if let Some(pointer) = &service_pointer {
        validate_service_pointer(pointer)?;
    }
    if external_group_id.len() < MIN_EXTERNAL_GROUP_ID_LEN {
        return Err(InvitePayloadError::InvalidExternalGroupIdLength {
            len: external_group_id.len(),
        });
    }
    Ok(ExternalInvitePayload {
        version: Some(ExternalInvitePayloadVersion::V1(ExternalInvitePayloadV1 {
            service_pointer,
            external_group_id,
            symmetric_key: Some(SymmetricKey {
                material: symmetric_key.to_vec(),
            }),
        })),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn well_formed_payload() -> ExternalInvitePayload {
        build_payload(
            Some(https_service_pointer("https://invites.example/abc").expect("valid https url")),
            generate_external_group_id().to_vec(),
            [0x42u8; SYMMETRIC_KEY_LEN],
        )
        .expect("recommended-length external_group_id is well-formed")
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
        assert_eq!(
            v1.symmetric_key.as_ref().unwrap().material.len(),
            SYMMETRIC_KEY_LEN
        );
        assert!(v1.external_group_id.len() >= MIN_EXTERNAL_GROUP_ID_LEN);
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn validate_accepts_absent_service_pointer() {
        // Absent pointer = application-resolved service: the scanning app
        // knows its own endpoint and the QR carries no fetch target.
        let payload = build_payload(
            None,
            generate_external_group_id().to_vec(),
            [0x42u8; SYMMETRIC_KEY_LEN],
        )?;
        let v1 = validate(&payload)?;
        assert!(v1.service_pointer.is_none());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn validate_rejects_present_but_empty_service_pointer() {
        // Present-but-empty is distinguishable from absent on the wire and
        // gives the joiner no fetch target: fail closed.
        let mut payload = well_formed_payload();
        if let Some(ExternalInvitePayloadVersion::V1(ref mut v1)) = payload.version {
            v1.service_pointer = Some(ServicePointer { location: None });
        }
        assert_eq!(
            validate(&payload),
            Err(InvitePayloadError::EmptyServicePointer)
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn https_pointer_rejects_non_https_and_garbage() {
        assert!(matches!(
            https_service_pointer("http://invites.example/abc"),
            Err(InvitePayloadError::InvalidHttpsUrl(_))
        ));
        assert!(matches!(
            https_service_pointer("not a url"),
            Err(InvitePayloadError::InvalidHttpsUrl(_))
        ));
        assert!(https_service_pointer("https://invites.example/abc").is_ok());
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
    fn build_payload_rejects_short_external_group_id() {
        let result = build_payload(
            None,
            vec![0u8; MIN_EXTERNAL_GROUP_ID_LEN - 1],
            [0x42u8; SYMMETRIC_KEY_LEN],
        );
        assert_eq!(
            result,
            Err(InvitePayloadError::InvalidExternalGroupIdLength {
                len: MIN_EXTERNAL_GROUP_ID_LEN - 1
            })
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn validate_rejects_short_external_group_id_from_wire() {
        // `build_payload` rejects too-short ids at construction; a wire payload
        // hand-crafted (bypassing `build_payload`) still needs to be caught at
        // validate time — defense-in-depth for receivers consuming bytes from
        // untrusted peers.
        let payload = ExternalInvitePayload {
            version: Some(ExternalInvitePayloadVersion::V1(ExternalInvitePayloadV1 {
                service_pointer: Some(opaque_service_pointer(b"svc".to_vec())),
                external_group_id: vec![0u8; MIN_EXTERNAL_GROUP_ID_LEN - 1],
                symmetric_key: Some(SymmetricKey {
                    material: vec![0x42u8; SYMMETRIC_KEY_LEN],
                }),
            })),
        };
        assert_eq!(
            validate(&payload),
            Err(InvitePayloadError::InvalidExternalGroupIdLength {
                len: MIN_EXTERNAL_GROUP_ID_LEN - 1
            })
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn validate_rejects_missing_symmetric_key() {
        let mut payload = well_formed_payload();
        if let Some(ExternalInvitePayloadVersion::V1(ref mut v1)) = payload.version {
            v1.symmetric_key = None;
        }
        assert_eq!(
            validate(&payload),
            Err(InvitePayloadError::MissingSymmetricKey)
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn validate_rejects_wrong_symmetric_key_length() {
        let mut payload = well_formed_payload();
        if let Some(ExternalInvitePayloadVersion::V1(ref mut v1)) = payload.version {
            v1.symmetric_key = Some(SymmetricKey {
                material: vec![0u8; SYMMETRIC_KEY_LEN - 1],
            });
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
        let pointer = https_service_pointer("https://invites.example/abc")?;
        let external_group_id = generate_external_group_id().to_vec();
        let key = [0x42u8; SYMMETRIC_KEY_LEN];

        let payload = build_payload(Some(pointer.clone()), external_group_id.clone(), key)?;
        let v1 = validate(&payload)?;
        assert_eq!(v1.service_pointer, Some(pointer));
        assert_eq!(v1.external_group_id, external_group_id);
        assert_eq!(
            v1.symmetric_key,
            Some(SymmetricKey {
                material: key.to_vec()
            })
        );
    }
}
