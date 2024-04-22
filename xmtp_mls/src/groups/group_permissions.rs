use std::collections::HashMap;

use prost::Message;
use thiserror::Error;

use xmtp_proto::xmtp::mls::message_contents::{
    membership_policy::{
        AndCondition as AndConditionProto, AnyCondition as AnyConditionProto,
        BasePolicy as BasePolicyProto, Kind as PolicyKindProto,
    },
    metadata_policy::{
        AndCondition as MetadataAndConditionProto, AnyCondition as MetadataAnyConditionProto,
        Kind as MetadataPolicyKindProto, MetadataBasePolicy as MetadataBasePolicyProto,
    },
    MembershipPolicy as MembershipPolicyProto, MetadataPolicy as MetadataPolicyProto,
    PolicySet as PolicySetProto,
};

use super::{
    group_mutable_metadata::{GroupMutableMetadata, GroupMutableMetadataError},
    validated_commit::{AggregatedMembershipChange, CommitParticipant, ValidatedCommit},
};

// A trait for policies that can update Metadata for the group

pub trait MetadataPolicy: std::fmt::Debug {
    // Verify relevant metadata is actually changed before evaluating against the MetadataPolicy
    // See evaluate_metadata_policy
    fn evaluate(&self, actor: &CommitParticipant, change: &MetadataChange) -> bool;
    fn to_proto(&self) -> Result<MetadataPolicyProto, PolicyError>;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MetadataBasePolicies {
    Allow,
    Deny,
    // Allow the change if the actor is the creator of the group
    AllowIfActorCreator,
    // AllowIfActorAdmin, TODO: Enable this once we have admin roles
}

impl MetadataPolicy for &MetadataBasePolicies {
    fn evaluate(&self, actor: &CommitParticipant, _change: &MetadataChange) -> bool {
        match self {
            MetadataBasePolicies::Allow => true,
            MetadataBasePolicies::Deny => false,
            MetadataBasePolicies::AllowIfActorCreator => actor.is_creator,
        }
    }

