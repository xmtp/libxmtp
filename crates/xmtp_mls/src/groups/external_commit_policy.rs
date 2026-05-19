//! External-commit policy lookup helpers.
//!
//! Two layers gate an incoming MLS External Commit (RFC 9420 §12.4.3.2):
//!
//! 1. **Master switch** — the `EXTERNAL_COMMIT_POLICY` well-known
//!    component, decoded into [`ExternalCommitPolicyV1`]. Carries
//!    `allow_external_commit` plus the time-window controls
//!    (`expires_at_ns`, `expire_in_ns`).
//! 2. **Per-component declarative permissions** — each component's
//!    `ComponentMetadata.external_committer_permissions` block. Sibling
//!    of the existing `permissions` block; governs what external
//!    committers may do to *this* component.
//!
//! Both layers default to "deny" when absent — this module surfaces
//! `Option<…>`/`bool` from "absent" rather than synthesizing a default
//! struct, so callers can route on whether the admin has ever opted in.
//!
//! The MLS-spec invariants (exactly one ExternalInit, joiner credential
//! binding on Adds, no by-reference proposals, no SelfRemove) are
//! hardcoded in the validator (see L-7); this module only covers the
//! AppData-resident policy.

use openmls::group::MlsGroup as OpenMlsGroup;
use prost::Message;
use tls_codec::VLBytes;
use xmtp_mls_common::{
    app_data::{
        component_id::ComponentId, components::tls_map_components::ComponentRegistryComponent,
        typed::Component,
    },
    invite::payload::{MIN_EXTERNAL_GROUP_ID_LEN, SYMMETRIC_KEY_LEN, validate_service_pointer},
    tls_map::TlsMapDelta,
};
use xmtp_proto::xmtp::mls::message_contents::{
    ComponentPermissions, ExternalCommitPolicyEntry, ExternalCommitPolicyV1,
    MetadataPolicy as MetadataPolicyProto, ServicePointer,
    external_commit_policy_entry::Version as ExternalCommitPolicyVersion,
    metadata_policy::{Kind as MetadataPolicyKind, MetadataBasePolicy},
};

use crate::groups::app_data::{component_source::ComponentSourceError, load_component_registry};

/// Caller-tunable settings for `MlsGroup::enable_external_commits`. The
/// freshly-generated `symmetric_key` / `external_group_id` are NOT here —
/// they are minted by the enable call itself (CSPRNG) and returned as
/// [`ExternalInviteCoordinates`].
#[derive(Debug, Clone, Default)]
pub struct ExternalInviteSettings {
    /// Wall-clock campaign expiry (ns since UNIX epoch); `0` = none.
    /// Per-invite: cleared by revoke.
    pub expires_at_ns: u64,
    /// Max staleness of the GroupInfo referenced by an external commit;
    /// `0` = none. Durable setting: survives revoke.
    pub expire_in_ns: u64,
    /// Concurrent cap on members admitted via the active invite; `0` =
    /// unlimited. Durable setting: survives revoke.
    pub max_uses: u32,
    /// Service locations members use to keep the invite blob fresh
    /// across epoch advances. Empty = member-driven refresh off
    /// (only the issuer and past scanners can refresh).
    pub refresh_pointers: Vec<ServicePointer>,
}

/// The invite coordinates minted by `MlsGroup::enable_external_commits` —
/// exactly what the QR payload carries alongside the per-QR service
/// pointer.
#[derive(Clone)]
pub struct ExternalInviteCoordinates {
    /// Fresh 32-byte ChaCha20Poly1305 key wrapping the GroupInfo blob.
    pub symmetric_key: [u8; SYMMETRIC_KEY_LEN],
    /// Fresh service-slot identifier.
    pub external_group_id: Vec<u8>,
}

impl std::fmt::Debug for ExternalInviteCoordinates {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The key is the secret; the slot id is service-visible by
        // design and safe to log.
        f.debug_struct("ExternalInviteCoordinates")
            .field("symmetric_key", &"<redacted>")
            .field("external_group_id", &hex::encode(&self.external_group_id))
            .finish()
    }
}

