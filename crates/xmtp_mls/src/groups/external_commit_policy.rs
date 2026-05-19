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
use xmtp_mls_common::app_data::component_id::ComponentId;
use xmtp_proto::xmtp::mls::message_contents::{
    ComponentPermissions, ExternalCommitPolicyEntry, ExternalCommitPolicyV1,
    external_commit_policy_entry::Version as ExternalCommitPolicyVersion,
};

use crate::groups::app_data::{component_source::ComponentSourceError, load_component_registry};

/// Read the `EXTERNAL_COMMIT_POLICY` component from the group's AppData
/// dictionary. Returns:
///
/// - `Ok(Some(policy))` — entry is present and decoded.
/// - `Ok(None)` — entry is absent, or the dict has no recognizable
///   version variant (defensive: unknown variants treated as absent).
/// - `Err(_)` — registry / extension decode failed.
//
// Consumed by the L-7 validator (`ValidatedCommit::from_external_commit`).
// Stays dead-allowed at this PR until L-7 lands.
#[allow(dead_code)]
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
    Ok(entry
        .version
        .map(|ExternalCommitPolicyVersion::V1(v1)| v1))
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
// Consumed by the L-7 validator. Dead-allowed until L-7 lands.
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
// Consumed by the L-7 validator. Dead-allowed until L-7 lands.
#[allow(dead_code)]
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

    #[xmtp_common::test(unwrap_try = true)]
    fn round_trip_allows_external_commit() {
        let v1 = ExternalCommitPolicyV1 {
            allow_external_commit: true,
            expires_at_ns: 1_700_000_000_000_000_000,
            expire_in_ns: 60_000_000_000,
            symmetric_key: vec![0x11u8; 32],
            external_group_id: vec![0x22u8; 16],
        };
        let bytes = encode_policy(v1.clone());
        let decoded = ExternalCommitPolicyEntry::decode(bytes.as_ref()).unwrap();
        match decoded.version {
            Some(ExternalCommitPolicyVersion::V1(v)) => {
                assert!(v.allow_external_commit);
                assert_eq!(v.expires_at_ns, v1.expires_at_ns);
                assert_eq!(v.expire_in_ns, v1.expire_in_ns);
                assert_eq!(v.symmetric_key, v1.symmetric_key);
                assert_eq!(v.external_group_id, v1.external_group_id);
            }
            None => panic!("decoded entry has no version variant"),
        }
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
