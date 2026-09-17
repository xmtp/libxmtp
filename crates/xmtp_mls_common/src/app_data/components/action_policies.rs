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
        _registry: &crate::app_data::component_registry::ComponentRegistry,
    ) -> Result<(), ComponentInvariantError> {
        // Registry policy can only check the actor. It cannot ensure that the
        // resulting Bytes value has the declared GroupActionPolicies shape.
        // Decode without re-encoding so valid protobuf bytes, including
        // combinators, remain byte-exact in the dictionary.
        if let Some(bytes) = change.new_value {
            GroupActionPolicies::decode(bytes).map_err(|error| {
                ComponentInvariantError::Violation {
                    component_id: Self::ID,
                    reason: format!("not a valid GroupActionPolicies value: {error}"),
                }
            })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xmtp_proto::xmtp::mls::message_contents::{
        GroupActionPolicies, MembershipPolicy,
        membership_policy::{AndCondition, BasePolicy, Kind as MembershipPolicyKind},
    };

    use crate::app_data::{
        component_registry::ComponentRegistry,
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
        let change = ComponentChange::builder()
            .component_id(ComponentId::GROUP_ACTION_POLICIES)
            .op(ComponentOp::Update)
            .actor(ActorAuthority {
                is_admin: true,
                is_super_admin: true,
            })
            .new_value(&encoded)
            .build();
        GroupActionPoliciesComponent::validate_invariant(&change, &ComponentRegistry::new())?;
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
