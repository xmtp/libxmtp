use std::collections::HashMap;

use openmls::{
    extensions::{Extension, Extensions, UnknownExtension},
    group::MlsGroup as OpenMlsGroup,
};
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
    permissions_update_policy::{
        AndCondition as PermissionsAndConditionProto, AnyCondition as PermissionsAnyConditionProto,
        Kind as PermissionsPolicyKindProto, PermissionsBasePolicy as PermissionsBasePolicyProto,
    },
    GroupMutablePermissionsV1 as GroupMutablePermissionsProto,
    MembershipPolicy as MembershipPolicyProto, MetadataPolicy as MetadataPolicyProto,
    PermissionsUpdatePolicy as PermissionsPolicyProto, PolicySet as PolicySetProto,
};

use crate::configuration::{GROUP_PERMISSIONS_EXTENSION_ID, SUPER_ADMIN_METADATA_PREFIX};

use super::{
    group_mutable_metadata::GroupMutableMetadata,
    validated_commit::{CommitParticipant, Inbox, MetadataFieldChange, ValidatedCommit},
};

#[derive(Debug, Error)]
pub enum GroupMutablePermissionsError {
    #[error("serialization: {0}")]
    Serialization(#[from] prost::EncodeError),
    #[error("deserialization: {0}")]
    Deserialization(#[from] prost::DecodeError),
    #[error("policy error {0}")]
    Policy(#[from] PolicyError),
    #[error("invalid conversation type")]
    InvalidConversationType,
    #[error("missing policies")]
    MissingPolicies,
    #[error("missing extension")]
    MissingExtension,
    #[error("invalid permission policy option")]
    InvalidPermissionPolicyOption,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GroupMutablePermissions {
    pub policies: PolicySet,
}

impl GroupMutablePermissions {
    pub fn new(policies: PolicySet) -> Self {
        Self { policies }
    }

    pub fn preconfigured_policy(
        &self,
    ) -> Result<PreconfiguredPolicies, GroupMutablePermissionsError> {
        Ok(PreconfiguredPolicies::from_policy_set(&self.policies)?)
    }

    pub(crate) fn from_proto(
        proto: GroupMutablePermissionsProto,
    ) -> Result<Self, GroupMutablePermissionsError> {
        if proto.policies.is_none() {
            return Err(GroupMutablePermissionsError::MissingPolicies);
        }
        let policies = proto.policies.expect("checked for none");

        Ok(Self::new(PolicySet::from_proto(policies)?))
    }

    pub(crate) fn to_proto(
        &self,
    ) -> Result<GroupMutablePermissionsProto, GroupMutablePermissionsError> {
        Ok(GroupMutablePermissionsProto {
            policies: Some(self.policies.to_proto()?),
        })
    }
}

impl TryFrom<GroupMutablePermissions> for Vec<u8> {
    type Error = GroupMutablePermissionsError;

    fn try_from(value: GroupMutablePermissions) -> Result<Self, Self::Error> {
        let mut buf = Vec::new();
        let proto_val = value.to_proto()?;
        proto_val.encode(&mut buf)?;

        Ok(buf)
    }
}

impl TryFrom<&Vec<u8>> for GroupMutablePermissions {
    type Error = GroupMutablePermissionsError;

    fn try_from(value: &Vec<u8>) -> Result<Self, Self::Error> {
        let proto_val = GroupMutablePermissionsProto::decode(value.as_slice())?;
        Self::from_proto(proto_val)
    }
}

impl TryFrom<GroupMutablePermissionsProto> for GroupMutablePermissions {
    type Error = GroupMutablePermissionsError;

    fn try_from(value: GroupMutablePermissionsProto) -> Result<Self, Self::Error> {
        Self::from_proto(value)
    }
}

impl TryFrom<&Extensions> for GroupMutablePermissions {
    type Error = GroupMutablePermissionsError;

    fn try_from(value: &Extensions) -> Result<Self, Self::Error> {
        for extension in value.iter() {
            if let Extension::Unknown(GROUP_PERMISSIONS_EXTENSION_ID, UnknownExtension(metadata)) =
                extension
            {
                return GroupMutablePermissions::try_from(metadata);
            }
        }
        Err(GroupMutablePermissionsError::MissingExtension)
    }
}

impl TryFrom<&OpenMlsGroup> for GroupMutablePermissions {
    type Error = GroupMutablePermissionsError;

    fn try_from(value: &OpenMlsGroup) -> Result<Self, Self::Error> {
        let extensions = value.export_group_context().extensions();
        extensions.try_into()
    }
}

pub fn extract_group_permissions(
    group: &OpenMlsGroup,
) -> Result<GroupMutablePermissions, GroupMutablePermissionsError> {
    let extensions = group.export_group_context().extensions();
    extensions.try_into()
}

// A trait for policies that can update Metadata for the group
pub trait MetadataPolicy: std::fmt::Debug {
    // Verify relevant metadata is actually changed before evaluating against the MetadataPolicy
    // See evaluate_metadata_policy
    fn evaluate(&self, actor: &CommitParticipant, change: &MetadataFieldChange) -> bool;
    fn to_proto(&self) -> Result<MetadataPolicyProto, PolicyError>;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MetadataBasePolicies {
    Allow,
    Deny,
    AllowIfActorAdminOrSuperAdmin,
    AllowIfActorSuperAdmin,
}

impl MetadataPolicy for &MetadataBasePolicies {
    fn evaluate(&self, actor: &CommitParticipant, _change: &MetadataFieldChange) -> bool {
        match self {
            MetadataBasePolicies::Allow => true,
            MetadataBasePolicies::Deny => false,
            MetadataBasePolicies::AllowIfActorAdminOrSuperAdmin => {
                actor.is_admin || actor.is_super_admin
            }
            MetadataBasePolicies::AllowIfActorSuperAdmin => actor.is_super_admin,
        }
    }

    fn to_proto(&self) -> Result<MetadataPolicyProto, PolicyError> {
        let inner = match self {
            MetadataBasePolicies::Allow => MetadataBasePolicyProto::Allow as i32,
            MetadataBasePolicies::Deny => MetadataBasePolicyProto::Deny as i32,
            MetadataBasePolicies::AllowIfActorAdminOrSuperAdmin => {
                MetadataBasePolicyProto::AllowIfAdmin as i32
            }
            MetadataBasePolicies::AllowIfActorSuperAdmin => {
                MetadataBasePolicyProto::AllowIfSuperAdmin as i32
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

    pub fn allow_if_actor_admin() -> Self {
        MetadataPolicies::Standard(MetadataBasePolicies::AllowIfActorAdminOrSuperAdmin)
    }

    pub fn allow_if_actor_super_admin() -> Self {
        MetadataPolicies::Standard(MetadataBasePolicies::AllowIfActorSuperAdmin)
    }

    pub fn and(policies: Vec<MetadataPolicies>) -> Self {
        MetadataPolicies::AndCondition(MetadataAndCondition::new(policies))
    }

    pub fn any(policies: Vec<MetadataPolicies>) -> Self {
        MetadataPolicies::AnyCondition(MetadataAnyCondition::new(policies))
    }
}

impl TryFrom<MetadataPolicyProto> for MetadataPolicies {
    type Error = PolicyError;

    fn try_from(proto: MetadataPolicyProto) -> Result<Self, Self::Error> {
        match proto.kind {
            Some(MetadataPolicyKindProto::Base(inner)) => match inner {
                1 => Ok(MetadataPolicies::allow()),
                2 => Ok(MetadataPolicies::deny()),
                3 => Ok(MetadataPolicies::allow_if_actor_admin()),
                4 => Ok(MetadataPolicies::allow_if_actor_super_admin()),
                _ => Err(PolicyError::InvalidMetadataPolicy),
            },
            Some(MetadataPolicyKindProto::AndCondition(inner)) => {
                if inner.policies.is_empty() {
                    return Err(PolicyError::InvalidMetadataPolicy);
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
                    return Err(PolicyError::InvalidMetadataPolicy);
                }

                let policies = inner
                    .policies
                    .into_iter()
                    .map(|policy| policy.try_into())
                    .collect::<Result<Vec<MetadataPolicies>, PolicyError>>()?;

                Ok(MetadataPolicies::any(policies))
            }
            None => Err(PolicyError::InvalidMetadataPolicy),
        }
    }
}

impl MetadataPolicy for MetadataPolicies {
    fn evaluate(&self, actor: &CommitParticipant, change: &MetadataFieldChange) -> bool {
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
    fn evaluate(&self, actor: &CommitParticipant, change: &MetadataFieldChange) -> bool {
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
    fn evaluate(&self, actor: &CommitParticipant, change: &MetadataFieldChange) -> bool {
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

// A trait for policies that can update Permissions for the group
pub trait PermissionsPolicy: std::fmt::Debug {
    // Verify relevant metadata is actually changed before evaluating against the MetadataPolicy
    // See evaluate_metadata_policy
    fn evaluate(&self, actor: &CommitParticipant) -> bool;
    fn to_proto(&self) -> Result<PermissionsPolicyProto, PolicyError>;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PermissionsBasePolicies {
    Deny,
    AllowIfActorAdminOrSuperAdmin,
    AllowIfActorSuperAdmin,
}

impl PermissionsPolicy for &PermissionsBasePolicies {
    fn evaluate(&self, actor: &CommitParticipant) -> bool {
        match self {
            PermissionsBasePolicies::Deny => false,
            PermissionsBasePolicies::AllowIfActorAdminOrSuperAdmin => {
                actor.is_admin || actor.is_super_admin
            }
            PermissionsBasePolicies::AllowIfActorSuperAdmin => actor.is_super_admin,
        }
    }

    fn to_proto(&self) -> Result<PermissionsPolicyProto, PolicyError> {
        let inner = match self {
            PermissionsBasePolicies::Deny => PermissionsBasePolicyProto::Deny as i32,
            PermissionsBasePolicies::AllowIfActorAdminOrSuperAdmin => {
                PermissionsBasePolicyProto::AllowIfAdmin as i32
            }
            PermissionsBasePolicies::AllowIfActorSuperAdmin => {
                PermissionsBasePolicyProto::AllowIfSuperAdmin as i32
            }
        };

        Ok(PermissionsPolicyProto {
            kind: Some(PermissionsPolicyKindProto::Base(inner)),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum PermissionsPolicies {
    Standard(PermissionsBasePolicies),
    AndCondition(PermissionsAndCondition),
    AnyCondition(PermissionsAnyCondition),
}

impl PermissionsPolicies {
    pub fn deny() -> Self {
        PermissionsPolicies::Standard(PermissionsBasePolicies::Deny)
    }

    #[allow(dead_code)]
    pub fn allow_if_actor_admin() -> Self {
        PermissionsPolicies::Standard(PermissionsBasePolicies::AllowIfActorAdminOrSuperAdmin)
    }

    #[allow(dead_code)]
    pub fn allow_if_actor_super_admin() -> Self {
        PermissionsPolicies::Standard(PermissionsBasePolicies::AllowIfActorSuperAdmin)
    }

    pub fn and(policies: Vec<PermissionsPolicies>) -> Self {
        PermissionsPolicies::AndCondition(PermissionsAndCondition::new(policies))
    }

    pub fn any(policies: Vec<PermissionsPolicies>) -> Self {
        PermissionsPolicies::AnyCondition(PermissionsAnyCondition::new(policies))
    }
}

impl TryFrom<PermissionsPolicyProto> for PermissionsPolicies {
    type Error = PolicyError;

    fn try_from(proto: PermissionsPolicyProto) -> Result<Self, Self::Error> {
        match proto.kind {
            Some(PermissionsPolicyKindProto::Base(inner)) => match inner {
                1 => Ok(PermissionsPolicies::deny()),
                2 => Ok(PermissionsPolicies::allow_if_actor_admin()),
                3 => Ok(PermissionsPolicies::allow_if_actor_super_admin()),
                _ => Err(PolicyError::InvalidPermissionsPolicy),
            },
            Some(PermissionsPolicyKindProto::AndCondition(inner)) => {
                if inner.policies.is_empty() {
                    return Err(PolicyError::InvalidPermissionsPolicy);
                }
                let policies = inner
                    .policies
                    .into_iter()
                    .map(|policy| policy.try_into())
                    .collect::<Result<Vec<PermissionsPolicies>, PolicyError>>()?;

                Ok(PermissionsPolicies::and(policies))
            }
            Some(PermissionsPolicyKindProto::AnyCondition(inner)) => {
                if inner.policies.is_empty() {
                    return Err(PolicyError::InvalidPermissionsPolicy);
                }

                let policies = inner
                    .policies
                    .into_iter()
                    .map(|policy| policy.try_into())
                    .collect::<Result<Vec<PermissionsPolicies>, PolicyError>>()?;

                Ok(PermissionsPolicies::any(policies))
            }
            None => Err(PolicyError::InvalidPermissionsPolicy),
        }
    }
}

impl PermissionsPolicy for PermissionsPolicies {
    fn evaluate(&self, actor: &CommitParticipant) -> bool {
        match self {
            PermissionsPolicies::Standard(policy) => policy.evaluate(actor),
            PermissionsPolicies::AndCondition(policy) => policy.evaluate(actor),
            PermissionsPolicies::AnyCondition(policy) => policy.evaluate(actor),
        }
    }

    fn to_proto(&self) -> Result<PermissionsPolicyProto, PolicyError> {
        Ok(match self {
            PermissionsPolicies::Standard(policy) => policy.to_proto()?,
            PermissionsPolicies::AndCondition(policy) => policy.to_proto()?,
            PermissionsPolicies::AnyCondition(policy) => policy.to_proto()?,
        })
    }
}

// An AndCondition evaluates to true if all the policies it contains evaluate to true
#[derive(Clone, Debug, PartialEq)]
pub struct PermissionsAndCondition {
    policies: Vec<PermissionsPolicies>,
}

impl PermissionsAndCondition {
    pub(super) fn new(policies: Vec<PermissionsPolicies>) -> Self {
        Self { policies }
    }
}

impl PermissionsPolicy for PermissionsAndCondition {
    fn evaluate(&self, actor: &CommitParticipant) -> bool {
        self.policies.iter().all(|policy| policy.evaluate(actor))
    }

    fn to_proto(&self) -> Result<PermissionsPolicyProto, PolicyError> {
        Ok(PermissionsPolicyProto {
            kind: Some(PermissionsPolicyKindProto::AndCondition(
                PermissionsAndConditionProto {
                    policies: self
                        .policies
                        .iter()
                        .map(|policy| policy.to_proto())
                        .collect::<Result<Vec<PermissionsPolicyProto>, PolicyError>>()?,
                },
            )),
        })
    }
}

// An AnyCondition evaluates to true if any of the contained policies evaluate to true
#[derive(Clone, Debug, PartialEq)]
pub struct PermissionsAnyCondition {
    policies: Vec<PermissionsPolicies>,
}

#[allow(dead_code)]
impl PermissionsAnyCondition {
    pub(super) fn new(policies: Vec<PermissionsPolicies>) -> Self {
        Self { policies }
    }
}

impl PermissionsPolicy for PermissionsAnyCondition {
    fn evaluate(&self, actor: &CommitParticipant) -> bool {
        self.policies.iter().any(|policy| policy.evaluate(actor))
    }

    fn to_proto(&self) -> Result<PermissionsPolicyProto, PolicyError> {
        Ok(PermissionsPolicyProto {
            kind: Some(PermissionsPolicyKindProto::AnyCondition(
                PermissionsAnyConditionProto {
                    policies: self
                        .policies
                        .iter()
                        .map(|policy| policy.to_proto())
                        .collect::<Result<Vec<PermissionsPolicyProto>, PolicyError>>()?,
                },
            )),
        })
    }
}

// A trait for policies that can add/remove members and installations for the group
pub trait MembershipPolicy: std::fmt::Debug {
    fn evaluate(&self, actor: &CommitParticipant, change: &Inbox) -> bool;
    fn to_proto(&self) -> Result<MembershipPolicyProto, PolicyError>;
}

#[derive(Debug, Error)]
pub enum PolicyError {
    #[error("serialization {0}")]
    Serialization(#[from] prost::EncodeError),
    #[error("deserialization {0}")]
    Deserialization(#[from] prost::DecodeError),
    #[error("Missing metadata policy field: {name}")]
    MissingMetadataPolicyField { name: String },
    #[error("invalid policy")]
    InvalidPolicy,
    #[error("unexpected preset policy")]
    InvalidPresetPolicy,
    #[error("invalid metadata policy")]
    InvalidMetadataPolicy,
    #[error("invalid membership policy")]
    InvalidMembershipPolicy,
    #[error("invalid permissions policy")]
    InvalidPermissionsPolicy,
    #[error("from proto add member invalid policy")]
    FromProtoAddMemberInvalidPolicy,
    #[error("from proto remove member invalid policy")]
    FromProtoRemoveMemberInvalidPolicy,
    #[error("from proto add admin invalid policy")]
    FromProtoAddAdminInvalidPolicy,
    #[error("from proto remove admin invalid policy")]
    FromProtoRemoveAdminInvalidPolicy,
    #[error("from proto update permissions invalid policy")]
    FromProtoUpdatePermissionsInvalidPolicy,
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(dead_code)]
#[repr(u8)]
pub enum BasePolicies {
    Allow,
    Deny,
    // Allow if the change only applies to subject installations with the same account address as the actor
    AllowSameMember,
    AllowIfAdminOrSuperAdmin,
    AllowIfSuperAdmin,
}

impl MembershipPolicy for BasePolicies {
    fn evaluate(&self, actor: &CommitParticipant, inbox: &Inbox) -> bool {
        match self {
            BasePolicies::Allow => true,
            BasePolicies::Deny => false,
            BasePolicies::AllowSameMember => inbox.inbox_id == actor.inbox_id,
            BasePolicies::AllowIfAdminOrSuperAdmin => actor.is_admin || actor.is_super_admin,
            BasePolicies::AllowIfSuperAdmin => actor.is_super_admin,
        }
    }

    fn to_proto(&self) -> Result<MembershipPolicyProto, PolicyError> {
        let inner = match self {
            BasePolicies::Allow => BasePolicyProto::Allow as i32,
            BasePolicies::Deny => BasePolicyProto::Deny as i32,
            BasePolicies::AllowSameMember => return Err(PolicyError::InvalidPolicy), // AllowSameMember is not needed on any of the wire format protos
            BasePolicies::AllowIfAdminOrSuperAdmin => {
                BasePolicyProto::AllowIfAdminOrSuperAdmin as i32
            }
            BasePolicies::AllowIfSuperAdmin => BasePolicyProto::AllowIfSuperAdmin as i32,
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
    pub fn allow_if_actor_admin() -> Self {
        MembershipPolicies::Standard(BasePolicies::AllowIfAdminOrSuperAdmin)
    }

    #[allow(dead_code)]
    pub fn allow_if_actor_super_admin() -> Self {
        MembershipPolicies::Standard(BasePolicies::AllowIfSuperAdmin)
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
                3 => Ok(MembershipPolicies::allow_if_actor_admin()),
                4 => Ok(MembershipPolicies::allow_if_actor_super_admin()),
                _ => Err(PolicyError::InvalidMembershipPolicy),
            },
            Some(PolicyKindProto::AndCondition(inner)) => {
                if inner.policies.is_empty() {
                    return Err(PolicyError::InvalidMembershipPolicy);
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
                    return Err(PolicyError::InvalidMembershipPolicy);
                }

                let policies = inner
                    .policies
                    .into_iter()
                    .map(|policy| policy.try_into())
                    .collect::<Result<Vec<MembershipPolicies>, PolicyError>>()?;

                Ok(MembershipPolicies::any(policies))
            }
            None => Err(PolicyError::InvalidMembershipPolicy),
        }
    }
}

impl MembershipPolicy for MembershipPolicies {
    fn evaluate(&self, actor: &CommitParticipant, inbox: &Inbox) -> bool {
        match self {
            MembershipPolicies::Standard(policy) => policy.evaluate(actor, inbox),
            MembershipPolicies::AndCondition(policy) => policy.evaluate(actor, inbox),
            MembershipPolicies::AnyCondition(policy) => policy.evaluate(actor, inbox),
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
    fn evaluate(&self, actor: &CommitParticipant, inbox: &Inbox) -> bool {
        self.policies
            .iter()
            .all(|policy| policy.evaluate(actor, inbox))
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
    fn evaluate(&self, actor: &CommitParticipant, inbox: &Inbox) -> bool {
        self.policies
            .iter()
            .any(|policy| policy.evaluate(actor, inbox))
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
    pub update_metadata_policy: HashMap<String, MetadataPolicies>,
    pub add_admin_policy: PermissionsPolicies,
    pub remove_admin_policy: PermissionsPolicies,
    pub update_permissions_policy: PermissionsPolicies,
}

impl PolicySet {
    pub fn new(
        add_member_policy: MembershipPolicies,
        remove_member_policy: MembershipPolicies,
        update_metadata_policy: HashMap<String, MetadataPolicies>,
        add_admin_policy: PermissionsPolicies,
        remove_admin_policy: PermissionsPolicies,
        update_permissions_policy: PermissionsPolicies,
    ) -> Self {
        Self {
            add_member_policy,
            remove_member_policy,
            update_metadata_policy,
            add_admin_policy,
            remove_admin_policy,
            update_permissions_policy,
        }
    }

    pub fn evaluate_commit(&self, commit: &ValidatedCommit) -> bool {
        // Verify add member policy was not violated
        let added_inboxes_valid = self.evaluate_policy(
            commit.added_inboxes.iter(),
            &self.add_member_policy,
            &commit.actor,
        );

        // Verify remove member policy was not violated
        // Super admin can not be removed from a group
        let removed_inboxes_valid = self.evaluate_policy(
            commit.removed_inboxes.iter(),
            &self.remove_member_policy,
            &commit.actor,
        ) && !commit
            .removed_inboxes
            .iter()
            .any(|inbox| inbox.is_super_admin);

        // Verify that update metadata policy was not violated
        let metadata_changes_valid = self.evaluate_metadata_policy(
            commit.metadata_changes.metadata_field_changes.iter(),
            &self.update_metadata_policy,
            &commit.actor,
        );

        // Verify that add admin policy was not violated
        let added_admins_valid = commit.metadata_changes.admins_added.is_empty()
            || self.add_admin_policy.evaluate(&commit.actor);

        // Verify that remove admin policy was not violated
        let removed_admins_valid = commit.metadata_changes.admins_removed.is_empty()
            || self.remove_admin_policy.evaluate(&commit.actor);

        // Verify that super admin add policy was not violated
        let super_admin_add_valid =
            commit.metadata_changes.super_admins_added.is_empty() || commit.actor.is_super_admin;

        // Verify that super admin remove policy was not violated
        // You can never remove the last super admin
        let super_admin_remove_valid = commit.metadata_changes.super_admins_removed.is_empty()
            || (commit.actor.is_super_admin && commit.metadata_changes.num_super_admins > 0);

        // Permissions can only be changed by the super admin
        let permissions_changes_valid = !commit.permissions_changed || commit.actor.is_super_admin;

        added_inboxes_valid
            && removed_inboxes_valid
            && metadata_changes_valid
            && added_admins_valid
            && removed_admins_valid
            && super_admin_add_valid
            && super_admin_remove_valid
            && permissions_changes_valid
    }

    fn evaluate_policy<'a, I, P>(
        &self,
        mut changes: I,
        policy: &P,
        actor: &CommitParticipant,
    ) -> bool
    where
        I: Iterator<Item = &'a Inbox>,
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

    fn evaluate_metadata_policy<'a, I>(
        &self,
        mut changes: I,
        policies: &HashMap<String, MetadataPolicies>,
        actor: &CommitParticipant,
    ) -> bool
    where
        I: Iterator<Item = &'a MetadataFieldChange>,
    {
        changes.all(|change| {
            if let Some(policy) = policies.get(&change.field_name) {
                if !policy.evaluate(actor, change) {
                    log::info!(
                        "Policy for field {} failed for actor {:?} and change {:?}",
                        change.field_name,
                        actor,
                        change
                    );
                    return false;
                }
                return true;
            }
            // Policy is not found for metadata change, let's check if the new field contains the super_admin prefix
            // and evaluate accordingly
            let policy_for_unrecognized_field =
                if change.field_name.starts_with(SUPER_ADMIN_METADATA_PREFIX) {
                    MetadataPolicies::allow_if_actor_super_admin()
                } else {
                    // Otherwise we default to admin only for fields with missing policies
                    MetadataPolicies::allow_if_actor_admin()
                };
            if !policy_for_unrecognized_field.evaluate(actor, change) {
                log::info!(
                    "Metadata field update with unknown policy was denied: {}",
                    change.field_name
                );
                return false;
            }
            true
        })
    }

    pub(crate) fn to_proto(&self) -> Result<PolicySetProto, PolicyError> {
        let add_member_policy = Some(self.add_member_policy.to_proto()?);
        let remove_member_policy = Some(self.remove_member_policy.to_proto()?);

        let mut update_metadata_policy = HashMap::new();
        for (key, policy) in &self.update_metadata_policy {
            let policy_proto = policy.to_proto()?;
            update_metadata_policy.insert(key.clone(), policy_proto);
        }
        let add_admin_policy = Some(self.add_admin_policy.to_proto()?);
        let remove_admin_policy = Some(self.remove_admin_policy.to_proto()?);
        let update_permissions_policy = Some(self.update_permissions_policy.to_proto()?);
        Ok(PolicySetProto {
            add_member_policy,
            remove_member_policy,
            update_metadata_policy,
            add_admin_policy,
            remove_admin_policy,
            update_permissions_policy,
        })
    }

    pub(crate) fn from_proto(proto: PolicySetProto) -> Result<Self, PolicyError> {
        let add_member_policy = MembershipPolicies::try_from(
            proto
                .add_member_policy
                .ok_or(PolicyError::FromProtoAddMemberInvalidPolicy)?,
        )?;
        let remove_member_policy = MembershipPolicies::try_from(
            proto
                .remove_member_policy
                .ok_or(PolicyError::FromProtoRemoveMemberInvalidPolicy)?,
        )?;
        let add_admin_policy = PermissionsPolicies::try_from(
            proto
                .add_admin_policy
                .ok_or(PolicyError::FromProtoAddAdminInvalidPolicy)?,
        )?;
        let remove_admin_policy = PermissionsPolicies::try_from(
            proto
                .remove_admin_policy
                .ok_or(PolicyError::FromProtoRemoveAdminInvalidPolicy)?,
        )?;
        let update_permissions_policy = PermissionsPolicies::try_from(
            proto
                .update_permissions_policy
                .ok_or(PolicyError::FromProtoUpdatePermissionsInvalidPolicy)?,
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
            add_admin_policy,
            remove_admin_policy,
            update_permissions_policy,
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

// Depending on if the client is on a newer or older version of libxmtp
// since the group was created, the number of metadata policies might not match
// the default All Members Policy Set. As long as all metadata policies are allow, we will
// match against All Members Preconfigured Policy
pub fn is_policy_all_members(policy: &PolicySet) -> Result<bool, PolicyError> {
    let mut metadata_policies_equal = true;
    for field_name in policy.update_metadata_policy.keys() {
        let metadata_policy = policy.update_metadata_policy.get(field_name).ok_or(
            PolicyError::MissingMetadataPolicyField {
                name: field_name.to_string(),
            },
        )?;
        metadata_policies_equal =
            metadata_policies_equal && metadata_policy.eq(&MetadataPolicies::allow());
    }
    Ok(metadata_policies_equal
        && policy.add_member_policy == MembershipPolicies::allow()
        && policy.remove_member_policy == MembershipPolicies::allow_if_actor_admin()
        && policy.add_admin_policy == PermissionsPolicies::allow_if_actor_super_admin()
        && policy.remove_admin_policy == PermissionsPolicies::allow_if_actor_super_admin()
        && policy.update_permissions_policy == PermissionsPolicies::allow_if_actor_super_admin())
}

// Depending on if the client is on a newer or older version of libxmtp
// since the group was created, the number of metadata policies might not match
// the default Admin Only Policy Set. As long as all metadata policies are admin only, we will
// match against Admin Only Preconfigured Policy
pub fn is_policy_admin_only(policy: &PolicySet) -> Result<bool, PolicyError> {
    let mut metadata_policies_equal = true;
    for field_name in policy.update_metadata_policy.keys() {
        let metadata_policy = policy.update_metadata_policy.get(field_name).ok_or(
            PolicyError::MissingMetadataPolicyField {
                name: field_name.to_string(),
            },
        )?;
        metadata_policies_equal = metadata_policies_equal
            && metadata_policy.eq(&MetadataPolicies::allow_if_actor_admin());
    }
    Ok(metadata_policies_equal
        && policy.add_member_policy == MembershipPolicies::allow_if_actor_admin()
        && policy.remove_member_policy == MembershipPolicies::allow_if_actor_admin()
        && policy.add_admin_policy == PermissionsPolicies::allow_if_actor_super_admin()
        && policy.remove_admin_policy == PermissionsPolicies::allow_if_actor_super_admin()
        && policy.update_permissions_policy == PermissionsPolicies::allow_if_actor_super_admin())
}

/// A policy where any member can add or remove any other member
pub(crate) fn policy_all_members() -> PolicySet {
    let mut metadata_policies_map: HashMap<String, MetadataPolicies> = HashMap::new();
    for field in GroupMutableMetadata::supported_fields() {
        metadata_policies_map.insert(field.to_string(), MetadataPolicies::allow());
    }
    PolicySet::new(
        MembershipPolicies::allow(),
        MembershipPolicies::allow_if_actor_admin(),
        metadata_policies_map,
        PermissionsPolicies::allow_if_actor_super_admin(),
        PermissionsPolicies::allow_if_actor_super_admin(),
        PermissionsPolicies::allow_if_actor_super_admin(),
    )
}

/// A policy where only the admins can add or remove members
pub(crate) fn policy_admin_only() -> PolicySet {
    let mut metadata_policies_map: HashMap<String, MetadataPolicies> = HashMap::new();
    for field in GroupMutableMetadata::supported_fields() {
        metadata_policies_map.insert(field.to_string(), MetadataPolicies::allow_if_actor_admin());
    }
    PolicySet::new(
        MembershipPolicies::allow_if_actor_admin(),
        MembershipPolicies::allow_if_actor_admin(),
        metadata_policies_map,
        PermissionsPolicies::allow_if_actor_super_admin(),
        PermissionsPolicies::allow_if_actor_super_admin(),
        PermissionsPolicies::allow_if_actor_super_admin(),
    )
}
impl Default for PolicySet {
    fn default() -> Self {
        PreconfiguredPolicies::default().to_policy_set()
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum PreconfiguredPolicies {
    #[default]
    AllMembers,
    AdminsOnly,
}

impl PreconfiguredPolicies {
    pub fn to_policy_set(&self) -> PolicySet {
        match self {
            PreconfiguredPolicies::AllMembers => policy_all_members(),
            PreconfiguredPolicies::AdminsOnly => policy_admin_only(),
        }
    }

    pub fn from_policy_set(policy_set: &PolicySet) -> Result<Self, PolicyError> {
        if is_policy_all_members(policy_set)? {
            Ok(PreconfiguredPolicies::AllMembers)
        } else if is_policy_admin_only(policy_set)? {
            Ok(PreconfiguredPolicies::AdminsOnly)
        } else {
            Err(PolicyError::InvalidPresetPolicy)
        }
    }
}

impl std::fmt::Display for PreconfiguredPolicies {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use crate::{
        groups::{group_mutable_metadata::MetadataField, validated_commit::MutableMetadataChanges},
        utils::test::{rand_string, rand_vec},
    };

    use super::*;

    fn build_change(inbox_id: Option<String>, is_admin: bool, is_super_admin: bool) -> Inbox {
        Inbox {
            inbox_id: inbox_id.unwrap_or(rand_string()),
            is_creator: is_super_admin,
            is_super_admin,
            is_admin,
        }
    }

    fn build_actor(
        inbox_id: Option<String>,
        installation_id: Option<Vec<u8>>,
        is_admin: bool,
        is_super_admin: bool,
    ) -> CommitParticipant {
        CommitParticipant {
            inbox_id: inbox_id.unwrap_or(rand_string()),
            installation_id: installation_id.unwrap_or_else(rand_vec),
            is_creator: is_super_admin,
            is_admin,
            is_super_admin,
        }
    }

    fn build_validated_commit(
        // Add a member with the same account address as the actor if true, random account address if false
        member_added: Option<bool>,
        member_removed: Option<bool>,
        metadata_fields_changed: Option<Vec<String>>,
        permissions_changed: bool,
        actor_is_admin: bool,
        actor_is_super_admin: bool,
    ) -> ValidatedCommit {
        let actor = build_actor(None, None, actor_is_admin, actor_is_super_admin);
        let build_membership_change = |same_address_as_actor| {
            if same_address_as_actor {
                vec![build_change(
                    Some(actor.inbox_id.clone()),
                    actor_is_admin,
                    actor_is_super_admin,
                )]
            } else {
                vec![build_change(None, false, false)]
            }
        };

        let field_changes = metadata_fields_changed
            .unwrap_or_default()
            .into_iter()
            .map(|field| MetadataFieldChange::new(field, Some(rand_string()), Some(rand_string())))
            .collect();

        ValidatedCommit {
            actor: actor.clone(),
            added_inboxes: member_added
                .map(build_membership_change)
                .unwrap_or_default(),
            removed_inboxes: member_removed
                .map(build_membership_change)
                .unwrap_or_default(),
            metadata_changes: MutableMetadataChanges {
                metadata_field_changes: field_changes,
                ..Default::default()
            },
            permissions_changed,
        }
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_allow_all() {
        let permissions = PolicySet::new(
            MembershipPolicies::allow(),
            MembershipPolicies::allow(),
            MetadataPolicies::default_map(MetadataPolicies::allow()),
            PermissionsPolicies::allow_if_actor_super_admin(),
            PermissionsPolicies::allow_if_actor_super_admin(),
            PermissionsPolicies::allow_if_actor_super_admin(),
        );

        let commit = build_validated_commit(Some(true), Some(true), None, false, false, false);
        assert!(permissions.evaluate_commit(&commit));
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_deny() {
        let permissions = PolicySet::new(
            MembershipPolicies::deny(),
            MembershipPolicies::deny(),
            MetadataPolicies::default_map(MetadataPolicies::deny()),
            PermissionsPolicies::allow_if_actor_super_admin(),
            PermissionsPolicies::allow_if_actor_super_admin(),
            PermissionsPolicies::allow_if_actor_super_admin(),
        );

        let member_added_commit =
            build_validated_commit(Some(false), None, None, false, false, false);
        assert!(!permissions.evaluate_commit(&member_added_commit));

        let member_removed_commit =
            build_validated_commit(None, Some(false), None, false, false, false);
        assert!(!permissions.evaluate_commit(&member_removed_commit));
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_actor_is_creator() {
        let permissions = PolicySet::new(
            MembershipPolicies::allow_if_actor_super_admin(),
            MembershipPolicies::allow_if_actor_super_admin(),
            MetadataPolicies::default_map(MetadataPolicies::deny()),
            PermissionsPolicies::allow_if_actor_super_admin(),
            PermissionsPolicies::allow_if_actor_super_admin(),
            PermissionsPolicies::allow_if_actor_super_admin(),
        );

        // Can not remove the creator if they are the only super admin
        let commit_with_creator =
            build_validated_commit(Some(true), Some(true), None, false, false, true);
        assert!(!permissions.evaluate_commit(&commit_with_creator));

        let commit_with_creator =
            build_validated_commit(Some(true), Some(false), None, false, false, true);
        assert!(permissions.evaluate_commit(&commit_with_creator));

        let commit_without_creator =
            build_validated_commit(Some(true), Some(true), None, false, false, false);
        assert!(!permissions.evaluate_commit(&commit_without_creator));
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_allow_same_member() {
        let permissions = PolicySet::new(
            MembershipPolicies::allow_same_member(),
            MembershipPolicies::deny(),
            MetadataPolicies::default_map(MetadataPolicies::deny()),
            PermissionsPolicies::allow_if_actor_super_admin(),
            PermissionsPolicies::allow_if_actor_super_admin(),
            PermissionsPolicies::allow_if_actor_super_admin(),
        );

        let commit_with_same_member =
            build_validated_commit(Some(true), None, None, false, false, false);
        assert!(permissions.evaluate_commit(&commit_with_same_member));

        let commit_with_different_member =
            build_validated_commit(Some(false), None, None, false, false, false);
        assert!(!permissions.evaluate_commit(&commit_with_different_member));
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_and_condition() {
        let permissions = PolicySet::new(
            MembershipPolicies::and(vec![
                MembershipPolicies::Standard(BasePolicies::Deny),
                MembershipPolicies::Standard(BasePolicies::Allow),
            ]),
            MembershipPolicies::allow(),
            MetadataPolicies::default_map(MetadataPolicies::deny()),
            PermissionsPolicies::allow_if_actor_super_admin(),
            PermissionsPolicies::allow_if_actor_super_admin(),
            PermissionsPolicies::allow_if_actor_super_admin(),
        );

        let member_added_commit =
            build_validated_commit(Some(true), None, None, false, false, false);
        assert!(!permissions.evaluate_commit(&member_added_commit));
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_any_condition() {
        let permissions = PolicySet::new(
            MembershipPolicies::any(vec![
                MembershipPolicies::deny(),
                MembershipPolicies::allow(),
            ]),
            MembershipPolicies::allow(),
            MetadataPolicies::default_map(MetadataPolicies::deny()),
            PermissionsPolicies::allow_if_actor_super_admin(),
            PermissionsPolicies::allow_if_actor_super_admin(),
            PermissionsPolicies::allow_if_actor_super_admin(),
        );

        let member_added_commit =
            build_validated_commit(Some(true), None, None, false, false, false);
        assert!(permissions.evaluate_commit(&member_added_commit));
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_serialize() {
        let permissions = PolicySet::new(
            MembershipPolicies::any(vec![
                MembershipPolicies::allow(),
                MembershipPolicies::deny(),
            ]),
            MembershipPolicies::and(vec![
                MembershipPolicies::allow_if_actor_super_admin(),
                MembershipPolicies::deny(),
            ]),
            MetadataPolicies::default_map(MetadataPolicies::deny()),
            PermissionsPolicies::allow_if_actor_super_admin(),
            PermissionsPolicies::allow_if_actor_super_admin(),
            PermissionsPolicies::allow_if_actor_super_admin(),
        );

        let proto = permissions.to_proto().unwrap();
        assert!(proto.add_member_policy.is_some());
        assert!(proto.remove_member_policy.is_some());

        let as_bytes = permissions.to_bytes().expect("serialization failed");
        let restored = PolicySet::from_bytes(as_bytes.as_slice()).expect("proto conversion failed");
        // All fields implement PartialEq so this should test equality all the way down
        assert!(permissions.eq(&restored))
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_update_group_name() {
        let allow_permissions = PolicySet::new(
            MembershipPolicies::allow(),
            MembershipPolicies::allow(),
            MetadataPolicies::default_map(MetadataPolicies::allow()),
            PermissionsPolicies::allow_if_actor_super_admin(),
            PermissionsPolicies::allow_if_actor_super_admin(),
            PermissionsPolicies::allow_if_actor_super_admin(),
        );

        let member_added_commit = build_validated_commit(
            Some(true),
            None,
            Some(vec![MetadataField::GroupName.to_string()]),
            false,
            false,
            false,
        );

        assert!(allow_permissions.evaluate_commit(&member_added_commit));

        let deny_permissions = PolicySet::new(
            MembershipPolicies::allow(),
            MembershipPolicies::allow(),
            MetadataPolicies::default_map(MetadataPolicies::deny()),
            PermissionsPolicies::allow_if_actor_super_admin(),
            PermissionsPolicies::allow_if_actor_super_admin(),
            PermissionsPolicies::allow_if_actor_super_admin(),
        );

        assert!(!deny_permissions.evaluate_commit(&member_added_commit));
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_disallow_serialize_allow_same_member() {
        let permissions = PolicySet::new(
            MembershipPolicies::allow_same_member(),
            MembershipPolicies::deny(),
            MetadataPolicies::default_map(MetadataPolicies::deny()),
            PermissionsPolicies::allow_if_actor_super_admin(),
            PermissionsPolicies::allow_if_actor_super_admin(),
            PermissionsPolicies::allow_if_actor_super_admin(),
        );

        let proto_result = permissions.to_proto();
        assert!(proto_result.is_err());
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_preconfigured_policy() {
        let group_permissions = GroupMutablePermissions::new(policy_all_members());

        assert_eq!(
            group_permissions.preconfigured_policy().unwrap(),
            PreconfiguredPolicies::AllMembers
        );

        let group_group_permissions_creator_admin =
            GroupMutablePermissions::new(policy_admin_only());

        assert_eq!(
            group_group_permissions_creator_admin
                .preconfigured_policy()
                .unwrap(),
            PreconfiguredPolicies::AdminsOnly
        );
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_preconfigured_policy_equality_new_metadata() {
        let mut metadata_policies_map = MetadataPolicies::default_map(MetadataPolicies::allow());
        metadata_policies_map.insert("new_metadata_field".to_string(), MetadataPolicies::allow());
        let policy_set_new_metadata_permission = PolicySet {
            add_member_policy: MembershipPolicies::allow(),
            remove_member_policy: MembershipPolicies::allow_if_actor_admin(),
            update_metadata_policy: metadata_policies_map,
            add_admin_policy: PermissionsPolicies::allow_if_actor_super_admin(),
            remove_admin_policy: PermissionsPolicies::allow_if_actor_super_admin(),
            update_permissions_policy: PermissionsPolicies::allow_if_actor_super_admin(),
        };

        assert!(is_policy_all_members(&policy_set_new_metadata_permission).unwrap());

        let mut metadata_policies_map =
            MetadataPolicies::default_map(MetadataPolicies::allow_if_actor_admin());
        metadata_policies_map.insert(
            "new_metadata_field_2".to_string(),
            MetadataPolicies::allow_if_actor_admin(),
        );
        let policy_set_new_metadata_permission = PolicySet {
            add_member_policy: MembershipPolicies::allow_if_actor_admin(),
            remove_member_policy: MembershipPolicies::allow_if_actor_admin(),
            update_metadata_policy: metadata_policies_map,
            add_admin_policy: PermissionsPolicies::allow_if_actor_super_admin(),
            remove_admin_policy: PermissionsPolicies::allow_if_actor_super_admin(),
            update_permissions_policy: PermissionsPolicies::allow_if_actor_super_admin(),
        };

        assert!(is_policy_admin_only(&policy_set_new_metadata_permission).unwrap());
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_permission_update() {
        let permissions = PolicySet::new(
            MembershipPolicies::allow(),
            MembershipPolicies::allow_if_actor_admin(),
            MetadataPolicies::default_map(MetadataPolicies::allow()),
            PermissionsPolicies::allow_if_actor_super_admin(),
            PermissionsPolicies::allow_if_actor_super_admin(),
            PermissionsPolicies::allow_if_actor_super_admin(),
        );

        // Commit should fail because actor is not superadmin
        let commit = build_validated_commit(None, None, None, true, false, false);
        assert!(!permissions.evaluate_commit(&commit));

        // Commit should pass because actor is superadmin
        let commit = build_validated_commit(None, None, None, true, false, true);
        assert!(permissions.evaluate_commit(&commit));
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_evaluate_field_with_unknown_policy() {
        // Create a group whose default metadata can be updated by any member
        let permissions = PolicySet::new(
            MembershipPolicies::allow(),
            MembershipPolicies::allow(),
            MetadataPolicies::default_map(MetadataPolicies::allow()),
            PermissionsPolicies::allow_if_actor_super_admin(),
            PermissionsPolicies::allow_if_actor_super_admin(),
            PermissionsPolicies::allow_if_actor_super_admin(),
        );

        // Non admin, non super admin can update group name
        let name_updated_commit = build_validated_commit(
            None,
            None,
            Some(vec![MetadataField::GroupName.to_string()]),
            false,
            false,
            false,
        );
        assert!(permissions.evaluate_commit(&name_updated_commit));

        // Non admin, non super admin can NOT update non existing field
        let non_existing_field_updated_commit = build_validated_commit(
            None,
            None,
            Some(vec!["non_existing_field".to_string()]),
            false,
            false,
            false,
        );
        assert!(!permissions.evaluate_commit(&non_existing_field_updated_commit));

        // Admin can update non existing field
        let non_existing_field_updated_commit = build_validated_commit(
            None,
            None,
            Some(vec!["non_existing_field".to_string()]),
            false,
            true,
            false,
        );
        assert!(permissions.evaluate_commit(&non_existing_field_updated_commit));

        // Admin can NOT update non existing field that starts with super_admin only prefix
        let non_existing_field_updated_commit = build_validated_commit(
            None,
            None,
            Some(vec![
                SUPER_ADMIN_METADATA_PREFIX.to_string() + "non_existing_field",
            ]),
            false,
            true,
            false,
        );
        assert!(!permissions.evaluate_commit(&non_existing_field_updated_commit));

        // Super Admin CAN update non existing field that starts with super_admin only prefix
        let non_existing_field_updated_commit = build_validated_commit(
            None,
            None,
            Some(vec![
                SUPER_ADMIN_METADATA_PREFIX.to_string() + "non_existing_field",
            ]),
            false,
            false,
            true,
        );
        assert!(permissions.evaluate_commit(&non_existing_field_updated_commit));
    }
}
