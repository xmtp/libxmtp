//! Translate the AppData `COMPONENT_REGISTRY`'s membership policies into
//! the legacy [`GroupMutablePermissions`] shape so the receive-side
//! validator and standalone-proposal validator can evaluate Add /
//! Remove proposals on migrated groups against the same policy that
//! `validate_app_data_update_proposals_in_commit` enforces for the
//! companion `AppDataUpdate(GROUP_MEMBERSHIP)` proposal.
//!
//! The mapping is:
//! - `GROUP_MEMBERSHIP.insert_policy` → `add_member_policy`
//! - `GROUP_MEMBERSHIP.delete_policy` → `remove_member_policy`
//!
//! Other slots on the synthesized `PolicySet` (metadata field updates,
//! admin add/remove, permissions update) keep the conservative
//! "super admin only" stub — the AppDataUpdate path enforces those
//! per-component when a real change comes through, and the legacy
//! callers only consult them as part of structural sanity checks.

use openmls::group::MlsGroup as OpenMlsGroup;
use xmtp_mls_common::app_data::component_id::ComponentId;
use xmtp_proto::xmtp::mls::message_contents::{
    MetadataPolicy as MetadataPolicyProto, metadata_policy::Kind as MetadataPolicyKindProto,
    metadata_policy::MetadataBasePolicy as MetadataBasePolicyProto,
};

use super::component_source::ComponentSourceError;
use crate::groups::group_permissions::{
    GroupMutablePermissions, MembershipPolicies, PermissionsPolicies, PolicySet,
};

/// Translate a wire-form `MetadataPolicy` into a `MembershipPolicies`.
///
/// Unknown / non-base variants conservatively map to `Deny` — Add /
/// Remove proposals from peers using a policy shape we don't recognize
/// must not slip through silently.
fn metadata_policy_to_membership(p: &MetadataPolicyProto) -> MembershipPolicies {
    match p.kind.as_ref() {
        Some(MetadataPolicyKindProto::Base(base)) => {
            match MetadataBasePolicyProto::try_from(*base) {
                Ok(MetadataBasePolicyProto::Allow) => MembershipPolicies::allow(),
                Ok(MetadataBasePolicyProto::Deny) => MembershipPolicies::deny(),
                Ok(MetadataBasePolicyProto::AllowIfAdmin) => {
                    MembershipPolicies::allow_if_actor_admin()
                }
                Ok(MetadataBasePolicyProto::AllowIfSuperAdmin) => {
                    MembershipPolicies::allow_if_actor_super_admin()
                }
                Ok(MetadataBasePolicyProto::Unspecified) | Err(_) => MembershipPolicies::deny(),
            }
        }
        // AndCondition / AnyCondition variants don't have a clean
        // `MembershipPolicies` analogue today — every well-known
        // component the bootstrap synthesizer emits uses `Base`.
        // Conservative deny here keeps the door closed if we ever hit
        // a richer policy shape we haven't translated.
        _ => MembershipPolicies::deny(),
    }
}

/// Build a `GroupMutablePermissions` whose `add_member_policy` /
/// `remove_member_policy` reflect the registry's `GROUP_MEMBERSHIP`
/// policies. Everything else is a stub — the AppDataUpdate validation
/// path is the authoritative gate for non-membership components.
pub(crate) fn membership_policy_set_from_registry(
    mls_group: &OpenMlsGroup,
) -> Result<GroupMutablePermissions, ComponentSourceError> {
    let registry = super::load_component_registry(mls_group)?;

    let (add_policy, remove_policy) = match registry.get(&ComponentId::GROUP_MEMBERSHIP) {
        Ok(Some(meta)) => match meta.permissions {
            Some(perms) => {
                let add = match perms.insert_policy.as_ref() {
                    Some(p) => metadata_policy_to_membership(p),
                    None => {
                        tracing::warn!(
                            "GROUP_MEMBERSHIP registry entry missing insert_policy; \
                             falling back to deny-add membership policy"
                        );
                        MembershipPolicies::deny()
                    }
                };
                let remove = match perms.delete_policy.as_ref() {
                    Some(p) => metadata_policy_to_membership(p),
                    None => {
                        tracing::warn!(
                            "GROUP_MEMBERSHIP registry entry missing delete_policy; \
                             falling back to deny-remove membership policy"
                        );
                        MembershipPolicies::deny()
                    }
                };
                (add, remove)
            }
            None => {
                tracing::warn!(
                    "GROUP_MEMBERSHIP registry entry has no permissions block; \
                     falling back to deny-all membership policy"
                );
                (MembershipPolicies::deny(), MembershipPolicies::deny())
            }
        },
        // Missing GROUP_MEMBERSHIP entry — the dict is partially
        // populated or we're racing the bootstrap. Production groups
        // always carry this entry post-bootstrap, so a miss here is
        // worth a breadcrumb rather than surfacing only as a
        // downstream `InsufficientPermissions`.
        Ok(None) => {
            tracing::warn!(
                "no GROUP_MEMBERSHIP entry in component registry; \
                 falling back to deny-all membership policy"
            );
            (MembershipPolicies::deny(), MembershipPolicies::deny())
        }
        Err(e) => {
            tracing::warn!(
                error = ?e,
                "GROUP_MEMBERSHIP registry decode failed; falling back to deny-all membership policy"
            );
            (MembershipPolicies::deny(), MembershipPolicies::deny())
        }
    };

    // Non-membership slots stay on the conservative super-admin stub:
    // metadata field updates, admin add/remove, and permissions update
    // are all enforced per-component by
    // `validate_app_data_update_proposals_in_commit` when a real change
    // comes through. Legacy callers only consult these slots as part
    // of structural sanity checks, so super-admin-only is safe.
    Ok(GroupMutablePermissions::new(PolicySet::new(
        add_policy,
        remove_policy,
        std::collections::HashMap::new(),
        PermissionsPolicies::allow_if_actor_super_admin(),
        PermissionsPolicies::allow_if_actor_super_admin(),
        PermissionsPolicies::allow_if_actor_super_admin(),
    )))
}