/// Violations of the XIP-82 field-coupling invariants on an
/// `EXTERNAL_COMMIT_POLICY` value. Enforced setter-side (the high-level
/// APIs refuse to queue a violating proposal) AND receive-side as a
/// post-state invariant (validators reject a commit whose resulting
/// policy state violates them) — both checks are pure functions of the
/// proposed value (+ post-state registry), so every member converges.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ExternalCommitPolicyError {
    /// Enabled policy without a `symmetric_key`.
    #[error("enabled policy requires symmetric_key")]
    MissingSymmetricKey,
    /// `symmetric_key.material` length is not exactly 32 bytes.
    #[error("symmetric_key.material must be {SYMMETRIC_KEY_LEN} bytes (got {0})")]
    InvalidSymmetricKeyLength(usize),
    /// Enabled policy whose `external_group_id` is shorter than the
    /// 4-byte floor.
    #[error("external_group_id must be at least {MIN_EXTERNAL_GROUP_ID_LEN} bytes (got {0})")]
    InvalidExternalGroupIdLength(usize),
    /// A `refresh_pointers` entry is present but carries no location
    /// variant, or an https location fails validation.
    #[error("invalid refresh_pointer: {0}")]
    InvalidRefreshPointer(String),
    /// Disabled policy retains per-invite state. The revoke invariant:
    /// `allow_external_commit == false` implies `symmetric_key` ABSENT,
    /// `external_group_id` empty, `expires_at_ns` 0, and
    /// `refresh_pointers` empty — a revoked policy serializes to nothing
    /// but the durable settings (`expire_in_ns`, `max_uses`),
    /// byte-identical to a policy that never had an invite. Lingering
    /// state is a trap: a stale key could be revived by a careless
    /// re-enable, stale pointers re-adopted, and a stale absolute
    /// `expires_at_ns` would silently mis-bound the next campaign.
    #[error("disabled policy must leave per-invite field absent: {field}")]
    PerInviteFieldNotCleared {
        /// Which per-invite field was left populated.
        field: &'static str,
    },
    /// Enabled policy in a group whose `GROUP_MEMBERSHIP`
    /// `ComponentMetadata.external_committer_permissions` does not admit
    /// a joiner inserting its own entry. Every conforming external
    /// commit is structurally required to write that entry, so without
    /// the grant the switch is on but every join dead-ends at
    /// validation. The enabling commit MUST establish the grant.
    #[error("enabled policy requires the GROUP_MEMBERSHIP external-committer insert grant")]
    MissingMembershipGrant,
}

/// Validate the XIP-82 field-coupling invariants on a policy value.
/// Pure function of the value; the cross-component grant check is
/// separate (see [`grant_admits_joiner_insert`]) because it needs the
/// post-state registry.
pub(crate) fn validate_policy_v1(
    policy: &ExternalCommitPolicyV1,
) -> Result<(), ExternalCommitPolicyError> {
    if policy.allow_external_commit {
        let key = policy
            .symmetric_key
            .as_ref()
            .ok_or(ExternalCommitPolicyError::MissingSymmetricKey)?;
        if key.material.len() != SYMMETRIC_KEY_LEN {
            return Err(ExternalCommitPolicyError::InvalidSymmetricKeyLength(
                key.material.len(),
            ));
        }
        if policy.external_group_id.len() < MIN_EXTERNAL_GROUP_ID_LEN {
            return Err(ExternalCommitPolicyError::InvalidExternalGroupIdLength(
                policy.external_group_id.len(),
            ));
        }
        for pointer in &policy.refresh_pointers {
            validate_service_pointer(pointer)
                .map_err(|e| ExternalCommitPolicyError::InvalidRefreshPointer(e.to_string()))?;
        }
    } else {
        // Revoke / disabled post-state: every per-invite field absent.
        // An empty SymmetricKey submessage is the forbidden second
        // representable state — only full absence is the cleared
        // encoding.
        if policy.symmetric_key.is_some() {
            return Err(ExternalCommitPolicyError::PerInviteFieldNotCleared {
                field: "symmetric_key",
            });
        }
        if !policy.external_group_id.is_empty() {
            return Err(ExternalCommitPolicyError::PerInviteFieldNotCleared {
                field: "external_group_id",
            });
        }
        if policy.expires_at_ns != 0 {
            return Err(ExternalCommitPolicyError::PerInviteFieldNotCleared {
                field: "expires_at_ns",
            });
        }
        if !policy.refresh_pointers.is_empty() {
            return Err(ExternalCommitPolicyError::PerInviteFieldNotCleared {
                field: "refresh_pointers",
            });
        }
    }
    Ok(())
}

