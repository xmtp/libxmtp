//! [`Component`] implementation for declared group action policies.
//!
//! The value is the protobuf encoding of `GroupActionPolicies`. It remains a
//! byte component here so the protobuf bytes, including nested policy
//! combinators, round-trip without normalization.

use openmls::messages::proposals::AppDataUpdateOperation;
use prost::Message as _;
use xmtp_proto::xmtp::mls::message_contents::{ComponentType, GroupActionPolicies};

use crate::app_data::{
    component_id::ComponentId,
    component_registry::ComponentOp,
    migration::{
        MigrationError, admin_list_policy_to_metadata_policy, membership_policy_to_metadata_policy,
    },
    typed::{Component, ComponentInvariantError, ComponentTypedError, ExpandedComponentChange},
};

/// The declared policies for membership and administrator actions.
pub struct GroupActionPoliciesComponent;

impl Component for GroupActionPoliciesComponent {
    const ID: ComponentId = ComponentId::GROUP_ACTION_POLICIES;
    const COMPONENT_TYPE: ComponentType = ComponentType::Bytes;
    type Value = Vec<u8>;
    type Mutation = Vec<u8>;

    fn decode_value(bytes: &[u8]) -> Result<Self::Value, ComponentTypedError> {
        Ok(bytes.to_vec())
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, ComponentTypedError> {
        Ok(value.clone())
    }

    fn encode_mutation(mutation: &Self::Mutation) -> Result<Vec<u8>, ComponentTypedError> {
        Ok(mutation.clone())
    }

    fn apply_update_payload(
        payload: &[u8],
        _prior: Option<&[u8]>,
    ) -> Result<Vec<u8>, ComponentTypedError> {
        Ok(payload.to_vec())
    }

    fn expand_to_changes(
        op: &AppDataUpdateOperation,
        _prior: Option<&[u8]>,
    ) -> Result<Vec<ExpandedComponentChange>, ComponentTypedError> {
        match op {
            AppDataUpdateOperation::Update(payload) => Ok(vec![ExpandedComponentChange {
                op: ComponentOp::Update,
                value: Some(payload.as_slice().to_vec()),
            }]),
            AppDataUpdateOperation::Remove => Ok(vec![ExpandedComponentChange {
                op: ComponentOp::Delete,
                value: None,
            }]),
        }
    }

    fn validate_invariant(
        change: &crate::app_data::validation::ComponentChange<'_>,
        registry: &crate::app_data::component_registry::ComponentRegistry,
    ) -> Result<(), ComponentInvariantError> {
        let violation = |reason| ComponentInvariantError::Violation {
            component_id: Self::ID,
            reason,
        };
        let bytes = change
            .new_value
            .ok_or_else(|| violation("group action policies cannot be absent".to_string()))?;
        let policies = GroupActionPolicies::decode(bytes).map_err(|error| {
            violation(format!("not a valid GroupActionPolicies value: {error}"))
        })?;
        // The caller supplies the final registry and declaration for the commit.
        // Use the same conversion as bootstrap and permission-update synthesis.
        for (id, insert, delete) in [
            (
                ComponentId::GROUP_MEMBERSHIP,
                policies
                    .add_member
                    .as_ref()
                    .ok_or(MigrationError::MissingPolicyField("add_member"))
                    .and_then(membership_policy_to_metadata_policy),
                policies
                    .remove_member
                    .as_ref()
                    .ok_or(MigrationError::MissingPolicyField("remove_member"))
                    .and_then(membership_policy_to_metadata_policy),
            ),
            (
                ComponentId::ADMIN_LIST,
                policies
                    .add_admin
                    .as_ref()
                    .ok_or(MigrationError::MissingPolicyField("add_admin"))
                    .and_then(admin_list_policy_to_metadata_policy),
                policies
                    .remove_admin
                    .as_ref()
                    .ok_or(MigrationError::MissingPolicyField("remove_admin"))
                    .and_then(admin_list_policy_to_metadata_policy),
            ),
        ] {
            let insert = insert.map_err(|error| violation(error.to_string()))?;
            let delete = delete.map_err(|error| violation(error.to_string()))?;
            let permissions = registry
                .get(&id)
                .map_err(|error| violation(error.to_string()))?
                .and_then(|metadata| metadata.permissions)
                .ok_or_else(|| violation(format!("missing registry permissions for {id}")))?;
            if permissions.insert_policy.as_ref() != Some(&insert)
                || permissions.delete_policy.as_ref() != Some(&delete)
            {
                return Err(violation(format!(
                    "group action policies disagree with registry permissions for {id}"
                )));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xmtp_proto::xmtp::mls::message_contents::{
        ComponentPermissions, ComponentType, GroupActionPolicies, MembershipPolicy, MetadataPolicy,
        PermissionsUpdatePolicy,
        membership_policy::{AndCondition, BasePolicy, Kind as MembershipPolicyKind},
        metadata_policy::{Kind as MetadataPolicyKind, MetadataBasePolicy},
        permissions_update_policy::{Kind as PermissionsPolicyKind, PermissionsBasePolicy},
    };

    use crate::app_data::{
        component_registry::{ComponentOp, ComponentRegistry, new_component_metadata},
        validation::{ActorAuthority, ComponentChange},
    };

    #[xmtp_common::test(unwrap_try = true)]
    fn round_trips_bytes_without_normalization() {
        let value = GroupActionPolicies {
            add_member: Some(MembershipPolicy {
                kind: Some(MembershipPolicyKind::AndCondition(AndCondition {
                    policies: vec![MembershipPolicy {
                        kind: Some(MembershipPolicyKind::Base(
                            BasePolicy::AllowIfAdminOrSuperAdmin as i32,
                        )),
                    }],
                })),
            }),
            ..Default::default()
        }
        .encode_to_vec();
        let encoded = GroupActionPoliciesComponent::encode_value(&value)?;
        assert_eq!(GroupActionPoliciesComponent::decode_value(&encoded)?, value);
        assert_eq!(
            GroupActionPoliciesComponent::apply_update_payload(&encoded, None)?,
            value
        );
    }

    fn membership_policy(base: BasePolicy) -> MembershipPolicy {
        MembershipPolicy {
            kind: Some(MembershipPolicyKind::Base(base as i32)),
        }
    }

    fn permissions_policy(base: PermissionsBasePolicy) -> PermissionsUpdatePolicy {
        PermissionsUpdatePolicy {
            kind: Some(PermissionsPolicyKind::Base(base as i32)),
        }
    }

    fn metadata_policy(base: MetadataBasePolicy) -> MetadataPolicy {
        MetadataPolicy {
            kind: Some(MetadataPolicyKind::Base(base as i32)),
        }
    }

    fn matching_fixture() -> (GroupActionPolicies, ComponentRegistry) {
        let actions = GroupActionPolicies {
            add_member: Some(membership_policy(BasePolicy::AllowIfAdminOrSuperAdmin)),
            remove_member: Some(membership_policy(BasePolicy::AllowIfSuperAdmin)),
            add_admin: Some(permissions_policy(PermissionsBasePolicy::AllowIfAdmin)),
            remove_admin: Some(permissions_policy(PermissionsBasePolicy::AllowIfSuperAdmin)),
            update_permissions: Some(permissions_policy(PermissionsBasePolicy::AllowIfSuperAdmin)),
        };
        let mut registry = ComponentRegistry::new();
        registry
            .set(
                ComponentId::GROUP_MEMBERSHIP,
                new_component_metadata(
                    ComponentPermissions {
                        insert_policy: Some(metadata_policy(MetadataBasePolicy::AllowIfAdmin)),
                        update_policy: Some(metadata_policy(MetadataBasePolicy::AllowIfAdmin)),
                        delete_policy: Some(metadata_policy(MetadataBasePolicy::AllowIfSuperAdmin)),
                    },
                    ComponentType::TlsMapInboxIdBytes,
                ),
            )
            .unwrap();
        registry
            .set(
                ComponentId::ADMIN_LIST,
                new_component_metadata(
                    ComponentPermissions {
                        insert_policy: Some(metadata_policy(MetadataBasePolicy::AllowIfAdmin)),
                        update_policy: Some(metadata_policy(MetadataBasePolicy::AllowIfAdmin)),
                        delete_policy: Some(metadata_policy(MetadataBasePolicy::AllowIfSuperAdmin)),
                    },
                    ComponentType::TlsSetInboxId,
                ),
            )
            .unwrap();
        (actions, registry)
    }

    fn action_change(value: &[u8]) -> ComponentChange<'_> {
        ComponentChange::builder()
            .component_id(ComponentId::GROUP_ACTION_POLICIES)
            .op(ComponentOp::Update)
            .actor(ActorAuthority {
                is_admin: true,
                is_super_admin: true,
            })
            .new_value(value)
            .build()
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_registry_policy_mismatches_and_accepts_matching_policies() {
        let (actions, registry) = matching_fixture();

        GroupActionPoliciesComponent::validate_invariant(
            &action_change(&actions.encode_to_vec()),
            &registry,
        )?;

        let cases = [
            (
                "membership insert",
                GroupActionPolicies {
                    add_member: Some(membership_policy(BasePolicy::Allow)),
                    ..actions.clone()
                },
            ),
            (
                "membership delete",
                GroupActionPolicies {
                    remove_member: Some(membership_policy(BasePolicy::Allow)),
                    ..actions.clone()
                },
            ),
            (
                "admin insert",
                GroupActionPolicies {
                    add_admin: Some(permissions_policy(PermissionsBasePolicy::Deny)),
                    ..actions.clone()
                },
            ),
            (
                "admin delete",
                GroupActionPolicies {
                    remove_admin: Some(permissions_policy(PermissionsBasePolicy::AllowIfAdmin)),
                    ..actions
                },
            ),
        ];
        for (name, mismatched) in cases {
            let error = GroupActionPoliciesComponent::validate_invariant(
                &action_change(&mismatched.encode_to_vec()),
                &registry,
            )
            .unwrap_err();
            assert!(
                matches!(error, ComponentInvariantError::Violation { .. }),
                "{name}: {error:?}"
            );
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_malformed_post_state() {
        let malformed = [0x0a];
        let change = ComponentChange::builder()
            .component_id(ComponentId::GROUP_ACTION_POLICIES)
            .op(ComponentOp::Update)
            .actor(ActorAuthority {
                is_admin: true,
                is_super_admin: true,
            })
            .new_value(&malformed)
            .build();

        GroupActionPoliciesComponent::validate_invariant(&change, &ComponentRegistry::new())
            .unwrap_err();
    }
}
