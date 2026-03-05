use openmls::{
    extensions::{Extension, Extensions, UnknownExtension},
    group::{GroupContext, MlsGroup as OpenMlsGroup},
};
use prost::Message;
use std::collections::HashMap;
use thiserror::Error;
use xmtp_common::ErrorCode;
use xmtp_proto::xmtp::mls::message_contents::{
    GroupMutablePermissionsV1 as GroupMutablePermissionsProto,
    MembershipPolicy as MembershipPolicyProto, MetadataPolicy as MetadataPolicyProto,
    PermissionsUpdatePolicy as PermissionsPolicyProto, PolicySet as PolicySetProto,
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
};

use super::validated_commit::{CommitParticipant, Inbox, MetadataFieldChange, ValidatedCommit};
use xmtp_configuration::{GROUP_PERMISSIONS_EXTENSION_ID, SUPER_ADMIN_METADATA_PREFIX};
use xmtp_mls_common::group_mutable_metadata::{GroupMutableMetadata, MetadataField};

/// Errors that can occur when working with GroupMutablePermissions.
#[derive(Debug, Error, ErrorCode)]
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

/// Represents the mutable permissions for a group.
///
/// This struct is stored as an MLS Unknown Group Context Extension.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupMutablePermissions {
    /// The set of policies that define the permissions for the group.
    pub policies: PolicySet,
}

impl GroupMutablePermissions {
    /// Creates a new GroupMutablePermissions instance.
    pub fn new(policies: PolicySet) -> Self {
        Self { policies }
    }

    /// Returns the preconfigured policy for the group permissions.
    pub fn preconfigured_policy(
        &self,
    ) -> Result<PreconfiguredPolicies, GroupMutablePermissionsError> {
        Ok(PreconfiguredPolicies::from_policy_set(&self.policies)?)
    }

    /// Creates a GroupMutablePermissions instance from a proto representation.
    pub(crate) fn from_proto(
        proto: GroupMutablePermissionsProto,
    ) -> Result<Self, GroupMutablePermissionsError> {
        if proto.policies.is_none() {
            return Err(GroupMutablePermissionsError::MissingPolicies);
        }
        let policies = proto.policies.expect("checked for none");

        Ok(Self::new(PolicySet::from_proto(policies)?))
    }

    /// Converts the GroupMutablePermissions to its proto representation.
    pub(crate) fn to_proto(
        &self,
    ) -> Result<GroupMutablePermissionsProto, GroupMutablePermissionsError> {
        Ok(GroupMutablePermissionsProto {
            policies: Some(self.policies.to_proto()?),
        })
    }
}

/// Implements conversion from GroupMutablePermissions to `Vec<u8>`.
impl TryFrom<GroupMutablePermissions> for Vec<u8> {
    type Error = GroupMutablePermissionsError;

    fn try_from(value: GroupMutablePermissions) -> Result<Self, Self::Error> {
        let mut buf = Vec::new();
        let proto_val = value.to_proto()?;
        proto_val.encode(&mut buf)?;

        Ok(buf)
    }
}

/// Implements conversion from `&Vec<u8>` to [`GroupMutablePermissions`].
impl TryFrom<&Vec<u8>> for GroupMutablePermissions {
    type Error = GroupMutablePermissionsError;

    fn try_from(value: &Vec<u8>) -> Result<Self, Self::Error> {
        let proto_val = GroupMutablePermissionsProto::decode(value.as_slice())?;
        Self::from_proto(proto_val)
    }
}

/// Implements conversion from GroupMutablePermissionsProto to GroupMutablePermissions.
impl TryFrom<GroupMutablePermissionsProto> for GroupMutablePermissions {
    type Error = GroupMutablePermissionsError;

    fn try_from(value: GroupMutablePermissionsProto) -> Result<Self, Self::Error> {
        Self::from_proto(value)
    }
}

/// Implements conversion from &Extensions to GroupMutablePermissions.
impl TryFrom<&Extensions<GroupContext>> for GroupMutablePermissions {
    type Error = GroupMutablePermissionsError;