    fn to_proto(&self) -> Result<MetadataPolicyProto, PolicyError> {
        let inner = match self {
            MetadataBasePolicies::Allow => MetadataBasePolicyProto::Allow as i32,
            MetadataBasePolicies::Deny => MetadataBasePolicyProto::Deny as i32,
            MetadataBasePolicies::AllowIfActorCreator => {
                MetadataBasePolicyProto::AllowIfActorCreator as i32
            }
        };

        Ok(MetadataPolicyProto {
            kind: Some(MetadataPolicyKindProto::Base(inner)),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum MetadataPolicies {
    Standard(MetadataBasePolicies),
    AndCondition(MetadataAndCondition),
    AnyCondition(MetadataAnyCondition),
}

impl MetadataPolicies {
    pub fn default_map(policies: MetadataPolicies) -> HashMap<String, MetadataPolicies> {
        let mut map: HashMap<String, MetadataPolicies> = HashMap::new();
        for field in GroupMutableMetadata::supported_fields() {
            map.insert(field.to_string(), policies.clone());
        }
        map
    }

    pub fn allow() -> Self {
        MetadataPolicies::Standard(MetadataBasePolicies::Allow)
    }

    pub fn deny() -> Self {
        MetadataPolicies::Standard(MetadataBasePolicies::Deny)
    }

    #[allow(dead_code)]
    pub fn allow_if_actor_creator() -> Self {
        MetadataPolicies::Standard(MetadataBasePolicies::AllowIfActorCreator)
    }

    pub fn and(policies: Vec<MetadataPolicies>) -> Self {
        MetadataPolicies::AndCondition(MetadataAndCondition::new(policies))
    }

    pub fn any(policies: Vec<MetadataPolicies>) -> Self {
        MetadataPolicies::AnyCondition(MetadataAnyCondition::new(policies))
    }
}

// Information for Metadata Update used for validation
#[derive(Clone, Debug)]
pub struct MetadataChange {
    pub(crate) old_value: GroupMutableMetadata,
    pub(crate) new_value: GroupMutableMetadata,
    pub(crate) metadata_policies: HashMap<String, MetadataPolicies>,
}

impl Default for MetadataChange {
    fn default() -> Self {
        Self {
            old_value: GroupMutableMetadata::default(),
            new_value: GroupMutableMetadata::default(),
            metadata_policies: MetadataPolicies::default_map(MetadataPolicies::allow()),
        }
    }
}

impl TryFrom<MetadataPolicyProto> for MetadataPolicies {
    type Error = PolicyError;

    fn try_from(proto: MetadataPolicyProto) -> Result<Self, Self::Error> {
        match proto.kind {
            Some(MetadataPolicyKindProto::Base(inner)) => match inner {
                1 => Ok(MetadataPolicies::allow()),
                2 => Ok(MetadataPolicies::deny()),
                3 => Ok(MetadataPolicies::allow_if_actor_creator()),
                _ => Err(PolicyError::InvalidPolicy),
            },
            Some(MetadataPolicyKindProto::AndCondition(inner)) => {
                if inner.policies.is_empty() {
                    return Err(PolicyError::InvalidPolicy);
                }
                let policies = inner
                    .policies
                    .into_iter()
                    .map(|policy| policy.try_into())
                    .collect::<Result<Vec<MetadataPolicies>, PolicyError>>()?;

                Ok(MetadataPolicies::and(policies))
            }
            Some(MetadataPolicyKindProto::AnyCondition(inner)) => {
                if inner.policies.is_empty() {
                    return Err(PolicyError::InvalidPolicy);
                }

                let policies = inner
                    .policies
                    .into_iter()
                    .map(|policy| policy.try_into())
                    .collect::<Result<Vec<MetadataPolicies>, PolicyError>>()?;

                Ok(MetadataPolicies::any(policies))
            }
            None => Err(PolicyError::InvalidPolicy),
        }
    }
}

impl MetadataPolicy for MetadataPolicies {
    fn evaluate(&self, actor: &CommitParticipant, change: &MetadataChange) -> bool {
        match self {
            MetadataPolicies::Standard(policy) => policy.evaluate(actor, change),
            MetadataPolicies::AndCondition(policy) => policy.evaluate(actor, change),
            MetadataPolicies::AnyCondition(policy) => policy.evaluate(actor, change),
        }
    }

    fn to_proto(&self) -> Result<MetadataPolicyProto, PolicyError> {
        Ok(match self {
            MetadataPolicies::Standard(policy) => policy.to_proto()?,
            MetadataPolicies::AndCondition(policy) => policy.to_proto()?,
            MetadataPolicies::AnyCondition(policy) => policy.to_proto()?,
        })
    }
}

// An AndCondition evaluates to true if all the policies it contains evaluate to true
#[derive(Clone, Debug, PartialEq)]
pub struct MetadataAndCondition {
    policies: Vec<MetadataPolicies>,
}

impl MetadataAndCondition {
    pub(super) fn new(policies: Vec<MetadataPolicies>) -> Self {
        Self { policies }
    }
}

impl MetadataPolicy for MetadataAndCondition {
    fn evaluate(&self, actor: &CommitParticipant, change: &MetadataChange) -> bool {
        self.policies
            .iter()
            .all(|policy| policy.evaluate(actor, change))
    }

    fn to_proto(&self) -> Result<MetadataPolicyProto, PolicyError> {
        Ok(MetadataPolicyProto {
            kind: Some(MetadataPolicyKindProto::AndCondition(
                MetadataAndConditionProto {
                    policies: self
                        .policies
                        .iter()
                        .map(|policy| policy.to_proto())
                        .collect::<Result<Vec<MetadataPolicyProto>, PolicyError>>()?,
                },
            )),
        })
    }
}

// An AnyCondition evaluates to true if any of the contained policies evaluate to true
#[derive(Clone, Debug, PartialEq)]
pub struct MetadataAnyCondition {
    policies: Vec<MetadataPolicies>,
}

#[allow(dead_code)]
impl MetadataAnyCondition {
    pub(super) fn new(policies: Vec<MetadataPolicies>) -> Self {
        Self { policies }
    }
}

impl MetadataPolicy for MetadataAnyCondition {
    fn evaluate(&self, actor: &CommitParticipant, change: &MetadataChange) -> bool {
        self.policies
            .iter()
            .any(|policy| policy.evaluate(actor, change))
    }

    fn to_proto(&self) -> Result<MetadataPolicyProto, PolicyError> {
        Ok(MetadataPolicyProto {
            kind: Some(MetadataPolicyKindProto::AnyCondition(
                MetadataAnyConditionProto {
                    policies: self
                        .policies
                        .iter()
                        .map(|policy| policy.to_proto())
                        .collect::<Result<Vec<MetadataPolicyProto>, PolicyError>>()?,
                },
            )),
        })
    }
}

// A trait for policies that can add/remove members and installations for the group
pub trait MembershipPolicy: std::fmt::Debug {
    fn evaluate(&self, actor: &CommitParticipant, change: &AggregatedMembershipChange) -> bool;
    fn to_proto(&self) -> Result<MembershipPolicyProto, PolicyError>;
}

#[derive(Debug, Error)]
pub enum PolicyError {
    #[error("serialization {0}")]
    Serialization(#[from] prost::EncodeError),
    #[error("deserialization {0}")]
    Deserialization(#[from] prost::DecodeError),
    #[error("invalid policy")]
    InvalidPolicy,
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(dead_code)]
#[repr(u8)]
pub enum BasePolicies {
    Allow,
    Deny,
    // Allow if the change only applies to subject installations with the same account address as the actor
    AllowSameMember,
    // Allow the change if the actor is the creator of the group
    AllowIfActorCreator,
    // AllowIfActorAdmin, TODO: Enable this once we have admin roles
}

impl MembershipPolicy for BasePolicies {
    fn evaluate(&self, actor: &CommitParticipant, change: &AggregatedMembershipChange) -> bool {
        match self {
            BasePolicies::Allow => true,
            BasePolicies::Deny => false,
            BasePolicies::AllowSameMember => change.account_address == actor.account_address,
            BasePolicies::AllowIfActorCreator => actor.is_creator,
        }
    }

    fn to_proto(&self) -> Result<MembershipPolicyProto, PolicyError> {
        let inner = match self {
            BasePolicies::Allow => BasePolicyProto::Allow as i32,
            BasePolicies::Deny => BasePolicyProto::Deny as i32,
            BasePolicies::AllowSameMember => return Err(PolicyError::InvalidPolicy), // AllowSameMember is not needed on any of the wire format protos
            BasePolicies::AllowIfActorCreator => BasePolicyProto::AllowIfActorCreator as i32,
        };

        Ok(MembershipPolicyProto {
            kind: Some(PolicyKindProto::Base(inner)),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum MembershipPolicies {
    Standard(BasePolicies),
    AndCondition(AndCondition),
    AnyCondition(AnyCondition),
}

impl MembershipPolicies {
    pub fn allow() -> Self {
        MembershipPolicies::Standard(BasePolicies::Allow)
    }

    pub fn deny() -> Self {
        MembershipPolicies::Standard(BasePolicies::Deny)
    }

    #[allow(dead_code)]
    pub fn allow_same_member() -> Self {
        MembershipPolicies::Standard(BasePolicies::AllowSameMember)
    }

    #[allow(dead_code)]
    pub fn allow_if_actor_creator() -> Self {
        MembershipPolicies::Standard(BasePolicies::AllowIfActorCreator)
    }

    pub fn and(policies: Vec<MembershipPolicies>) -> Self {
        MembershipPolicies::AndCondition(AndCondition::new(policies))
    }

    pub fn any(policies: Vec<MembershipPolicies>) -> Self {
        MembershipPolicies::AnyCondition(AnyCondition::new(policies))
    }
}

impl TryFrom<MembershipPolicyProto> for MembershipPolicies {
    type Error = PolicyError;

    fn try_from(proto: MembershipPolicyProto) -> Result<Self, Self::Error> {
        match proto.kind {
            Some(PolicyKindProto::Base(inner)) => match inner {
                1 => Ok(MembershipPolicies::allow()),
                2 => Ok(MembershipPolicies::deny()),
                3 => Ok(MembershipPolicies::allow_if_actor_creator()),
                _ => Err(PolicyError::InvalidPolicy),
            },
            Some(PolicyKindProto::AndCondition(inner)) => {
                if inner.policies.is_empty() {
                    return Err(PolicyError::InvalidPolicy);
                }
                let policies = inner
                    .policies
                    .into_iter()
                    .map(|policy| policy.try_into())
                    .collect::<Result<Vec<MembershipPolicies>, PolicyError>>()?;

                Ok(MembershipPolicies::and(policies))
            }
            Some(PolicyKindProto::AnyCondition(inner)) => {
                if inner.policies.is_empty() {
                    return Err(PolicyError::InvalidPolicy);
                }

                let policies = inner
                    .policies
                    .into_iter()
                    .map(|policy| policy.try_into())
                    .collect::<Result<Vec<MembershipPolicies>, PolicyError>>()?;

                Ok(MembershipPolicies::any(policies))
            }
            None => Err(PolicyError::InvalidPolicy),
        }
    }
}

impl MembershipPolicy for MembershipPolicies {
    fn evaluate(&self, actor: &CommitParticipant, change: &AggregatedMembershipChange) -> bool {
        match self {
            MembershipPolicies::Standard(policy) => policy.evaluate(actor, change),
            MembershipPolicies::AndCondition(policy) => policy.evaluate(actor, change),
            MembershipPolicies::AnyCondition(policy) => policy.evaluate(actor, change),
        }
    }

    fn to_proto(&self) -> Result<MembershipPolicyProto, PolicyError> {
        Ok(match self {
            MembershipPolicies::Standard(policy) => policy.to_proto()?,
            MembershipPolicies::AndCondition(policy) => policy.to_proto()?,
            MembershipPolicies::AnyCondition(policy) => policy.to_proto()?,
        })
    }
}

// An AndCondition evaluates to true if all the policies it contains evaluate to true
#[derive(Clone, Debug, PartialEq)]
pub struct AndCondition {
    policies: Vec<MembershipPolicies>,
}

impl AndCondition {
    pub(super) fn new(policies: Vec<MembershipPolicies>) -> Self {
        Self { policies }
    }
}

impl MembershipPolicy for AndCondition {
    fn evaluate(&self, actor: &CommitParticipant, change: &AggregatedMembershipChange) -> bool {
        self.policies
            .iter()
            .all(|policy| policy.evaluate(actor, change))
    }

    fn to_proto(&self) -> Result<MembershipPolicyProto, PolicyError> {
        Ok(MembershipPolicyProto {
            kind: Some(PolicyKindProto::AndCondition(AndConditionProto {
                policies: self
                    .policies
                    .iter()
                    .map(|policy| policy.to_proto())
                    .collect::<Result<Vec<MembershipPolicyProto>, PolicyError>>()?,
            })),
        })
    }
}

// An AnyCondition evaluates to true if any of the contained policies evaluate to true
#[derive(Clone, Debug, PartialEq)]
pub struct AnyCondition {
    policies: Vec<MembershipPolicies>,
}

#[allow(dead_code)]
impl AnyCondition {
    pub(super) fn new(policies: Vec<MembershipPolicies>) -> Self {
        Self { policies }
    }
}

impl MembershipPolicy for AnyCondition {
    fn evaluate(&self, actor: &CommitParticipant, change: &AggregatedMembershipChange) -> bool {
        self.policies
            .iter()
            .any(|policy| policy.evaluate(actor, change))
    }

    fn to_proto(&self) -> Result<MembershipPolicyProto, PolicyError> {
        Ok(MembershipPolicyProto {
            kind: Some(PolicyKindProto::AnyCondition(AnyConditionProto {
                policies: self
                    .policies
                    .iter()
                    .map(|policy| policy.to_proto())
                    .collect::<Result<Vec<MembershipPolicyProto>, PolicyError>>()?,
            })),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub struct PolicySet {
    pub add_member_policy: MembershipPolicies,
    pub remove_member_policy: MembershipPolicies,
    pub add_installation_policy: MembershipPolicies,
    pub remove_installation_policy: MembershipPolicies,
    pub update_metadata_policy: HashMap<String, MetadataPolicies>,
}

fn extract_field_changed(change: &MetadataChange) -> Result<String, GroupMutableMetadataError> {
    let changes: Vec<&String> = change
        .old_value
        .attributes
        .iter()
        .filter(|(key, old_value)| {
            match change.new_value.attributes.get(*key) {
                Some(new_value) => &new_value != old_value,
                None => true, // Assuming missing keys in `new_value` count as changes
            }
        })
        .map(|(key, _)| key)
        .collect();

    match changes.len() {
        1 => Ok(changes[0].clone()), // There is exactly one change
        0 => Err(GroupMutableMetadataError::NoUpdates),
        _ => Err(GroupMutableMetadataError::TooManyUpdates),
    }
}

#[allow(dead_code)]
impl PolicySet {
    pub fn new(
        add_member_policy: MembershipPolicies,
        remove_member_policy: MembershipPolicies,
        update_metadata_policy: HashMap<String, MetadataPolicies>,
    ) -> Self {
        Self {
            add_member_policy,
            remove_member_policy,
            add_installation_policy: default_add_installation_policy(),
            remove_installation_policy: default_remove_installation_policy(),
            update_metadata_policy,
        }
    }

    pub fn evaluate_commit(&self, commit: &ValidatedCommit) -> bool {
        self.evaluate_policy(
            commit.members_added.iter(),
            &self.add_member_policy,
            &commit.actor,
        ) && self.evaluate_policy(
            commit.members_removed.iter(),
            &self.remove_member_policy,
            &commit.actor,
        ) && self.evaluate_policy(
            commit.installations_added.iter(),
            &self.add_installation_policy,
            &commit.actor,
        ) && self.evaluate_policy(
            commit.installations_removed.iter(),
            &self.remove_installation_policy,
            &commit.actor,
        ) & self.evaluate_metadata_policy(&commit.group_name_updated, &commit.actor)
    }

    fn evaluate_policy<'a, I, P>(
        &self,
        mut changes: I,
        policy: &P,
        actor: &CommitParticipant,
    ) -> bool
    where
        I: Iterator<Item = &'a AggregatedMembershipChange>,
        P: MembershipPolicy + std::fmt::Debug,
    {
        changes.all(|change| {
            let is_ok = policy.evaluate(actor, change);
            if !is_ok {
                log::info!(
                    "Policy {:?} failed for actor {:?} and change {:?}",
                    policy,
                    actor,
                    change
                );
            }
            is_ok
        })
    }

    // In case group creator is on future version of libxmtp, we can validate
    // metadata policies on new unknown fields
    fn evaluate_metadata_policy(&self, change: &MetadataChange, actor: &CommitParticipant) -> bool {
        #[allow(clippy::needless_late_init)]
        let field_changed;
        match extract_field_changed(change) {
            Ok(f) => field_changed = f,
            Err(error) => {
                match error {
                    // If there is no change in metadata, no need to validate the policy
                    GroupMutableMetadataError::NoUpdates => return true,
                    _ => {
                        log::info!(
                            "Change extraction failed for actor {:?} and change {:?}",
                            actor,
                            change
                        );
                        return false;
                    }
                }
            }
        }

        let policy = change.metadata_policies.get(&field_changed).unwrap();

        let is_ok = policy.evaluate(actor, change);
        if !is_ok {
            log::info!(
                "Policy {:?} failed for actor {:?} and change {:?}",
                policy,
                actor,
                change
            );
        }
        is_ok
    }

    pub(crate) fn to_proto(&self) -> Result<PolicySetProto, PolicyError> {
        let add_member_policy = Some(self.add_member_policy.to_proto()?);
        let remove_member_policy = Some(self.remove_member_policy.to_proto()?);

        let mut update_metadata_policy = HashMap::new();
        for (key, policy) in &self.update_metadata_policy {
            let policy_proto = policy.to_proto()?;
            update_metadata_policy.insert(key.clone(), policy_proto);
        }
        Ok(PolicySetProto {
            add_member_policy,
            remove_member_policy,
            update_metadata_policy,
        })
    }

    pub(crate) fn from_proto(proto: PolicySetProto) -> Result<Self, PolicyError> {
        let add_member_policy = MembershipPolicies::try_from(
            proto.add_member_policy.ok_or(PolicyError::InvalidPolicy)?,
        )?;
        let remove_member_policy = MembershipPolicies::try_from(
            proto
                .remove_member_policy
                .ok_or(PolicyError::InvalidPolicy)?,
        )?;

        let mut update_metadata_policy = HashMap::new();
        for (key, policy_proto) in proto.update_metadata_policy {
            let policy = MetadataPolicies::try_from(policy_proto)?;
            update_metadata_policy.insert(key, policy);
        }
        Ok(Self::new(
            add_member_policy,
            remove_member_policy,
            update_metadata_policy,
        ))
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, PolicyError> {
        let proto = self.to_proto()?;
        let mut buf = Vec::new();
        proto.encode(&mut buf)?;
        Ok(buf)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, PolicyError> {
        let proto = PolicySetProto::decode(bytes)?;
        Self::from_proto(proto)
    }
}

fn default_add_installation_policy() -> MembershipPolicies {
    MembershipPolicies::allow()
}

fn default_remove_installation_policy() -> MembershipPolicies {
    MembershipPolicies::deny()
}

/// A policy where any member can add or remove any other member
pub(crate) fn policy_everyone_is_admin() -> PolicySet {
    let mut metadata_policies_map: HashMap<String, MetadataPolicies> = HashMap::new();
    for field in GroupMutableMetadata::supported_fields() {
        metadata_policies_map.insert(field.to_string(), MetadataPolicies::allow());
    }
    PolicySet::new(
        MembershipPolicies::allow(),
        MembershipPolicies::allow(),
        metadata_policies_map,
    )
}

/// A policy where only the group creator can add or remove members
pub(crate) fn policy_group_creator_is_admin() -> PolicySet {
    let mut metadata_policies_map: HashMap<String, MetadataPolicies> = HashMap::new();
    for field in GroupMutableMetadata::supported_fields() {
        metadata_policies_map.insert(
            field.to_string(),
            MetadataPolicies::allow_if_actor_creator(),
        );
    }
    PolicySet::new(
        MembershipPolicies::allow_if_actor_creator(),
        MembershipPolicies::allow_if_actor_creator(),
        metadata_policies_map,
    )
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum PreconfiguredPolicies {
    #[default]
    EveryoneIsAdmin,
    GroupCreatorIsAdmin,
}

impl PreconfiguredPolicies {
    pub fn to_policy_set(&self) -> PolicySet {
        match self {
            PreconfiguredPolicies::EveryoneIsAdmin => policy_everyone_is_admin(),
            PreconfiguredPolicies::GroupCreatorIsAdmin => policy_group_creator_is_admin(),
        }
    }

    pub fn from_policy_set(policy_set: &PolicySet) -> Result<Self, PolicyError> {
        if policy_set.eq(&policy_everyone_is_admin()) {
            Ok(PreconfiguredPolicies::EveryoneIsAdmin)
        } else if policy_set.eq(&policy_group_creator_is_admin()) {
            Ok(PreconfiguredPolicies::GroupCreatorIsAdmin)
        } else {
            Err(PolicyError::InvalidPolicy)
        }
    }
}

impl std::fmt::Display for PreconfiguredPolicies {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[cfg(test)]
mod tests {
    use crate::utils::test::{rand_account_address, rand_vec};

    use super::*;

    fn build_change(
        account_address: Option<String>,
        installation_id: Option<Vec<u8>>,
        is_creator: bool,
    ) -> AggregatedMembershipChange {
        AggregatedMembershipChange {
            account_address: account_address.unwrap_or_else(rand_account_address),
            installation_ids: vec![installation_id.unwrap_or_else(rand_vec)],
            is_creator,
        }
    }

    fn build_actor(
        account_address: Option<String>,
        installation_id: Option<Vec<u8>>,
        is_creator: bool,
    ) -> CommitParticipant {
        CommitParticipant {
            account_address: account_address.unwrap_or_else(rand_account_address),
            installation_id: installation_id.unwrap_or_else(rand_vec),
            is_creator,
        }
    }

    fn build_validated_commit(
        // Add a member with the same account address as the actor if true, random account address if false
        member_added: Option<bool>,
        member_removed: Option<bool>,
        installation_added: Option<bool>,
        installation_removed: Option<bool>,
        actor_is_creator: bool,
    ) -> ValidatedCommit {
        let actor = build_actor(None, None, actor_is_creator);
        let build_membership_change = |same_address_as_actor| {
            if same_address_as_actor {
                vec![build_change(
                    Some(actor.account_address.clone()),
                    None,
                    actor_is_creator,
                )]
            } else {
                vec![build_change(None, None, false)]
            }
        };

        ValidatedCommit {
            actor: actor.clone(),
            members_added: member_added
                .map(build_membership_change)
                .unwrap_or_default(),
            members_removed: member_removed
                .map(build_membership_change)
                .unwrap_or_default(),
            installations_added: installation_added
                .map(build_membership_change)
                .unwrap_or_default(),
            installations_removed: installation_removed
                .map(build_membership_change)
                .unwrap_or_default(),
            group_name_updated: MetadataChange::default(),
        }
    }

    // TODO CVOELL: add metadata specific test here

    #[test]
    fn test_allow_all() {
        let permissions = PolicySet::new(
            MembershipPolicies::allow(),
            MembershipPolicies::allow(),
            MetadataPolicies::default_map(MetadataPolicies::allow()),
        );

        let commit = build_validated_commit(Some(true), Some(true), None, None, false);
        assert!(permissions.evaluate_commit(&commit));
    }

    #[test]
    fn test_deny() {
        let permissions = PolicySet::new(
            MembershipPolicies::deny(),
            MembershipPolicies::deny(),
            MetadataPolicies::default_map(MetadataPolicies::deny()),
        );

        let member_added_commit = build_validated_commit(Some(false), None, None, None, false);
        assert!(!permissions.evaluate_commit(&member_added_commit));

        let member_removed_commit = build_validated_commit(None, Some(false), None, None, false);
        assert!(!permissions.evaluate_commit(&member_removed_commit));

        let installation_added_commit =
            build_validated_commit(None, None, Some(false), None, false);
        // Installation added is always allowed
        assert!(permissions.evaluate_commit(&installation_added_commit));

        // Installation removed is always denied
        let installation_removed_commit =
            build_validated_commit(None, None, None, Some(false), false);
        assert!(!permissions.evaluate_commit(&installation_removed_commit));
    }

    #[test]
    fn test_actor_is_creator() {
        let permissions = PolicySet::new(
            MembershipPolicies::allow_if_actor_creator(),
            MembershipPolicies::allow_if_actor_creator(),
            MetadataPolicies::default_map(MetadataPolicies::deny()),
        );

        let commit_with_creator = build_validated_commit(Some(true), Some(true), None, None, true);
        assert!(permissions.evaluate_commit(&commit_with_creator));

        let commit_without_creator =
            build_validated_commit(Some(true), Some(true), None, None, false);
        assert!(!permissions.evaluate_commit(&commit_without_creator));
    }

    #[test]
    fn test_allow_same_member() {
        let permissions = PolicySet::new(
            MembershipPolicies::allow_same_member(),
            MembershipPolicies::deny(),
            MetadataPolicies::default_map(MetadataPolicies::deny()),
        );

        let commit_with_same_member = build_validated_commit(Some(true), None, None, None, false);
        assert!(permissions.evaluate_commit(&commit_with_same_member));

        let commit_with_different_member =
            build_validated_commit(Some(false), None, None, None, false);
        assert!(!permissions.evaluate_commit(&commit_with_different_member));
    }

    #[test]
    fn test_and_condition() {
        let permissions = PolicySet::new(
            MembershipPolicies::and(vec![
                MembershipPolicies::Standard(BasePolicies::Deny),
                MembershipPolicies::Standard(BasePolicies::Allow),
            ]),
            MembershipPolicies::allow(),
            MetadataPolicies::default_map(MetadataPolicies::deny()),
        );

        let member_added_commit = build_validated_commit(Some(true), None, None, None, false);
        assert!(!permissions.evaluate_commit(&member_added_commit));
    }

    #[test]
    fn test_any_condition() {
        let permissions = PolicySet::new(
            MembershipPolicies::any(vec![
                MembershipPolicies::deny(),
                MembershipPolicies::allow(),
            ]),
            MembershipPolicies::allow(),
            MetadataPolicies::default_map(MetadataPolicies::deny()),
        );

        let member_added_commit = build_validated_commit(Some(true), None, None, None, false);
        assert!(permissions.evaluate_commit(&member_added_commit));
    }

    #[test]
    fn test_serialize() {
        let permissions = PolicySet::new(
            MembershipPolicies::any(vec![
                MembershipPolicies::allow(),
                MembershipPolicies::deny(),
            ]),
            MembershipPolicies::and(vec![
                MembershipPolicies::allow_if_actor_creator(),
                MembershipPolicies::deny(),
            ]),
            MetadataPolicies::default_map(MetadataPolicies::deny()),
        );

        let proto = permissions.to_proto().unwrap();
        assert!(proto.add_member_policy.is_some());
        assert!(proto.remove_member_policy.is_some());

        let as_bytes = permissions.to_bytes().expect("serialization failed");
        let restored = PolicySet::from_bytes(as_bytes.as_slice()).expect("proto conversion failed");
        // All fields implement PartialEq so this should test equality all the way down
        assert!(permissions.eq(&restored))
    }

    #[test]
    fn test_disallow_serialize_allow_same_member() {
        let permissions = PolicySet::new(
            MembershipPolicies::allow_same_member(),
            MembershipPolicies::deny(),
            MetadataPolicies::default_map(MetadataPolicies::deny()),
        );

        let proto_result = permissions.to_proto();
        assert!(proto_result.is_err());
    }
}
