//! Read group policies from the AppData component registry.

use openmls::{extensions::Extensions, group::GroupContext};
use xmtp_mls_common::{
    app_data::{component_id::ComponentId, component_registry::ComponentRegistry},
    group_mutable_metadata::GroupMutableMetadata,
};
use xmtp_proto::xmtp::mls::message_contents::{
    ComponentPermissions, MetadataPolicy as MetadataPolicyProto,
    metadata_policy::Kind as MetadataPolicyKindProto,
    metadata_policy::MetadataBasePolicy as MetadataBasePolicyProto,
};

use super::component_source::{ComponentSourceError, metadata_field_to_component_id};
use crate::groups::group_permissions::{
    GroupMutablePermissions, MembershipPolicies, MetadataPolicies, PermissionsPolicies, PolicySet,
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

/// ADMIN_LIST permits only admin or super-admin policies. Treat any other
/// policy as Deny, as the registry validator does for unsupported entries.
fn metadata_policy_to_permissions(policy: &MetadataPolicyProto) -> PermissionsPolicies {
    match policy.kind.as_ref() {
        Some(MetadataPolicyKindProto::Base(base)) => match MetadataBasePolicyProto::try_from(*base)
        {
            Ok(MetadataBasePolicyProto::AllowIfAdmin) => {
                PermissionsPolicies::allow_if_actor_admin()
            }
            Ok(MetadataBasePolicyProto::AllowIfSuperAdmin) => {
                PermissionsPolicies::allow_if_actor_super_admin()
            }
            _ => PermissionsPolicies::deny(),
        },
        _ => PermissionsPolicies::deny(),
    }
}

fn component_permissions(
    registry: &ComponentRegistry,
    id: ComponentId,
) -> Option<ComponentPermissions> {
    match registry.get(&id) {
        Ok(Some(metadata)) => metadata.permissions,
        Ok(None) => {
            tracing::warn!(component_id = %id, "component registry entry is missing; deny access");
            None
        }
        Err(error) => {
            tracing::warn!(component_id = %id, ?error, "component registry entry is invalid; deny access");
            None
        }
    }
}

/// Reconstruct the complete policy set. Callers use the legacy extension
/// instead when the group has no migration marker.
pub(crate) fn policy_set_from_registry(
    extensions: &Extensions<GroupContext>,
) -> Result<GroupMutablePermissions, ComponentSourceError> {
    let registry = super::load_component_registry_from_extensions(extensions)?;
    let membership = component_permissions(&registry, ComponentId::GROUP_MEMBERSHIP);
    let admins = component_permissions(&registry, ComponentId::ADMIN_LIST);
    let mut metadata_policies = std::collections::HashMap::new();
    for field in GroupMutableMetadata::supported_fields() {
        let id = metadata_field_to_component_id(field.as_str())
            .ok_or_else(|| ComponentSourceError::UnknownMetadataField(field.to_string()))?;
        let policy = component_permissions(&registry, id)
            .and_then(|permissions| permissions.update_policy)
            .map(MetadataPolicies::try_from)
            .transpose()
            .map_err(|error| ComponentSourceError::MalformedComponentValue {
                component_id: id,
                reason: format!("invalid metadata policy: {error}"),
            })?
            .unwrap_or_else(MetadataPolicies::deny);
        metadata_policies.insert(field.to_string(), policy);
    }

    Ok(GroupMutablePermissions::new(PolicySet::new(
        membership
            .as_ref()
            .and_then(|p| p.insert_policy.as_ref())
            .map(metadata_policy_to_membership)
            .unwrap_or_else(MembershipPolicies::deny),
        membership
            .as_ref()
            .and_then(|p| p.delete_policy.as_ref())
            .map(metadata_policy_to_membership)
            .unwrap_or_else(MembershipPolicies::deny),
        metadata_policies,
        admins
            .as_ref()
            .and_then(|p| p.insert_policy.as_ref())
            .map(metadata_policy_to_permissions)
            .unwrap_or_else(PermissionsPolicies::deny),
        admins
            .as_ref()
            .and_then(|p| p.delete_policy.as_ref())
            .map(metadata_policy_to_permissions)
            .unwrap_or_else(PermissionsPolicies::deny),
        PermissionsPolicies::allow_if_actor_super_admin(),
    )))
}