    fn try_from(value: &Extensions<GroupContext>) -> Result<Self, Self::Error> {
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

/// Implements conversion from &OpenMlsGroup to GroupMutablePermissions.
impl TryFrom<&OpenMlsGroup> for GroupMutablePermissions {
    type Error = GroupMutablePermissionsError;

    fn try_from(value: &OpenMlsGroup) -> Result<Self, Self::Error> {
        let extensions = value.extensions();
        extensions.try_into()
    }
}

/// Extracts group permissions from an OpenMlsGroup.
pub fn extract_group_permissions(
    group: &OpenMlsGroup,
) -> Result<GroupMutablePermissions, GroupMutablePermissionsError> {
    let extensions = group.extensions();
    extensions.try_into()
}

/// A trait for policies that can update Metadata for the group.
pub trait MetadataPolicy: std::fmt::Debug {
    /// Evaluates the policy for a given actor and metadata change.
    ///
    /// Verify relevant metadata is actually changed before evaluating against the MetadataPolicy.
    /// See evaluate_metadata_policy.
    fn evaluate(&self, actor: &CommitParticipant, change: &MetadataFieldChange) -> bool;

    /// Converts the policy to its proto representation.
    fn to_proto(&self) -> Result<MetadataPolicyProto, PolicyError>;
}

/// Represents the base policies for metadata updates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MetadataBasePolicies {
    Allow,
    Deny,
    AllowIfActorAdminOrSuperAdmin,
    AllowIfActorSuperAdmin,
}

/// Implements the MetadataPolicy trait for MetadataBasePolicies.
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

/// Represents the different types of metadata policies.
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum MetadataPolicies {
    Standard(MetadataBasePolicies),
    AndCondition(MetadataAndCondition),
    AnyCondition(MetadataAnyCondition),
}

impl MetadataPolicies {
    /// Creates a default map of metadata policies.
    pub fn default_map(policies: MetadataPolicies) -> HashMap<String, MetadataPolicies> {
        let mut map: HashMap<String, MetadataPolicies> = HashMap::new();
        for field in GroupMutableMetadata::supported_fields() {
            match field {
                MetadataField::MessageDisappearInNS => {
                    map.insert(field.to_string(), MetadataPolicies::allow_if_actor_admin());
                }
                MetadataField::MessageDisappearFromNS => {
                    map.insert(field.to_string(), MetadataPolicies::allow_if_actor_admin());
                }
                MetadataField::MinimumSupportedProtocolVersion => {
                    map.insert(
                        field.to_string(),
                        MetadataPolicies::allow_if_actor_super_admin(),
                    );
                }
                _ => {
                    map.insert(field.to_string(), policies.clone());
                }
            }
        }
        map
    }

    // by default members of DM groups can update all metadata
    pub fn dm_map() -> HashMap<String, MetadataPolicies> {
        let mut map: HashMap<String, MetadataPolicies> = HashMap::new();
        for field in GroupMutableMetadata::supported_fields() {
            map.insert(field.to_string(), MetadataPolicies::allow());
        }
        map
    }

    /// Creates an "Allow" metadata policy.
    pub fn allow() -> Self {
        MetadataPolicies::Standard(MetadataBasePolicies::Allow)
    }

    /// Creates a "Deny" metadata policy.
    pub fn deny() -> Self {
        MetadataPolicies::Standard(MetadataBasePolicies::Deny)
    }

    /// Creates an "Allow if actor is admin" metadata policy.
    pub fn allow_if_actor_admin() -> Self {
        MetadataPolicies::Standard(MetadataBasePolicies::AllowIfActorAdminOrSuperAdmin)
    }

    /// Creates an "Allow if actor is super admin" metadata policy.
    pub fn allow_if_actor_super_admin() -> Self {
        MetadataPolicies::Standard(MetadataBasePolicies::AllowIfActorSuperAdmin)
    }

    /// Creates an "And" condition metadata policy.
    pub fn and(policies: Vec<MetadataPolicies>) -> Self {
        MetadataPolicies::AndCondition(MetadataAndCondition::new(policies))
    }

    /// Creates an "Any" condition metadata policy.
    pub fn any(policies: Vec<MetadataPolicies>) -> Self {
        MetadataPolicies::AnyCondition(MetadataAnyCondition::new(policies))
    }
}

/// Implements conversion from MetadataPolicyProto to MetadataPolicies.
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

/// Implements the MetadataPolicy trait for MetadataPolicies.
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

/// An AndCondition evaluates to true if all the policies it contains evaluate to true.
#[derive(Clone, Debug, PartialEq)]
pub struct MetadataAndCondition {
    policies: Vec<MetadataPolicies>,
}

impl MetadataAndCondition {
    pub(super) fn new(policies: Vec<MetadataPolicies>) -> Self {
        Self { policies }
    }
}

/// Implements the MetadataPolicy trait for MetadataAndCondition.
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

/// An AnyCondition evaluates to true if any of the contained policies evaluate to true.
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

/// Implements the MetadataPolicy trait for MetadataAnyCondition.
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

/// A trait for policies that can update Permissions for the group.
pub trait PermissionsPolicy: std::fmt::Debug {
    /// Evaluates the policy for a given actor.
    fn evaluate(&self, actor: &CommitParticipant) -> bool;

