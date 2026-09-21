//! Project the public policy view from the component registry.
//!
//! Exact round trips cover nonempty membership And/Any trees over the four
//! defined base policies, base admin policies (Deny, AllowIfAdmin,
//! AllowIfSuperAdmin), super-admin permission updates, and complete maps of
//! valid supported metadata policies. Child order, duplicate
//! children, and all condition wrappers are preserved.
//!
//! This is not an identity for arbitrary PolicySet values. Admin combinators
//! have no forward mapping. Permission updates always require a super admin
//! (including DMs, whose legacy policy is Deny). Creation fills sparse metadata
//! maps with defaults. These are existing limits, not additional stored state.

use openmls::{extensions::Extensions, group::GroupContext};
use xmtp_proto::xmtp::mls::message_contents::{
    MembershipPolicy, MetadataPolicy, PermissionsUpdatePolicy, PolicySet,
    membership_policy::{self, BasePolicy as MembershipBasePolicy, Kind as MembershipPolicyKind},
    metadata_policy::{Kind as MetadataPolicyKind, MetadataBasePolicy},
    permissions_update_policy::{Kind as PermissionsPolicyKind, PermissionsBasePolicy},
};

use crate::app_data::{
    component_id::ComponentId,
    component_registry::{ComponentOp, ComponentRegistry, ComponentRegistryError},
    creation::metadata_field_registry_mapping,
};

