//! Reconstruct legacy policy messages from the app-data dictionary.
//!
//! The dictionary is received state. Bad container bytes deny all policies
//! in that container. Invalid policy syntax denies only the affected field.

use openmls::{extensions::Extensions, group::GroupContext};
use prost::Message as _;
use xmtp_proto::xmtp::mls::message_contents::{
    GroupActionPolicies, MembershipPolicy, MetadataPolicy, PermissionsUpdatePolicy, PolicySet,
    membership_policy::{BasePolicy as MembershipBasePolicy, Kind as MembershipPolicyKind},
    metadata_policy::{Kind as MetadataPolicyKind, MetadataBasePolicy},
    permissions_update_policy::{Kind as PermissionsPolicyKind, PermissionsBasePolicy},
};

use crate::app_data::{
    component_id::ComponentId, component_registry::ComponentRegistry,
    migration::metadata_field_registry_mapping,
};

fn deny_membership() -> MembershipPolicy {
    MembershipPolicy {
        kind: Some(MembershipPolicyKind::Base(
            MembershipBasePolicy::Deny as i32,
        )),
    }
}

fn deny_metadata() -> MetadataPolicy {
    MetadataPolicy {
        kind: Some(MetadataPolicyKind::Base(MetadataBasePolicy::Deny as i32)),
    }
}

fn deny_permissions() -> PermissionsUpdatePolicy {
    PermissionsUpdatePolicy {
        kind: Some(PermissionsPolicyKind::Base(
            PermissionsBasePolicy::Deny as i32,
        )),
    }
}

fn valid_membership(policy: &MembershipPolicy) -> bool {
    match &policy.kind {
        Some(MembershipPolicyKind::Base(base)) => MembershipBasePolicy::try_from(*base)
            .is_ok_and(|base| base != MembershipBasePolicy::Unspecified),
        Some(MembershipPolicyKind::AndCondition(condition)) => {
            !condition.policies.is_empty() && condition.policies.iter().all(valid_membership)
        }
        Some(MembershipPolicyKind::AnyCondition(condition)) => {
            !condition.policies.is_empty() && condition.policies.iter().all(valid_membership)
        }
        None => false,
    }
}

fn valid_metadata(policy: &MetadataPolicy) -> bool {
    match &policy.kind {
        Some(MetadataPolicyKind::Base(base)) => MetadataBasePolicy::try_from(*base)
            .is_ok_and(|base| base != MetadataBasePolicy::Unspecified),
        Some(MetadataPolicyKind::AndCondition(condition)) => {
            !condition.policies.is_empty() && condition.policies.iter().all(valid_metadata)
        }
        Some(MetadataPolicyKind::AnyCondition(condition)) => {
            !condition.policies.is_empty() && condition.policies.iter().all(valid_metadata)
        }
        None => false,
    }
}

fn valid_permissions(policy: &PermissionsUpdatePolicy) -> bool {
    match &policy.kind {
        Some(PermissionsPolicyKind::Base(base)) => PermissionsBasePolicy::try_from(*base)
            .is_ok_and(|base| base != PermissionsBasePolicy::Unspecified),
        Some(PermissionsPolicyKind::AndCondition(condition)) => {
            !condition.policies.is_empty() && condition.policies.iter().all(valid_permissions)
        }
        Some(PermissionsPolicyKind::AnyCondition(condition)) => {
            !condition.policies.is_empty() && condition.policies.iter().all(valid_permissions)
        }
        None => false,
    }
}

fn registry_from_dictionary(extensions: &Extensions<GroupContext>) -> Option<ComponentRegistry> {
    let bytes = extensions
        .app_data_dictionary()?
        .dictionary()
        .get(&ComponentId::COMPONENT_REGISTRY.as_u16())?;
    xmtp_common::optify!(
        ComponentRegistry::from_bytes(bytes),
        "component registry is malformed; deny dictionary policies"
    )
}

fn action_policies_from_dictionary(
    extensions: &Extensions<GroupContext>,
) -> Option<GroupActionPolicies> {
    let bytes = extensions
        .app_data_dictionary()?
        .dictionary()
        .get(&ComponentId::GROUP_ACTION_POLICIES.as_u16())?;
    xmtp_common::optify!(
        GroupActionPolicies::decode(bytes),
        "group action policies are malformed; deny actions"
    )
}