    /// Converts the policy to its proto representation.
    fn to_proto(&self) -> Result<PermissionsPolicyProto, PolicyError>;
}

/// Represents the base policies for permissions updates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PermissionsBasePolicies {
    Deny,
    AllowIfActorAdminOrSuperAdmin,
    AllowIfActorSuperAdmin,
}

/// Implements the PermissionsPolicy trait for PermissionsBasePolicies.
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

/// Represents the different types of permissions policies.
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum PermissionsPolicies {
    Standard(PermissionsBasePolicies),
    AndCondition(PermissionsAndCondition),
    AnyCondition(PermissionsAnyCondition),
}

impl PermissionsPolicies {
    /// Creates a "Deny" permissions policy.
    pub fn deny() -> Self {
        PermissionsPolicies::Standard(PermissionsBasePolicies::Deny)
    }

    /// Creates an "Allow if actor is admin" permissions policy.
    pub fn allow_if_actor_admin() -> Self {
        PermissionsPolicies::Standard(PermissionsBasePolicies::AllowIfActorAdminOrSuperAdmin)
    }

    /// Creates an "Allow if actor is super admin" permissions policy.
    pub fn allow_if_actor_super_admin() -> Self {
        PermissionsPolicies::Standard(PermissionsBasePolicies::AllowIfActorSuperAdmin)
    }

    /// Creates an "And" condition permissions policy.
    pub fn and(policies: Vec<PermissionsPolicies>) -> Self {
        PermissionsPolicies::AndCondition(PermissionsAndCondition::new(policies))
    }

    /// Creates an "Any" condition permissions policy.
    pub fn any(policies: Vec<PermissionsPolicies>) -> Self {
        PermissionsPolicies::AnyCondition(PermissionsAnyCondition::new(policies))
    }
}

/// Implements conversion from PermissionsPolicyProto to PermissionsPolicies.
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

/// Implements the PermissionsPolicy trait for PermissionsPolicies.
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

/// An AndCondition evaluates to true if all the policies it contains evaluate to true.
#[derive(Clone, Debug, PartialEq)]
pub struct PermissionsAndCondition {
    policies: Vec<PermissionsPolicies>,
}

impl PermissionsAndCondition {
    pub(super) fn new(policies: Vec<PermissionsPolicies>) -> Self {
        Self { policies }
    }
}

/// Implements the PermissionsPolicy trait for PermissionsAndCondition.
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

/// An AnyCondition evaluates to true if any of the contained policies evaluate to true.
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

/// Implements the PermissionsPolicy trait for PermissionsAnyCondition.
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

/// A trait for policies that can add/remove members and installations for the group.
pub trait MembershipPolicy: std::fmt::Debug {
    /// Evaluates the policy for a given actor and inbox change.
    fn evaluate(&self, actor: &CommitParticipant, change: &Inbox) -> bool;

    /// Converts the policy to its proto representation.
    fn to_proto(&self) -> Result<MembershipPolicyProto, PolicyError>;
}

/// Errors that can occur when working with policies.
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

/// Represents the base policies for membership updates.
#[derive(Debug, Clone, Copy, PartialEq)]
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

/// Implements the MembershipPolicy trait for BasePolicies.
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

/// Represents the different types of membership policies.
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum MembershipPolicies {
    Standard(BasePolicies),
    AndCondition(AndCondition),
    AnyCondition(AnyCondition),
}

impl MembershipPolicies {
    /// Creates an "Allow" membership policy.
    pub fn allow() -> Self {
        MembershipPolicies::Standard(BasePolicies::Allow)
    }

    /// Creates a "Deny" membership policy.
    pub fn deny() -> Self {
        MembershipPolicies::Standard(BasePolicies::Deny)
    }

    /// Creates an "Allow if actor is admin" membership policy.
    #[allow(dead_code)]
    pub fn allow_if_actor_admin() -> Self {
        MembershipPolicies::Standard(BasePolicies::AllowIfAdminOrSuperAdmin)
    }

    /// Creates an "Allow if actor is super admin" membership policy.
    #[allow(dead_code)]
    pub fn allow_if_actor_super_admin() -> Self {
        MembershipPolicies::Standard(BasePolicies::AllowIfSuperAdmin)
    }