#[derive(Debug, thiserror::Error)]
pub enum PolicyProjectionError {
    #[error("component registry is missing")]
    MissingRegistry,
    #[error(transparent)]
    Registry(#[from] ComponentRegistryError),
    #[error("invalid membership policy: {0:?}")]
    InvalidMembershipPolicy(Option<i32>),
    #[error("admin policy must be Deny, AllowIfAdmin, or AllowIfSuperAdmin: {0:?}")]
    InvalidAdminPolicy(Option<i32>),
}

/// Invert the membership mapping without changing the policy tree.
/// Visit all children. An earlier Allow must not hide an invalid OR child.
pub fn metadata_policy_to_membership_policy(
    policy: &MetadataPolicy,
) -> Result<MembershipPolicy, PolicyProjectionError> {
    let kind = match &policy.kind {
        Some(MetadataPolicyKind::Base(base)) => {
            let base = match MetadataBasePolicy::try_from(*base) {
                Ok(MetadataBasePolicy::Allow) => MembershipBasePolicy::Allow,
                Ok(MetadataBasePolicy::Deny) => MembershipBasePolicy::Deny,
                // Both evaluators use is_admin || is_super_admin.
                Ok(MetadataBasePolicy::AllowIfAdmin) => {
                    MembershipBasePolicy::AllowIfAdminOrSuperAdmin
                }
                Ok(MetadataBasePolicy::AllowIfSuperAdmin) => {
                    MembershipBasePolicy::AllowIfSuperAdmin
                }
                _ => return Err(PolicyProjectionError::InvalidMembershipPolicy(Some(*base))),
            };
            MembershipPolicyKind::Base(base as i32)
        }
        Some(MetadataPolicyKind::AndCondition(condition)) if !condition.policies.is_empty() => {
            MembershipPolicyKind::AndCondition(membership_policy::AndCondition {
                policies: condition
                    .policies
                    .iter()
                    .map(metadata_policy_to_membership_policy)
                    .collect::<Result<_, _>>()?,
            })
        }
        Some(MetadataPolicyKind::AnyCondition(condition)) if !condition.policies.is_empty() => {
            MembershipPolicyKind::AnyCondition(membership_policy::AnyCondition {
                policies: condition
                    .policies
                    .iter()
                    .map(metadata_policy_to_membership_policy)
                    .collect::<Result<_, _>>()?,
            })
        }
        _ => return Err(PolicyProjectionError::InvalidMembershipPolicy(None)),
    };
    Ok(MembershipPolicy { kind: Some(kind) })
}

/// Invert the constrained admin mapping. Combinators are not supported.
pub fn metadata_policy_to_admin_policy(
    policy: &MetadataPolicy,
) -> Result<PermissionsUpdatePolicy, PolicyProjectionError> {
    let Some(MetadataPolicyKind::Base(base)) = policy.kind else {
        return Err(PolicyProjectionError::InvalidAdminPolicy(None));
    };
    let mapped = match MetadataBasePolicy::try_from(base) {
        Ok(MetadataBasePolicy::Deny) => PermissionsBasePolicy::Deny,
        Ok(MetadataBasePolicy::AllowIfAdmin) => PermissionsBasePolicy::AllowIfAdmin,
        Ok(MetadataBasePolicy::AllowIfSuperAdmin) => PermissionsBasePolicy::AllowIfSuperAdmin,
        _ => return Err(PolicyProjectionError::InvalidAdminPolicy(Some(base))),
    };
    Ok(PermissionsUpdatePolicy {
        kind: Some(PermissionsPolicyKind::Base(mapped as i32)),
    })
}

fn registry_policy(
    registry: &ComponentRegistry,
    id: ComponentId,
    op: ComponentOp,
) -> Result<MetadataPolicy, PolicyProjectionError> {
    let permissions = registry
        .get(&id)?
        .and_then(|metadata| metadata.permissions)
        .ok_or(ComponentRegistryError::MissingPermissions(id))?;
    let policy = match op {
        ComponentOp::Insert => permissions.insert_policy,
        ComponentOp::Update => permissions.update_policy,
        ComponentOp::Delete => permissions.delete_policy,
    };
    Ok(policy.ok_or(ComponentRegistryError::MissingPolicyField(id, op))?)
}

/// Require the action entries and validate each complete action policy tree.
/// Use this on Welcome admission and on the final registry of a commit.
// implements: PERM-017
pub fn validate_registry_action_policies(
    registry: &ComponentRegistry,
) -> Result<(), PolicyProjectionError> {
    for op in [ComponentOp::Insert, ComponentOp::Delete] {
        metadata_policy_to_membership_policy(&registry_policy(
            registry,
            ComponentId::GROUP_MEMBERSHIP,
            op,
        )?)?;
        metadata_policy_to_admin_policy(&registry_policy(registry, ComponentId::ADMIN_LIST, op)?)?;
    }
    Ok(())
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

/// Read the four action slots from COMPONENT_REGISTRY alone.
/// Invalid action trees return an error; they never become a different policy.
/// Metadata fields retain their existing per-field deny fallback.
// implements: PERM-024
pub fn policy_set_from_dictionary(
    extensions: &Extensions<GroupContext>,
) -> Result<PolicySet, PolicyProjectionError> {
    let bytes = extensions
        .app_data_dictionary()
        .and_then(|dictionary| {
            dictionary
                .dictionary()
                .get(&ComponentId::COMPONENT_REGISTRY.as_u16())
        })
        .ok_or(PolicyProjectionError::MissingRegistry)?;
    let registry = ComponentRegistry::from_bytes(bytes)?;
    let membership = |op| {
        metadata_policy_to_membership_policy(&registry_policy(
            &registry,
            ComponentId::GROUP_MEMBERSHIP,
            op,
        )?)
    };
    let admin = |op| {
        metadata_policy_to_admin_policy(&registry_policy(&registry, ComponentId::ADMIN_LIST, op)?)
    };
    let mut update_metadata_policy = std::collections::HashMap::new();
    for (field, component_id, _) in metadata_field_registry_mapping() {
        let policy = xmtp_common::optify!(
            registry_policy(&registry, *component_id, ComponentOp::Update),
            "registry metadata policy is missing or malformed"
        )
        // This view validates the whole metadata tree. Enforcement can
        // short-circuit OR, so malformed trailing children can give a
        // different result. This pre-existing difference is retained.
        .filter(valid_metadata)
        .unwrap_or(MetadataPolicy {
            kind: Some(MetadataPolicyKind::Base(MetadataBasePolicy::Deny as i32)),
        });
        update_metadata_policy.insert(field.as_str().to_string(), policy);
    }
    Ok(PolicySet {
        add_member_policy: Some(membership(ComponentOp::Insert)?),
        remove_member_policy: Some(membership(ComponentOp::Delete)?),
        add_admin_policy: Some(admin(ComponentOp::Insert)?),
        remove_admin_policy: Some(admin(ComponentOp::Delete)?),
        update_metadata_policy,
        // No stored value controls permission-update authority.
        update_permissions_policy: Some(PermissionsUpdatePolicy {
            kind: Some(PermissionsPolicyKind::Base(
                PermissionsBasePolicy::AllowIfSuperAdmin as i32,
            )),
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_data::creation::{
        admin_list_policy_to_metadata_policy, membership_policy_to_metadata_policy,
        synthesize_registry_from_policy_set,
    };
    use openmls::extensions::{AppDataDictionary, AppDataDictionaryExtension, Extension};
    use prost::Message;
    use xmtp_proto::xmtp::mls::message_contents::metadata_policy::{AndCondition, AnyCondition};

    fn base(base: MetadataBasePolicy) -> MetadataPolicy {
        MetadataPolicy {
            kind: Some(MetadataPolicyKind::Base(base as i32)),
        }
    }

    fn policy_set() -> PolicySet {
        PolicySet {
            add_member_policy: Some(
                metadata_policy_to_membership_policy(&base(MetadataBasePolicy::Allow)).unwrap(),
            ),
            remove_member_policy: Some(
                metadata_policy_to_membership_policy(&base(MetadataBasePolicy::AllowIfAdmin))
                    .unwrap(),
            ),
            add_admin_policy: Some(
                metadata_policy_to_admin_policy(&base(MetadataBasePolicy::AllowIfSuperAdmin))
                    .unwrap(),
            ),
            remove_admin_policy: Some(
                metadata_policy_to_admin_policy(&base(MetadataBasePolicy::AllowIfSuperAdmin))
                    .unwrap(),
            ),
            update_permissions_policy: Some(
                metadata_policy_to_admin_policy(&base(MetadataBasePolicy::AllowIfSuperAdmin))
                    .unwrap(),
            ),
            update_metadata_policy: metadata_field_registry_mapping()
                .iter()
                .map(|(field, _, _)| {
                    (
                        field.as_str().to_string(),
                        base(MetadataBasePolicy::AllowIfAdmin),
                    )
                })
                .collect(),
        }
    }

    fn extensions(registry: &ComponentRegistry) -> Extensions<GroupContext> {
        let mut dictionary = AppDataDictionary::new();
        dictionary.insert(
            ComponentId::COMPONENT_REGISTRY.as_u16(),
            registry.to_bytes().unwrap(),
        );
        Extensions::from_vec(vec![Extension::AppDataDictionary(
            AppDataDictionaryExtension::new(dictionary),
        )])
        .unwrap()
    }

    #[xmtp_common::test(unwrap_try = true)]
    // verifies: PERM-024
    fn update_permissions_reports_enforced_super_admin_policy() {
        for stored in [
            MetadataBasePolicy::Deny,
            MetadataBasePolicy::AllowIfAdmin,
            MetadataBasePolicy::AllowIfSuperAdmin,
        ] {
            let mut expected = policy_set();
            expected.update_permissions_policy =
                Some(metadata_policy_to_admin_policy(&base(stored))?);
            let registry = synthesize_registry_from_policy_set(&expected)?;
            assert_eq!(
                policy_set_from_dictionary(&extensions(&registry))?.update_permissions_policy,
                policy_set().update_permissions_policy
            );
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn inverse_preserves_nested_membership_trees_and_all_base_variants() {
        let leaves = [
            MetadataBasePolicy::Allow,
            MetadataBasePolicy::Deny,
            MetadataBasePolicy::AllowIfAdmin,
            MetadataBasePolicy::AllowIfSuperAdmin,
        ];
        for leaf in leaves {
            let tree = MetadataPolicy {
                kind: Some(MetadataPolicyKind::AndCondition(AndCondition {
                    policies: vec![
                        base(leaf),
                        MetadataPolicy {
                            kind: Some(MetadataPolicyKind::AnyCondition(AnyCondition {
                                policies: vec![
                                    base(MetadataBasePolicy::Deny),
                                    base(leaf),
                                    base(leaf),
                                ],
                            })),
                        },
                    ],
                })),
            };
            let membership = metadata_policy_to_membership_policy(&tree)?;
            assert_eq!(
                membership_policy_to_metadata_policy(&membership)?.encode_to_vec(),
                tree.encode_to_vec()
            );
            let mut expected = policy_set();
            expected.add_member_policy = Some(membership);
            let registry = synthesize_registry_from_policy_set(&expected)?;
            assert_eq!(
                policy_set_from_dictionary(&extensions(&registry))?,
                expected
            );
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    // verifies: PERM-017
    fn inverse_rejects_malformed_trailing_or_child() {
        for bad in [
            None,
            Some(MetadataPolicyKind::Base(0)),
            Some(MetadataPolicyKind::Base(i32::MAX)),
        ] {
            let tree = MetadataPolicy {
                kind: Some(MetadataPolicyKind::AnyCondition(AnyCondition {
                    policies: vec![
                        base(MetadataBasePolicy::Allow),
                        MetadataPolicy { kind: bad },
                    ],
                })),
            };
            assert!(metadata_policy_to_membership_policy(&tree).is_err());
            let mut registry = synthesize_registry_from_policy_set(&policy_set())?;
            let mut metadata = registry.get(&ComponentId::GROUP_MEMBERSHIP)??;
            metadata.permissions.as_mut()?.insert_policy = Some(tree);
            registry.set(ComponentId::GROUP_MEMBERSHIP, metadata)?;
            assert!(validate_registry_action_policies(&registry).is_err());
            assert!(policy_set_from_dictionary(&extensions(&registry)).is_err());
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    // verifies: PERM-017
    fn inverse_rejects_admin_combinators_and_preserves_supported_bases() {
        for leaf in [
            MetadataBasePolicy::Deny,
            MetadataBasePolicy::AllowIfAdmin,
            MetadataBasePolicy::AllowIfSuperAdmin,
        ] {
            let policy = base(leaf);
            assert_eq!(
                admin_list_policy_to_metadata_policy(&metadata_policy_to_admin_policy(&policy)?)?,
                policy
            );
            for kind in [
                MetadataPolicyKind::AndCondition(AndCondition {
                    policies: vec![policy.clone()],
                }),
                MetadataPolicyKind::AnyCondition(AnyCondition {
                    policies: vec![policy],
                }),
            ] {
                assert!(
                    metadata_policy_to_admin_policy(&MetadataPolicy { kind: Some(kind) }).is_err()
                );
            }
        }
        assert!(metadata_policy_to_admin_policy(&base(MetadataBasePolicy::Allow)).is_err());
    }

    #[xmtp_common::test(unwrap_try = true)]
    // verifies: PERM-017
    fn action_entries_cannot_be_absent() {
        for id in [ComponentId::GROUP_MEMBERSHIP, ComponentId::ADMIN_LIST] {
            let mut registry = synthesize_registry_from_policy_set(&policy_set())?;
            registry.remove(&id)?;
            assert!(validate_registry_action_policies(&registry).is_err());
            assert!(policy_set_from_dictionary(&extensions(&registry)).is_err());
        }
        assert!(policy_set_from_dictionary(&Extensions::default()).is_err());
    }
}