/// Reconstruct a legacy [`PolicySet`] from the dictionary.
///
/// The five action policies are declared by `GROUP_ACTION_POLICIES`. Metadata
/// update policies stay in their well-known registry entries. Missing or
/// invalid policies become `Deny` for that field.
///
/// Fail closed, never fail hard: a conversion error can reach
/// `CommitValidationError::installed_state` in `xmtp_mls`. That error is not
/// a safe rejection. It leaves the commit head pending and blocks every
/// member of the group. A bad stored field must deny that action instead.
pub fn policy_set_from_dictionary(extensions: &Extensions<GroupContext>) -> PolicySet {
    let action_policies = action_policies_from_dictionary(extensions);
    let registry = registry_from_dictionary(extensions);

    let mut update_metadata_policy = std::collections::HashMap::new();
    for (field, component_id, _) in metadata_field_registry_mapping() {
        let policy = registry
            .as_ref()
            .and_then(|registry| {
                xmtp_common::optify!(
                    registry.get(component_id),
                    "registry entry is malformed; deny metadata policy"
                )
                .flatten()
            })
            .and_then(|metadata| metadata.permissions)
            .and_then(|permissions| permissions.update_policy)
            .filter(valid_metadata)
            .unwrap_or_else(deny_metadata);
        update_metadata_policy.insert(field.as_str().to_string(), policy);
    }

    PolicySet {
        add_member_policy: Some(
            action_policies
                .as_ref()
                .and_then(|policies| policies.add_member.clone())
                .filter(valid_membership)
                .unwrap_or_else(deny_membership),
        ),
        remove_member_policy: Some(
            action_policies
                .as_ref()
                .and_then(|policies| policies.remove_member.clone())
                .filter(valid_membership)
                .unwrap_or_else(deny_membership),
        ),
        update_metadata_policy,
        add_admin_policy: Some(
            action_policies
                .as_ref()
                .and_then(|policies| policies.add_admin.clone())
                .filter(valid_permissions)
                .unwrap_or_else(deny_permissions),
        ),
        remove_admin_policy: Some(
            action_policies
                .as_ref()
                .and_then(|policies| policies.remove_admin.clone())
                .filter(valid_permissions)
                .unwrap_or_else(deny_permissions),
        ),
        update_permissions_policy: Some(
            action_policies
                .and_then(|policies| policies.update_permissions)
                .filter(valid_permissions)
                .unwrap_or_else(deny_permissions),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app_data::migration::synthesize_registry_from_policy_set,
        group_mutable_metadata::MetadataField,
    };
    use openmls::extensions::{AppDataDictionary, AppDataDictionaryExtension, Extension};

    fn policy_set_with_denies() -> PolicySet {
        let mut update_metadata_policy = std::collections::HashMap::new();
        for (field, _, _) in metadata_field_registry_mapping() {
            update_metadata_policy.insert(field.as_str().to_string(), deny_metadata());
        }
        PolicySet {
            add_member_policy: Some(deny_membership()),
            remove_member_policy: Some(deny_membership()),
            update_metadata_policy,
            add_admin_policy: Some(deny_permissions()),
            remove_admin_policy: Some(deny_permissions()),
            update_permissions_policy: Some(deny_permissions()),
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn reconstructs_declared_actions_and_registry_metadata() {
        let expected = policy_set_with_denies();
        let registry = synthesize_registry_from_policy_set(&expected)?;
        let actions = GroupActionPolicies {
            add_member: expected.add_member_policy.clone(),
            remove_member: expected.remove_member_policy.clone(),
            add_admin: expected.add_admin_policy.clone(),
            remove_admin: expected.remove_admin_policy.clone(),
            update_permissions: expected.update_permissions_policy.clone(),
        };
        let mut dictionary = AppDataDictionary::new();
        dictionary.insert(
            ComponentId::COMPONENT_REGISTRY.as_u16(),
            registry.to_bytes()?,
        );
        dictionary.insert(
            ComponentId::GROUP_ACTION_POLICIES.as_u16(),
            actions.encode_to_vec(),
        );
        let extensions = Extensions::from_vec(vec![Extension::AppDataDictionary(
            AppDataDictionaryExtension::new(dictionary),
        )])?;

        assert_eq!(policy_set_from_dictionary(&extensions), expected);
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn preserves_policy_combinator_messages_exactly() {
        let mut expected = policy_set_with_denies();
        expected.add_member_policy = Some(MembershipPolicy {
            kind: Some(MembershipPolicyKind::AndCondition(
                xmtp_proto::xmtp::mls::message_contents::membership_policy::AndCondition {
                    policies: vec![deny_membership(), deny_membership()],
                },
            )),
        });
        expected.update_metadata_policy.insert(
            MetadataField::GroupName.as_str().to_string(),
            MetadataPolicy {
                kind: Some(MetadataPolicyKind::AnyCondition(
                    xmtp_proto::xmtp::mls::message_contents::metadata_policy::AnyCondition {
                        policies: vec![deny_metadata(), deny_metadata()],
                    },
                )),
            },
        );
        let registry = synthesize_registry_from_policy_set(&expected)?;
        let actions = GroupActionPolicies {
            add_member: expected.add_member_policy.clone(),
            remove_member: expected.remove_member_policy.clone(),
            add_admin: expected.add_admin_policy.clone(),
            remove_admin: expected.remove_admin_policy.clone(),
            update_permissions: expected.update_permissions_policy.clone(),
        };
        let mut dictionary = AppDataDictionary::new();
        dictionary.insert(
            ComponentId::COMPONENT_REGISTRY.as_u16(),
            registry.to_bytes()?,
        );
        dictionary.insert(
            ComponentId::GROUP_ACTION_POLICIES.as_u16(),
            actions.encode_to_vec(),
        );
        let extensions = Extensions::from_vec(vec![Extension::AppDataDictionary(
            AppDataDictionaryExtension::new(dictionary),
        )])?;

        let actual = policy_set_from_dictionary(&extensions);
        assert_eq!(actual, expected);
        assert_eq!(
            actual.add_member_policy.as_ref()?.encode_to_vec(),
            expected.add_member_policy.as_ref()?.encode_to_vec()
        );
        assert_eq!(
            actual
                .update_metadata_policy
                .get(MetadataField::GroupName.as_str())?
                .encode_to_vec(),
            expected
                .update_metadata_policy
                .get(MetadataField::GroupName.as_str())?
                .encode_to_vec()
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn missing_dictionary_denies_every_policy() {
        let extensions = Extensions::from_vec(vec![])?;
        let policies = policy_set_from_dictionary(&extensions);
        assert_eq!(policies.add_member_policy, Some(deny_membership()));
        assert_eq!(policies.remove_member_policy, Some(deny_membership()));
        assert_eq!(policies.add_admin_policy, Some(deny_permissions()));
        assert_eq!(policies.remove_admin_policy, Some(deny_permissions()));
        assert_eq!(policies.update_permissions_policy, Some(deny_permissions()));
        assert!(
            policies
                .update_metadata_policy
                .values()
                .all(|p| p == &deny_metadata())
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn malformed_fields_deny_without_changing_valid_fields() {
        let mut expected = policy_set_with_denies();
        expected.remove_member_policy = Some(MembershipPolicy {
            kind: Some(MembershipPolicyKind::Base(
                MembershipBasePolicy::Allow as i32,
            )),
        });
        let mut registry = synthesize_registry_from_policy_set(&expected)?;
        let mut metadata = registry.get(&ComponentId::GROUP_NAME)??;
        metadata.permissions.as_mut()?.update_policy = Some(MetadataPolicy {
            kind: Some(MetadataPolicyKind::AndCondition(
                xmtp_proto::xmtp::mls::message_contents::metadata_policy::AndCondition {
                    policies: vec![],
                },
            )),
        });
        registry.set(ComponentId::GROUP_NAME, metadata)?;
        let actions = GroupActionPolicies {
            // A valid sibling in an OR must not hide an unknown policy.
            add_member: Some(MembershipPolicy {
                kind: Some(MembershipPolicyKind::AnyCondition(
                    xmtp_proto::xmtp::mls::message_contents::membership_policy::AnyCondition {
                        policies: vec![
                            expected.remove_member_policy.clone()?,
                            MembershipPolicy {
                                kind: Some(MembershipPolicyKind::Base(i32::MAX)),
                            },
                        ],
                    },
                )),
            }),
            remove_member: expected.remove_member_policy.clone(),
            add_admin: Some(PermissionsUpdatePolicy {
                kind: Some(PermissionsPolicyKind::Base(i32::MAX)),
            }),
            remove_admin: expected.remove_admin_policy.clone(),
            update_permissions: None,
        };
        let mut dictionary = AppDataDictionary::new();
        dictionary.insert(
            ComponentId::COMPONENT_REGISTRY.as_u16(),
            registry.to_bytes()?,
        );
        dictionary.insert(
            ComponentId::GROUP_ACTION_POLICIES.as_u16(),
            actions.encode_to_vec(),
        );
        let extensions = Extensions::from_vec(vec![Extension::AppDataDictionary(
            AppDataDictionaryExtension::new(dictionary),
        )])?;
        assert_eq!(policy_set_from_dictionary(&extensions), expected);
    }
}