    /// Creates an "And" condition membership policy.
    pub fn and(policies: Vec<MembershipPolicies>) -> Self {
        MembershipPolicies::AndCondition(AndCondition::new(policies))
    }

    /// Creates an "Any" condition membership policy.
    pub fn any(policies: Vec<MembershipPolicies>) -> Self {
        MembershipPolicies::AnyCondition(AnyCondition::new(policies))
    }
}

/// Implements conversion from MembershipPolicyProto to MembershipPolicies.
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

/// Implements the MembershipPolicy trait for MembershipPolicies.
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

/// An AndCondition evaluates to true if all the policies it contains evaluate to true.
#[derive(Clone, Debug, PartialEq)]
pub struct AndCondition {
    policies: Vec<MembershipPolicies>,
}

impl AndCondition {
    pub(super) fn new(policies: Vec<MembershipPolicies>) -> Self {
        Self { policies }
    }
}

/// Implements the MembershipPolicy trait for AndCondition.
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

/// An AnyCondition evaluates to true if any of the contained policies evaluate to true.
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

/// Implements the MembershipPolicy trait for AnyCondition.
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

/// Represents a set of policies for a group.
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub struct PolicySet {
    /// The policy for adding members to the group.
    pub add_member_policy: MembershipPolicies,
    /// The policy for removing members from the group.
    pub remove_member_policy: MembershipPolicies,
    /// The policies for updating metadata fields.
    pub update_metadata_policy: HashMap<String, MetadataPolicies>,
    /// The policy for adding admins to the group.
    pub add_admin_policy: PermissionsPolicies,
    /// The policy for removing admins from the group.
    pub remove_admin_policy: PermissionsPolicies,
    /// The policy for updating permissions.
    pub update_permissions_policy: PermissionsPolicies,
}

impl PolicySet {
    /// Creates a new PolicySet instance.
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

    pub fn new_dm() -> Self {
        Self {
            add_member_policy: MembershipPolicies::deny(),
            remove_member_policy: MembershipPolicies::deny(),
            update_metadata_policy: MetadataPolicies::dm_map(),
            add_admin_policy: PermissionsPolicies::deny(),
            remove_admin_policy: PermissionsPolicies::deny(),
            update_permissions_policy: PermissionsPolicies::deny(),
        }
    }

    /// The [`evaluate_commit`](Self::evaluate_commit) function is the core function for client side verification
    /// that [ValidatedCommit]
    /// adheres to the XMTP permission policies set in the PolicySet.
    pub fn evaluate_commit(&self, commit: &ValidatedCommit) -> bool {
        // Verify add member policy was not violated
        let mut added_inboxes_valid = self.evaluate_policy(
            commit.added_inboxes.iter(),
            &self.add_member_policy,
            &commit.actor,
        );

        // We can always add DM member's inboxId to a DM
        if let Some(dm_members) = &commit.dm_members
            && commit.added_inboxes.len() == 1
        {
            let added_inbox_id = &commit.added_inboxes[0].inbox_id;
            if (added_inbox_id == &dm_members.member_one_inbox_id
                || added_inbox_id == &dm_members.member_two_inbox_id)
                && added_inbox_id != &commit.actor_inbox_id()
            {
                added_inboxes_valid = true;
            }
        }

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
            commit
                .metadata_validation_info
                .metadata_field_changes
                .iter(),
            &self.update_metadata_policy,
            &commit.actor,
        );

        // Verify that add admin policy was not violated
        let added_admins_valid = commit.metadata_validation_info.admins_added.is_empty()
            || self.add_admin_policy.evaluate(&commit.actor);

        // Verify that remove admin policy was not violated
        let removed_admins_valid = commit.metadata_validation_info.admins_removed.is_empty()
            || self.remove_admin_policy.evaluate(&commit.actor);

        // Verify that super admin add policy was not violated
        let super_admin_add_valid = commit
            .metadata_validation_info
            .super_admins_added
            .is_empty()
            || commit.actor.is_super_admin;

        // Verify that super admin remove policy was not violated
        // You can never remove the last super admin
        let super_admin_remove_valid = commit
            .metadata_validation_info
            .super_admins_removed
            .is_empty()
            || (commit.actor.is_super_admin
                && commit.metadata_validation_info.num_super_admins > 0);

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

