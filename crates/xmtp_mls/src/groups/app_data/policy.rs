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
        // Fail closed, never fail hard. An unrecognized or malformed stored
        // policy (an empty And/Any condition, or a base value from a newer
        // client) must degrade to deny for that one field, exactly as the
        // sibling `metadata_policy_to_permissions` and
        // `metadata_policy_to_membership` converters do.
        //
        // Returning `Err` here would propagate through
        // `policy_set_from_registry` into `ValidatedCommit::from_staged_commit`
        // as `CommitValidationError::installed_state`, which is NOT in
        // `is_safe_rejection` — so the commit head would stay pending and
        // every member would wedge on the group permanently, rather than
        // rejecting one commit. A single bad registry entry must not be able
        // to brick a group.
        let policy = component_permissions(&registry, id)
            .and_then(|permissions| permissions.update_policy)
            .and_then(|stored| MetadataPolicies::try_from(stored).ok())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::groups::group_permissions::PolicyError;
    use openmls::extensions::{AppDataDictionary, AppDataDictionaryExtension, Extension};
    use xmtp_mls_common::{
        app_data::migration::synthesize_registry_from_policy_set,
        group_mutable_metadata::MetadataField,
    };
    use xmtp_proto::xmtp::mls::message_contents::metadata_policy::AndCondition;

    #[xmtp_common::test(unwrap_try = true)]
    fn test_policy_set_from_registry_denies_malformed_metadata_policy() {
        let mut expected = PolicySet::default();
        let mut registry = synthesize_registry_from_policy_set(&expected.to_proto()?)?;
        let malformed = MetadataPolicyProto {
            kind: Some(MetadataPolicyKindProto::AndCondition(AndCondition {
                policies: vec![],
            })),
        };
        assert!(matches!(
            MetadataPolicies::try_from(malformed.clone()),
            Err(PolicyError::InvalidMetadataPolicy)
        ));

        // GROUP_NAME accepts stored policies that the policy converter rejects.
        let mut metadata = registry.get(&ComponentId::GROUP_NAME)??;
        metadata.permissions.as_mut()?.update_policy = Some(malformed);
        registry.set(ComponentId::GROUP_NAME, metadata)?;

        let mut dictionary = AppDataDictionary::new();
        assert!(
            dictionary
                .insert(
                    ComponentId::COMPONENT_REGISTRY.as_u16(),
                    registry.to_bytes()?
                )
                .is_none()
        );
        let extensions = Extensions::from_vec(vec![Extension::AppDataDictionary(
            AppDataDictionaryExtension::new(dictionary),
        )])?;

        let permissions = policy_set_from_registry(&extensions)?;
        expected.update_metadata_policy.insert(
            MetadataField::GroupName.to_string(),
            MetadataPolicies::deny(),
        );
        // All other metadata, membership, and admin policies must stay intact.
        assert_eq!(permissions.policies, expected);
    }
}