/// Whether a `GROUP_MEMBERSHIP` `external_committer_permissions` block
/// admits a joiner inserting its own entry: the block must be present
/// with `insert_policy` of base `Allow`. (The external committer is by
/// definition neither admin nor super-admin at validation time, so any
/// stricter base policy denies it; the validator's atomic-shape checks
/// — own entry only — bound what "Allow" can do.)
pub(crate) fn grant_admits_joiner_insert(perms: Option<&ComponentPermissions>) -> bool {
    matches!(
        perms
            .and_then(|p| p.insert_policy.as_ref())
            .and_then(|policy| policy.kind.as_ref()),
        Some(MetadataPolicyKind::Base(base))
            if *base == MetadataBasePolicy::Allow as i32
    )
}

/// Build the `AppDataUpdate(COMPONENT_REGISTRY)` payload that grants
/// external committers `insert` access to their own `GROUP_MEMBERSHIP`
/// entry, preserving everything else on the component's metadata.
///
/// The enable commit ALWAYS carries this write, even when the current
/// registry already admits the insert: an enable racing a concurrent
/// grant removal would otherwise land grant-less and be rejected by
/// every validator (post-state invariant) with no way for the intent
/// retry to recover. Update / delete stay untouched (absent =
/// all-Deny), so an external committer still cannot rewrite or remove
/// entries; the validator's atomic-shape checks bound the insert to the
/// joiner's own entry.
///
/// Known lost-update window (same class as the existing
/// `update_permission` path): the payload snapshots the component's
/// full `ComponentMetadata` at queue time, so a concurrent metadata
/// write to GROUP_MEMBERSHIP that lands between queue and commit is
/// clobbered (last writer wins). All members apply the same delta, so
/// state converges; residual-delta computation for registry writes is
/// the documented follow-on for the generic AppDataUpdate path.
pub(crate) fn build_membership_grant_registry_payload(
    mls_group: &OpenMlsGroup,
) -> Result<Vec<u8>, ComponentSourceError> {
    let registry = load_component_registry(mls_group)?;
    let mut metadata = registry
        .get(&ComponentId::GROUP_MEMBERSHIP)
        .map_err(|e| ComponentSourceError::MalformedComponentValue {
            component_id: ComponentId::GROUP_MEMBERSHIP,
            reason: format!("registry get failed: {e}"),
        })?
        .ok_or_else(|| ComponentSourceError::MalformedComponentValue {
            component_id: ComponentId::GROUP_MEMBERSHIP,
            reason: "registry has no entry for GROUP_MEMBERSHIP".into(),
        })?;

    let mut perms = metadata
        .external_committer_permissions
        .clone()
        .unwrap_or_default();
    perms.insert_policy = Some(MetadataPolicyProto {
        kind: Some(MetadataPolicyKind::Base(MetadataBasePolicy::Allow as i32)),
    });
    metadata.external_committer_permissions = Some(perms);

    let delta = TlsMapDelta::<ComponentId, VLBytes>::new().update(
        ComponentId::GROUP_MEMBERSHIP,
        VLBytes::new(metadata.encode_to_vec()),
    );
    <ComponentRegistryComponent as Component>::encode_mutation(&delta)
        .map_err(ComponentSourceError::from)
}