    /// Evaluates a policy for a given set of changes.
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
                tracing::info!(
                    "Policy {:?} failed for actor {:?} and change {:?}",
                    policy,
                    actor,
                    change
                );
            }
            is_ok
        })
    }

    /// Evaluates metadata policies for a given set of changes.
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
                    tracing::info!(
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
                tracing::info!(
                    "Metadata field update with unknown policy was denied: {}",
                    change.field_name
                );
                return false;
            }
            true
        })
    }

    /// Converts the PolicySet to its proto representation.
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

    /// Creates a PolicySet from its proto representation.
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

    /// Converts the PolicySet to a `Vec<u8>`.
    pub fn to_bytes(&self) -> Result<Vec<u8>, PolicyError> {
        let proto = self.to_proto()?;
        let mut buf = Vec::new();
        proto.encode(&mut buf)?;
        Ok(buf)
    }

    /// Creates a PolicySet from a `Vec<u8>`.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, PolicyError> {
        let proto = PolicySetProto::decode(bytes)?;
        Self::from_proto(proto)
    }
}

/// Checks if a PolicySet is equivalent to the "All Members" preconfigured policy.
///
/// Depending on if the client is on a newer or older version of libxmtp
/// since the group was created, the number of metadata policies might not match
/// the default All Members Policy Set. As long as all metadata policies are allow, we will
/// match against All Members Preconfigured Policy
pub fn is_policy_default(policy: &PolicySet) -> Result<bool, PolicyError> {
    let mut metadata_policies_equal = true;
    for field_name in policy.update_metadata_policy.keys() {
        let metadata_policy = policy.update_metadata_policy.get(field_name).ok_or(
            PolicyError::MissingMetadataPolicyField {
                name: field_name.to_string(),
            },
        )?;
        if field_name == MetadataField::MessageDisappearInNS.as_str()
            || field_name == MetadataField::MessageDisappearFromNS.as_str()
        {
            metadata_policies_equal = metadata_policies_equal
                && metadata_policy.eq(&MetadataPolicies::allow_if_actor_admin());
        } else if field_name == MetadataField::MinimumSupportedProtocolVersion.as_str() {
            metadata_policies_equal = metadata_policies_equal
                && metadata_policy.eq(&MetadataPolicies::allow_if_actor_super_admin());
        } else {
            metadata_policies_equal =
                metadata_policies_equal && metadata_policy.eq(&MetadataPolicies::allow());
        }
    }
    Ok(metadata_policies_equal
        && policy.add_member_policy == MembershipPolicies::allow()
        && policy.remove_member_policy == MembershipPolicies::allow_if_actor_admin()
        && policy.add_admin_policy == PermissionsPolicies::allow_if_actor_super_admin()
        && policy.remove_admin_policy == PermissionsPolicies::allow_if_actor_super_admin()
        && policy.update_permissions_policy == PermissionsPolicies::allow_if_actor_super_admin())
}

/// Checks if a PolicySet is equivalent to the "Admin Only" preconfigured policy.
///
/// Depending on if the client is on a newer or older version of libxmtp
/// since the group was created, the number of metadata policies might not match
/// the default Admin Only Policy Set. As long as all metadata policies are admin only, we will
/// match against Admin Only Preconfigured Policy
pub fn is_policy_admin_only(policy: &PolicySet) -> Result<bool, PolicyError> {
    let mut metadata_policies_equal = true;
    for field_name in policy.update_metadata_policy.keys() {
        let metadata_policy = policy.update_metadata_policy.get(field_name).ok_or(
            PolicyError::MissingMetadataPolicyField {
                name: field_name.to_string(),
            },
        )?;
        if field_name == MetadataField::MinimumSupportedProtocolVersion.as_str() {
            metadata_policies_equal = metadata_policies_equal
                && metadata_policy.eq(&MetadataPolicies::allow_if_actor_super_admin());
        } else {
            metadata_policies_equal = metadata_policies_equal
                && metadata_policy.eq(&MetadataPolicies::allow_if_actor_admin());
        }
    }
    Ok(metadata_policies_equal
        && policy.add_member_policy == MembershipPolicies::allow_if_actor_admin()
        && policy.remove_member_policy == MembershipPolicies::allow_if_actor_admin()
        && policy.add_admin_policy == PermissionsPolicies::allow_if_actor_super_admin()
        && policy.remove_admin_policy == PermissionsPolicies::allow_if_actor_super_admin()
        && policy.update_permissions_policy == PermissionsPolicies::allow_if_actor_super_admin())
}

/// Returns the "All Members" preconfigured policy.
///
/// A policy where any member can add or remove any other member
pub(crate) fn default_policy() -> PolicySet {
    let mut metadata_policies_map: HashMap<String, MetadataPolicies> = HashMap::new();
    for field in GroupMutableMetadata::supported_fields() {
        match field {
            MetadataField::MessageDisappearInNS => {
                metadata_policies_map
                    .insert(field.to_string(), MetadataPolicies::allow_if_actor_admin());
            }
            MetadataField::MessageDisappearFromNS => {
                metadata_policies_map
                    .insert(field.to_string(), MetadataPolicies::allow_if_actor_admin());
            }
            MetadataField::MinimumSupportedProtocolVersion => {
                metadata_policies_map.insert(
                    field.to_string(),
                    MetadataPolicies::allow_if_actor_super_admin(),
                );
            }
            _ => {
                metadata_policies_map.insert(field.to_string(), MetadataPolicies::allow());
            }
        }
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

/// Returns the "Admin Only" preconfigured policy.
///
/// A policy where only the admins can add or remove members
pub(crate) fn policy_admin_only() -> PolicySet {
    let mut metadata_policies_map: HashMap<String, MetadataPolicies> = HashMap::new();
    for field in GroupMutableMetadata::supported_fields() {
        match field {
            MetadataField::MessageDisappearInNS => {
                metadata_policies_map
                    .insert(field.to_string(), MetadataPolicies::allow_if_actor_admin());
            }
            MetadataField::MessageDisappearFromNS => {
                metadata_policies_map
                    .insert(field.to_string(), MetadataPolicies::allow_if_actor_admin());
            }
            MetadataField::MinimumSupportedProtocolVersion => {
                metadata_policies_map.insert(
                    field.to_string(),
                    MetadataPolicies::allow_if_actor_super_admin(),
                );
            }
            _ => {
                metadata_policies_map
                    .insert(field.to_string(), MetadataPolicies::allow_if_actor_admin());
            }
        }
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

/// Implements the Default trait for PolicySet.
impl Default for PolicySet {
    fn default() -> Self {
        PreconfiguredPolicies::default().to_policy_set()
    }
}

/// Represents preconfigured policies for a group.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum PreconfiguredPolicies {
    /// The "All Members" preconfigured policy.
    #[default]
    Default,
    /// The "Admin Only" preconfigured policy.
    AdminsOnly,
}

impl PreconfiguredPolicies {
    /// Converts the PreconfiguredPolicies to a PolicySet.
    pub fn to_policy_set(&self) -> PolicySet {
        match self {
            PreconfiguredPolicies::Default => default_policy(),
            PreconfiguredPolicies::AdminsOnly => policy_admin_only(),
        }
    }

    /// Creates a PreconfiguredPolicies from a PolicySet.
    pub fn from_policy_set(policy_set: &PolicySet) -> Result<Self, PolicyError> {
        if is_policy_default(policy_set)? {
            Ok(PreconfiguredPolicies::Default)
        } else if is_policy_admin_only(policy_set)? {
            Ok(PreconfiguredPolicies::AdminsOnly)
        } else {
            Err(PolicyError::InvalidPresetPolicy)
        }
    }
}

/// Implements the Display trait for PreconfiguredPolicies.
impl std::fmt::Display for PreconfiguredPolicies {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use std::collections::HashSet;

    use crate::groups::validated_commit::MutableMetadataValidationInfo;
    use xmtp_common::{rand_string, rand_vec};
    use xmtp_mls_common::group_metadata::DmMembers;

    use super::*;

    fn build_change(inbox_id: Option<String>, is_admin: bool, is_super_admin: bool) -> Inbox {
        Inbox {
            inbox_id: inbox_id.unwrap_or(rand_string::<24>()),
            is_creator: is_super_admin,
            is_super_admin,
            is_admin,
        }
    }

    /// Test helper function for building a CommitParticipant.
    fn build_actor(
        inbox_id: Option<String>,
        installation_id: Option<Vec<u8>>,
        is_admin: bool,
        is_super_admin: bool,
    ) -> CommitParticipant {
        CommitParticipant {
            inbox_id: inbox_id.unwrap_or(rand_string::<24>()),
            installation_id: installation_id.unwrap_or_else(rand_vec::<24>),
            is_creator: is_super_admin,
            is_admin,
            is_super_admin,
        }
    }

    enum MemberType {
        SameAsActor,
        DmTarget,
        Random,
    }

    /// Test helper function for building a ValidatedCommit.
    fn build_validated_commit(
        // Add a member with the same account address as the actor if true, random account address if false
        member_added: Option<MemberType>,
        member_removed: Option<MemberType>,
        metadata_fields_changed: Option<Vec<String>>,
        permissions_changed: bool,
        actor_is_admin: bool,
        actor_is_super_admin: bool,
        dm_target_inbox_id: Option<String>,
    ) -> ValidatedCommit {
        let actor = build_actor(None, None, actor_is_admin, actor_is_super_admin);
        let dm_target_inbox_id_clone = dm_target_inbox_id.clone();
        let build_membership_change = |member_type: MemberType| match member_type {
            MemberType::SameAsActor => vec![build_change(
                Some(actor.inbox_id.clone()),
                actor_is_admin,
                actor_is_super_admin,
            )],
            MemberType::DmTarget => {
                vec![build_change(dm_target_inbox_id_clone.clone(), false, false)]
            }
            MemberType::Random => vec![build_change(None, false, false)],
        };

        let field_changes = metadata_fields_changed
            .unwrap_or_default()
            .into_iter()
            .map(|field| {
                MetadataFieldChange::new(
                    field,
                    Some(rand_string::<24>()),
                    Some(rand_string::<24>()),
                )
            })
            .collect();

        let dm_members = if let Some(dm_target_inbox_id) = dm_target_inbox_id {
            Some(DmMembers {
                member_one_inbox_id: actor.inbox_id.clone(),
                member_two_inbox_id: dm_target_inbox_id,
            })
        } else {
            None
        };

        ValidatedCommit {
            actor: actor.clone(),
            added_inboxes: member_added
                .map(build_membership_change)
                .unwrap_or_default(),
            removed_inboxes: member_removed
                .map(build_membership_change)
                .unwrap_or_default(),
            readded_installations: HashSet::new(),
            metadata_validation_info: MutableMetadataValidationInfo {
                metadata_field_changes: field_changes,
                ..Default::default()
            },
            installations_changed: false,
            permissions_changed,
            dm_members,
        }
    }

    /// Tests that a commit by a non admin/super admin can add and remove members
    /// with allow policies.
    #[xmtp_common::test]
    fn test_allow_all() {
        let permissions = PolicySet::new(
            MembershipPolicies::allow(),
            MembershipPolicies::allow(),
            MetadataPolicies::default_map(MetadataPolicies::allow()),
            PermissionsPolicies::allow_if_actor_super_admin(),
            PermissionsPolicies::allow_if_actor_super_admin(),
            PermissionsPolicies::allow_if_actor_super_admin(),
        );

        let commit = build_validated_commit(
            Some(MemberType::SameAsActor),
            Some(MemberType::SameAsActor),
            None,
            false,
            false,
            false,
            None,
        );
        assert!(permissions.evaluate_commit(&commit));
    }

    /// Tests that a commit by a non admin/super admin is denied for add and remove member policies.
    #[xmtp_common::test]
    fn test_deny() {
        let permissions = PolicySet::new(
            MembershipPolicies::deny(),
            MembershipPolicies::deny(),
            MetadataPolicies::default_map(MetadataPolicies::deny()),
            PermissionsPolicies::allow_if_actor_super_admin(),
            PermissionsPolicies::allow_if_actor_super_admin(),
            PermissionsPolicies::allow_if_actor_super_admin(),
        );

        let member_added_commit = build_validated_commit(
            Some(MemberType::Random),
            None,
            None,
            false,
            false,
            false,
            None,
        );
        assert!(!permissions.evaluate_commit(&member_added_commit));

        let member_removed_commit = build_validated_commit(
            None,
            Some(MemberType::Random),
            None,
            false,
            false,
            false,
            None,
        );
        assert!(!permissions.evaluate_commit(&member_removed_commit));
    }

    /// Tests that a group creator can perform super admin actions.
    #[xmtp_common::test]
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
        let commit_with_creator = build_validated_commit(
            Some(MemberType::SameAsActor),
            Some(MemberType::SameAsActor),
            None,
            false,
            false,
            true,
            None,
        );
        assert!(!permissions.evaluate_commit(&commit_with_creator));

        let commit_with_creator = build_validated_commit(
            Some(MemberType::SameAsActor),
            Some(MemberType::Random),
            None,
            false,
            false,
            true,
            None,
        );
        assert!(permissions.evaluate_commit(&commit_with_creator));

        let commit_without_creator = build_validated_commit(
            Some(MemberType::SameAsActor),
            Some(MemberType::SameAsActor),
            None,
            false,
            false,
            false,
            None,
        );
        assert!(!permissions.evaluate_commit(&commit_without_creator));
    }

    /// Tests that and conditions are enforced as expected.
    #[xmtp_common::test]
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

        let member_added_commit = build_validated_commit(
            Some(MemberType::SameAsActor),
            None,
            None,
            false,
            false,
            false,
            None,
        );
        assert!(!permissions.evaluate_commit(&member_added_commit));
    }

    /// Tests that any conditions are enforced as expected.
    #[xmtp_common::test]
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

        let member_added_commit = build_validated_commit(
            Some(MemberType::SameAsActor),
            None,
            None,
            false,
            false,
            false,
            None,
        );
        assert!(permissions.evaluate_commit(&member_added_commit));
    }

    /// Tests that the PolicySet can be serialized and deserialized.
    #[xmtp_common::test]
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

    /// Tests that the PolicySet can enforce update group name policy.
    #[xmtp_common::test]
    /// Tests that the PolicySet can enforce update group name policy.
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
            Some(MemberType::SameAsActor),
            None,
            Some(vec![MetadataField::GroupName.to_string()]),
            false,
            false,
            false,
            None,
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

    /// Tests that the preconfigured policy functions work as expected
    #[xmtp_common::test]
    fn test_preconfigured_policy() {
        let group_permissions = GroupMutablePermissions::new(default_policy());

        assert_eq!(
            group_permissions.preconfigured_policy().unwrap(),
            PreconfiguredPolicies::Default
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

    /// Tests that the preconfigured policy functions work as expected with new metadata fields.
    #[xmtp_common::test]
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

        assert!(is_policy_default(&policy_set_new_metadata_permission).unwrap());

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

    /// Tests that the permission update policy is enforced as expected.
    #[xmtp_common::test]
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
        let commit = build_validated_commit(None, None, None, true, false, false, None);
        assert!(!permissions.evaluate_commit(&commit));

        // Commit should pass because actor is superadmin
        let commit = build_validated_commit(None, None, None, true, false, true, None);
        assert!(permissions.evaluate_commit(&commit));
    }

    /// Tests that the PolicySet can evaluate field updates with unknown policies.
    #[xmtp_common::test]
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
            None,
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
            None,
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
            None,
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
            None,
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
            None,
        );
        assert!(permissions.evaluate_commit(&non_existing_field_updated_commit));
    }

    #[xmtp_common::test]
    fn test_dm_group_permissions() {
        // Simulate a group with DM Permissions
        let permissions = PolicySet::new_dm();

        // String below represents the inbox id of the DM target
        const TARGET_INBOX_ID: &str = "example_target_dm_id";

        // DM group can not add a random inbox
        let commit = build_validated_commit(
            Some(MemberType::Random),
            None,
            None,
            false,
            false,
            false,
            Some(TARGET_INBOX_ID.to_string()),
        );
        assert!(!permissions.evaluate_commit(&commit));

        // DM group can not add themselves
        let commit = build_validated_commit(
            Some(MemberType::SameAsActor),
            None,
            None,
            false,
            false,
            false,
            Some(TARGET_INBOX_ID.to_string()),
        );
        assert!(!permissions.evaluate_commit(&commit));

        // DM group can add the target inbox
        let commit = build_validated_commit(
            Some(MemberType::DmTarget),
            None,
            None,
            false,
            false,
            false,
            Some(TARGET_INBOX_ID.to_string()),
        );
        assert!(permissions.evaluate_commit(&commit));

        // DM group can not remove
        let commit = build_validated_commit(
            None,
            Some(MemberType::Random),
            None,
            false,
            false,
            false,
            Some(TARGET_INBOX_ID.to_string()),
        );
        assert!(!permissions.evaluate_commit(&commit));
        let commit = build_validated_commit(
            None,
            Some(MemberType::DmTarget),
            None,
            false,
            false,
            false,
            Some(TARGET_INBOX_ID.to_string()),
        );
        assert!(!permissions.evaluate_commit(&commit));
        let commit = build_validated_commit(
            None,
            Some(MemberType::SameAsActor),
            None,
            false,
            false,
            false,
            Some(TARGET_INBOX_ID.to_string()),
        );
        assert!(!permissions.evaluate_commit(&commit));

        // DM group can update metadata
        let commit = build_validated_commit(
            None,
            None,
            Some(vec![MetadataField::GroupName.to_string()]),
            false,
            false,
            false,
            Some(TARGET_INBOX_ID.to_string()),
        );
        assert!(permissions.evaluate_commit(&commit));
    }
}