/// Read the `EXTERNAL_COMMIT_POLICY` component from the group's AppData
/// dictionary. Returns:
///
/// - `Ok(Some(policy))` — entry is present and decoded.
/// - `Ok(None)` — entry is absent, or the dict has no recognizable
///   version variant (defensive: unknown variants treated as absent).
/// - `Err(_)` — registry / extension decode failed.
//
// Consumed by `revoke_external_commits` (durable-settings preservation)
// and by the L-7 validator (`ValidatedCommit::from_external_commit`).
pub(crate) fn load_external_commit_policy(
    mls_group: &OpenMlsGroup,
) -> Result<Option<ExternalCommitPolicyV1>, ComponentSourceError> {
    let Some(bytes) = mls_group
        .extensions()
        .app_data_dictionary()
        .and_then(|ext| {
            ext.dictionary()
                .get(&ComponentId::EXTERNAL_COMMIT_POLICY.as_u16())
        })
    else {
        return Ok(None);
    };

    let entry = ExternalCommitPolicyEntry::decode(bytes).map_err(|e| {
        ComponentSourceError::MalformedComponentValue {
            component_id: ComponentId::EXTERNAL_COMMIT_POLICY,
            reason: format!("ExternalCommitPolicyEntry decode: {e}"),
        }
    })?;

    // Unknown future variant — treat as default-disabled rather than
    // failing hard. Newer clients understand the variant; older ones
    // fail closed.
    Ok(entry.version.map(|ExternalCommitPolicyVersion::V1(v1)| v1))
}

/// Convenience: true iff the group has opted into accepting external
/// commits via `EXTERNAL_COMMIT_POLICY.v1.allow_external_commit`.
///
/// This is the cheap first-line check the validator runs before any
/// per-proposal evaluation. It does NOT enforce the time-window fields
/// (`expires_at_ns` / `expire_in_ns`) — the validator consults the full
/// policy via [`load_external_commit_policy`] for those, because they
/// require additional context (wall-clock time and GroupInfo export
/// timestamp) the helper itself doesn't have.
///
/// Returns `false` on absent entry, decode failure, or any policy
/// shape that doesn't set the bit. Fails closed.
//
// The validator (`ValidatedCommit::from_external_commit`) reads the
// full policy via `load_external_commit_policy` instead; this stays as
// the cheap pre-check for the L-8 ingestion dispatch. Dead-allowed
// until L-8 lands.
#[allow(dead_code)]
pub(crate) fn is_external_commit_allowed(mls_group: &OpenMlsGroup) -> bool {
    load_external_commit_policy(mls_group)
        .ok()
        .flatten()
        .map(|policy| policy.allow_external_commit)
        .unwrap_or(false)
}

/// Read the `external_committer_permissions` block from the
/// `ComponentMetadata` of the given component in the registry.
///
/// Returns:
///
/// - `Ok(Some(perms))` — component has an `external_committer_permissions`
///   block. The caller evaluates each proposal's effect against the
///   relevant policy slot.
/// - `Ok(None)` — component is in the registry but has no
///   `external_committer_permissions` block, OR component isn't in the
///   registry at all. In both cases the validator treats this as
///   all-Deny: external committers may not touch this component.
/// - `Err(_)` — registry decode failed.
//
// Consumed by the L-7 validator (check 10).
pub(crate) fn external_committer_permissions_for(
    mls_group: &OpenMlsGroup,
    component_id: ComponentId,
) -> Result<Option<ComponentPermissions>, ComponentSourceError> {
    let registry = load_component_registry(mls_group)?;
    let Some(meta) = registry.get(&component_id).ok().flatten() else {
        return Ok(None);
    };
    Ok(meta.external_committer_permissions)
}

#[cfg(test)]
mod tests {
    //! Round-trip + absence coverage for the policy lookup helpers.
    use super::*;
    use openmls::extensions::{
        AppDataDictionary, AppDataDictionaryExtension, Extension, Extensions,
    };
    use xmtp_proto::xmtp::mls::message_contents::ComponentMetadata;

    fn encode_policy(v1: ExternalCommitPolicyV1) -> Vec<u8> {
        ExternalCommitPolicyEntry {
            version: Some(ExternalCommitPolicyVersion::V1(v1)),
        }
        .encode_to_vec()
    }

    fn extensions_with_policy_bytes(bytes: Vec<u8>) -> Extensions<openmls::group::GroupContext> {
        let mut dict = AppDataDictionary::new();
        let _ = dict.insert(ComponentId::EXTERNAL_COMMIT_POLICY.as_u16(), bytes);
        Extensions::from_vec(vec![Extension::AppDataDictionary(
            AppDataDictionaryExtension::new(dict),
        )])
        .expect("AppDataDictionary is a valid GroupContext extension")
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn empty_dict_treated_as_disabled() {
        let extensions: Extensions<openmls::group::GroupContext> =
            Extensions::from_vec(vec![]).unwrap();
        let dict_entry = extensions.app_data_dictionary().and_then(|ext| {
            ext.dictionary()
                .get(&ComponentId::EXTERNAL_COMMIT_POLICY.as_u16())
        });
        assert!(dict_entry.is_none(), "no dict entry should be present");
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn malformed_entry_surfaces_decode_error() {
        let extensions = extensions_with_policy_bytes(vec![0xFF; 16]);
        let bytes = extensions
            .app_data_dictionary()
            .and_then(|ext| {
                ext.dictionary()
                    .get(&ComponentId::EXTERNAL_COMMIT_POLICY.as_u16())
            })
            .unwrap();
        let err = ExternalCommitPolicyEntry::decode(bytes);
        assert!(err.is_err(), "malformed bytes must fail to decode");
    }

    /// A well-formed enabled policy, reused by the invariant tests.
    fn enabled_policy() -> ExternalCommitPolicyV1 {
        use xmtp_proto::xmtp::mls::message_contents::SymmetricKey;
        ExternalCommitPolicyV1 {
            allow_external_commit: true,
            expires_at_ns: 1_700_000_000_000_000_000,
            expire_in_ns: 60_000_000_000,
            symmetric_key: Some(SymmetricKey {
                material: vec![0x11u8; 32],
            }),
            external_group_id: vec![0x22u8; 16],
            max_uses: 5,
            refresh_pointers: vec![],
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn round_trip_allows_external_commit() {
        let v1 = enabled_policy();
        let bytes = encode_policy(v1.clone());
        let decoded = ExternalCommitPolicyEntry::decode(bytes.as_ref()).unwrap();
        match decoded.version {
            Some(ExternalCommitPolicyVersion::V1(v)) => {
                assert!(v.allow_external_commit);
                assert_eq!(v, v1);
            }
            None => panic!("decoded entry has no version variant"),
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn invariants_accept_enabled_and_revoked_shapes() {
        // Well-formed enabled policy passes.
        validate_policy_v1(&enabled_policy())?;

        // A clean revoke passes — and durable settings surviving the
        // revoke are legal (only per-invite fields must be absent).
        let revoked = ExternalCommitPolicyV1 {
            allow_external_commit: false,
            expire_in_ns: 60_000_000_000,
            max_uses: 5,
            ..Default::default()
        };
        validate_policy_v1(&revoked)?;
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn invariants_reject_malformed_enabled_policies() {
        use xmtp_proto::xmtp::mls::message_contents::SymmetricKey;

        let mut missing_key = enabled_policy();
        missing_key.symmetric_key = None;
        assert_eq!(
            validate_policy_v1(&missing_key),
            Err(ExternalCommitPolicyError::MissingSymmetricKey)
        );

        let mut short_key = enabled_policy();
        short_key.symmetric_key = Some(SymmetricKey {
            material: vec![0u8; 31],
        });
        assert_eq!(
            validate_policy_v1(&short_key),
            Err(ExternalCommitPolicyError::InvalidSymmetricKeyLength(31))
        );

        let mut short_id = enabled_policy();
        short_id.external_group_id = vec![0u8; 3];
        assert_eq!(
            validate_policy_v1(&short_id),
            Err(ExternalCommitPolicyError::InvalidExternalGroupIdLength(3))
        );

        // A refresh pointer with no location variant fails closed.
        let mut empty_pointer = enabled_policy();
        empty_pointer.refresh_pointers =
            vec![xmtp_proto::xmtp::mls::message_contents::ServicePointer { location: None }];
        assert!(matches!(
            validate_policy_v1(&empty_pointer),
            Err(ExternalCommitPolicyError::InvalidRefreshPointer(_))
        ));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn invariants_reject_lingering_per_invite_state_on_revoke() {
        use xmtp_proto::xmtp::mls::message_contents::SymmetricKey;

        // An EMPTY SymmetricKey submessage is the forbidden second
        // representable state — absence is the only cleared encoding.
        let cases: Vec<(&str, ExternalCommitPolicyV1)> = vec![
            (
                "symmetric_key",
                ExternalCommitPolicyV1 {
                    symmetric_key: Some(SymmetricKey { material: vec![] }),
                    ..Default::default()
                },
            ),
            (
                "external_group_id",
                ExternalCommitPolicyV1 {
                    external_group_id: vec![0x22u8; 16],
                    ..Default::default()
                },
            ),
            (
                "expires_at_ns",
                ExternalCommitPolicyV1 {
                    expires_at_ns: 1,
                    ..Default::default()
                },
            ),
            (
                "refresh_pointers",
                ExternalCommitPolicyV1 {
                    refresh_pointers: vec![
                        xmtp_mls_common::invite::payload::opaque_service_pointer(b"x".to_vec()),
                    ],
                    ..Default::default()
                },
            ),
        ];
        for (field, policy) in cases {
            assert_eq!(
                validate_policy_v1(&policy),
                Err(ExternalCommitPolicyError::PerInviteFieldNotCleared { field }),
                "expected {field} to be rejected"
            );
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn grant_check_requires_insert_allow() {
        use xmtp_proto::xmtp::mls::message_contents::MetadataPolicy as MetadataPolicyProto;

        // Absent block: deny.
        assert!(!grant_admits_joiner_insert(None));
        // Block without insert policy: deny.
        assert!(!grant_admits_joiner_insert(Some(
            &ComponentPermissions::default()
        )));
        // Insert Deny: deny.
        let deny = ComponentPermissions {
            insert_policy: Some(MetadataPolicyProto {
                kind: Some(MetadataPolicyKind::Base(MetadataBasePolicy::Deny as i32)),
            }),
            ..Default::default()
        };
        assert!(!grant_admits_joiner_insert(Some(&deny)));
        // Insert Allow: admit.
        let allow = ComponentPermissions {
            insert_policy: Some(MetadataPolicyProto {
                kind: Some(MetadataPolicyKind::Base(MetadataBasePolicy::Allow as i32)),
            }),
            ..Default::default()
        };
        assert!(grant_admits_joiner_insert(Some(&allow)));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn round_trip_default_disabled() {
        // Zero-valued ExternalCommitPolicyV1 must decode back unchanged.
        let v1 = ExternalCommitPolicyV1::default();
        let bytes = encode_policy(v1);
        let decoded = ExternalCommitPolicyEntry::decode(bytes.as_ref()).unwrap();
        match decoded.version {
            Some(ExternalCommitPolicyVersion::V1(v)) => {
                assert!(!v.allow_external_commit);
                assert_eq!(v.expires_at_ns, 0);
                assert_eq!(v.expire_in_ns, 0);
            }
            None => panic!("decoded entry has no version variant"),
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn component_metadata_without_external_block_is_treated_as_deny() {
        // ComponentMetadata with no external_committer_permissions field
        // is treated as all-Deny by the validator.
        let meta = ComponentMetadata {
            component_type: 1,
            permissions: None,
            external_committer_permissions: None,
        };
        assert!(meta.external_committer_permissions.is_none());
    }
}
