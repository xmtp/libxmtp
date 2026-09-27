use xmtp_mls::groups::ConversationStateSnapshot;
use xmtp_mls::groups::group_permissions::{
    BasePolicies, MembershipPolicies, MetadataBasePolicies, MetadataPolicies,
    PermissionsBasePolicies, PermissionsPolicies,
};
use xmtp_mls::mls_common::group_mutable_metadata::MetadataField;

use crate::{ConsentState, InboxID, InstallationID, PublicIdentity, Timestamp, XmtpError};

#[derive(Clone, Debug, uniffi::Enum)]
pub enum PermissionLevel {
    Member,
    Admin,
    SuperAdmin,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct Member {
    pub inbox_id: InboxID,
    pub identities: Vec<PublicIdentity>,
    pub permission_level: PermissionLevel,
    pub consent_state: ConsentState,
}

impl TryFrom<xmtp_mls::groups::members::GroupMember> for Member {
    type Error = XmtpError;

    fn try_from(value: xmtp_mls::groups::members::GroupMember) -> Result<Self, Self::Error> {
        Ok(Self {
            inbox_id: InboxID::try_from(value.inbox_id.to_string())?,
            identities: value
                .account_identifiers
                .into_iter()
                .map(Into::into)
                .collect(),
            permission_level: match value.permission_level {
                xmtp_mls::groups::members::PermissionLevel::Member => PermissionLevel::Member,
                xmtp_mls::groups::members::PermissionLevel::Admin => PermissionLevel::Admin,
                xmtp_mls::groups::members::PermissionLevel::SuperAdmin => {
                    PermissionLevel::SuperAdmin
                }
            },
            consent_state: value.consent_state.into(),
        })
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct MembershipResult {
    pub added: Vec<InboxID>,
    pub removed: Vec<InboxID>,
    pub failed_installation_ids: Vec<InstallationID>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct HmacKey {
    pub key: Vec<u8>,
    pub epoch: i64,
}

impl From<xmtp_db::user_preferences::HmacKey> for HmacKey {
    fn from(value: xmtp_db::user_preferences::HmacKey) -> Self {
        Self {
            key: value.key.to_vec(),
            epoch: value.epoch,
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct ConversationHmacKeys {
    pub conversation_id: crate::ConversationID,
    pub keys: Vec<HmacKey>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct LastReadTimeEntry {
    pub inbox_id: InboxID,
    pub read_at: Timestamp,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct ConversationDebugInfo {
    pub epoch: u64,
    pub maybe_forked: bool,
    pub fork_details: String,
    pub is_commit_log_forked: Option<bool>,
    pub local_commit_log: String,
    pub remote_commit_log: String,
    pub cursor: Vec<u64>,
}

impl From<xmtp_mls::groups::ConversationDebugInfo> for ConversationDebugInfo {
    fn from(value: xmtp_mls::groups::ConversationDebugInfo) -> Self {
        Self {
            epoch: value.epoch,
            maybe_forked: value.maybe_forked,
            fork_details: value.fork_details,
            is_commit_log_forked: value.is_commit_log_forked,
            local_commit_log: value.local_commit_log,
            remote_commit_log: value.remote_commit_log,
            cursor: value.cursor.into_iter().map(|cursor| cursor.0).collect(),
        }
    }
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum MlsExtensionType {
    ApplicationId,
    RatchetTree,
    RequiredCapabilities,
    ExternalPub,
    ExternalSenders,
    LastResort,
    ImmutableMetadata,
    AppDataDictionary,
    Unknown { id: u16 },
    Grease { id: u16 },
}

impl From<xmtp_mls::groups::MlsExtensionType> for MlsExtensionType {
    fn from(value: xmtp_mls::groups::MlsExtensionType) -> Self {
        use xmtp_mls::groups::MlsExtensionType as Core;
        match value {
            Core::ApplicationId => Self::ApplicationId,
            Core::RatchetTree => Self::RatchetTree,
            Core::RequiredCapabilities => Self::RequiredCapabilities,
            Core::ExternalPub => Self::ExternalPub,
            Core::ExternalSenders => Self::ExternalSenders,
            Core::LastResort => Self::LastResort,
            Core::ImmutableMetadata => Self::ImmutableMetadata,
            Core::AppDataDictionary => Self::AppDataDictionary,
            Core::Unknown(id) => Self::Unknown { id },
            Core::Grease(id) => Self::Grease { id },
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct InstallationCapabilities {
    pub installation_id: InstallationID,
    pub is_own: bool,
    pub supported_extensions: Vec<MlsExtensionType>,
    pub capabilities_known: bool,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct InboxCapabilities {
    pub inbox_id: InboxID,
    pub installations: Vec<InstallationCapabilities>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct GroupMembershipCapabilities {
    pub context_extensions: Vec<MlsExtensionType>,
    pub members: Vec<InboxCapabilities>,
}

impl TryFrom<xmtp_mls::groups::GroupMembershipCapabilities> for GroupMembershipCapabilities {
    type Error = XmtpError;

    fn try_from(value: xmtp_mls::groups::GroupMembershipCapabilities) -> Result<Self, Self::Error> {
        Ok(Self {
            context_extensions: value
                .context_extensions
                .into_iter()
                .map(Into::into)
                .collect(),
            members: value
                .members
                .into_iter()
                .map(|member| {
                    Ok(InboxCapabilities {
                        inbox_id: InboxID::try_from(member.inbox_id.to_string())?,
                        installations: member
                            .installations
                            .into_iter()
                            .map(|installation| {
                                Ok(InstallationCapabilities {
                                    installation_id: InstallationID::try_from(hex::encode(
                                        installation.installation_id,
                                    ))?,
                                    is_own: installation.is_own,
                                    supported_extensions: installation
                                        .supported_extensions
                                        .into_iter()
                                        .map(Into::into)
                                        .collect(),
                                    capabilities_known: installation.capabilities_known,
                                })
                            })
                            .collect::<Result<_, XmtpError>>()?,
                    })
                })
                .collect::<Result<_, XmtpError>>()?,
        })
    }
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum NotificationOverride {
    Enabled,
    Disabled,
    Default,
}

impl From<NotificationOverride> for xmtp_mls::client::notifications::NotificationOverride {
    fn from(value: NotificationOverride) -> Self {
        match value {
            NotificationOverride::Enabled => Self::Enabled,
            NotificationOverride::Disabled => Self::Disabled,
            NotificationOverride::Default => Self::Default,
        }
    }
}

impl TryFrom<xmtp_mls::groups::UpdateGroupMembershipResult> for MembershipResult {
    type Error = XmtpError;

    fn try_from(value: xmtp_mls::groups::UpdateGroupMembershipResult) -> Result<Self, Self::Error> {
        Ok(Self {
            added: value
                .added_members
                .into_keys()
                .map(InboxID::try_from)
                .collect::<Result<_, _>>()?,
            removed: value
                .removed_members
                .into_iter()
                .map(InboxID::try_from)
                .collect::<Result<_, _>>()?,
            failed_installation_ids: value
                .failed_installations
                .into_iter()
                .map(|bytes| InstallationID::try_from(hex::encode(bytes)))
                .collect::<Result<_, _>>()?,
        })
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct DisappearingSettings {
    pub from: Timestamp,
    pub retention_ns: i64,
}

impl From<DisappearingSettings>
    for xmtp_mls::mls_common::group_mutable_metadata::MessageDisappearingSettings
{
    fn from(value: DisappearingSettings) -> Self {
        Self::new(value.from.0, value.retention_ns)
    }
}

impl TryFrom<PermissionPolicySet> for xmtp_mls::groups::group_permissions::PolicySet {
    type Error = XmtpError;

    fn try_from(value: PermissionPolicySet) -> Result<Self, Self::Error> {
        use std::collections::HashMap;
        use xmtp_mls::groups::group_permissions::PolicySet;

        fn member(policy: PermissionPolicy) -> Result<MembershipPolicies, XmtpError> {
            Ok(match policy {
                PermissionPolicy::Allow => MembershipPolicies::allow(),
                PermissionPolicy::Deny => MembershipPolicies::deny(),
                PermissionPolicy::Admin => MembershipPolicies::allow_if_actor_admin(),
                PermissionPolicy::SuperAdmin => MembershipPolicies::allow_if_actor_super_admin(),
                _ => return Err(XmtpError::invalid("unsupported member policy")),
            })
        }
        fn admin(policy: PermissionPolicy) -> Result<PermissionsPolicies, XmtpError> {
            Ok(match policy {
                PermissionPolicy::Deny => PermissionsPolicies::deny(),
                PermissionPolicy::Admin => PermissionsPolicies::allow_if_actor_admin(),
                PermissionPolicy::SuperAdmin => PermissionsPolicies::allow_if_actor_super_admin(),
                _ => return Err(XmtpError::invalid("unsupported admin policy")),
            })
        }
        fn metadata(policy: PermissionPolicy) -> Result<MetadataPolicies, XmtpError> {
            Ok(match policy {
                PermissionPolicy::Allow => MetadataPolicies::allow(),
                PermissionPolicy::Deny => MetadataPolicies::deny(),
                PermissionPolicy::Admin => MetadataPolicies::allow_if_actor_admin(),
                PermissionPolicy::SuperAdmin => MetadataPolicies::allow_if_actor_super_admin(),
                _ => return Err(XmtpError::invalid("unsupported metadata policy")),
            })
        }
        let mut metadata_policies = HashMap::new();
        for (field, policy) in [
            (MetadataField::GroupName, value.update_name),
            (MetadataField::Description, value.update_description),
            (MetadataField::GroupImageUrlSquare, value.update_image),
            (
                MetadataField::MessageDisappearFromNS,
                value.update_disappearing.clone(),
            ),
            (
                MetadataField::MessageDisappearInNS,
                value.update_disappearing,
            ),
            (MetadataField::AppData, value.update_app_data),
        ] {
            metadata_policies.insert(field.to_string(), metadata(policy)?);
        }
        Ok(PolicySet {
            add_member_policy: member(value.add_member)?,
            remove_member_policy: member(value.remove_member)?,
            add_admin_policy: admin(value.add_admin)?,
            remove_admin_policy: admin(value.remove_admin)?,
            update_metadata_policy: metadata_policies,
            update_permissions_policy: PermissionsPolicies::allow_if_actor_super_admin(),
        })
    }
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum CommitLogForkStatus {
    Forked,
    NotForked,
    Unknown,
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum MembershipState {
    Allowed,
    Rejected,
    Pending,
    Restored,
    PendingRemove,
}

#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum PermissionPolicy {
    Allow,
    Deny,
    Admin,
    SuperAdmin,
    DoesNotExist,
    Other,
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum GroupPolicyType {
    AllMembers,
    AdminOnly,
    Custom,
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum PermissionUpdateKind {
    AddMember,
    RemoveMember,
    AddAdmin,
    RemoveAdmin,
    UpdateMetadata,
}

impl From<PermissionUpdateKind> for xmtp_mls::groups::intents::PermissionUpdateType {
    fn from(value: PermissionUpdateKind) -> Self {
        match value {
            PermissionUpdateKind::AddMember => Self::AddMember,
            PermissionUpdateKind::RemoveMember => Self::RemoveMember,
            PermissionUpdateKind::AddAdmin => Self::AddAdmin,
            PermissionUpdateKind::RemoveAdmin => Self::RemoveAdmin,
            PermissionUpdateKind::UpdateMetadata => Self::UpdateMetadata,
        }
    }
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum MetadataFieldKind {
    Name,
    Description,
    ImageUrl,
    Disappearing,
    AppData,
}

impl From<MetadataFieldKind> for MetadataField {
    fn from(value: MetadataFieldKind) -> Self {
        match value {
            MetadataFieldKind::Name => Self::GroupName,
            MetadataFieldKind::Description => Self::Description,
            MetadataFieldKind::ImageUrl => Self::GroupImageUrlSquare,
            MetadataFieldKind::Disappearing => Self::MessageDisappearInNS,
            MetadataFieldKind::AppData => Self::AppData,
        }
    }
}

impl TryFrom<PermissionPolicy> for xmtp_mls::groups::intents::PermissionPolicyOption {
    type Error = XmtpError;

    fn try_from(value: PermissionPolicy) -> Result<Self, Self::Error> {
        match value {
            PermissionPolicy::Allow => Ok(Self::Allow),
            PermissionPolicy::Deny => Ok(Self::Deny),
            PermissionPolicy::Admin => Ok(Self::AdminOnly),
            PermissionPolicy::SuperAdmin => Ok(Self::SuperAdminOnly),
            _ => Err(XmtpError::invalid("unsupported permission policy")),
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct PermissionPolicySet {
    pub add_member: PermissionPolicy,
    pub remove_member: PermissionPolicy,
    pub add_admin: PermissionPolicy,
    pub remove_admin: PermissionPolicy,
    pub update_name: PermissionPolicy,
    pub update_description: PermissionPolicy,
    pub update_image: PermissionPolicy,
    pub update_disappearing: PermissionPolicy,
    pub update_app_data: PermissionPolicy,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct GroupPermissions {
    pub policy_type: GroupPolicyType,
    pub policy_set: PermissionPolicySet,
}

impl From<&MembershipPolicies> for PermissionPolicy {
    fn from(value: &MembershipPolicies) -> Self {
        match value {
            MembershipPolicies::Standard(BasePolicies::Allow) => Self::Allow,
            MembershipPolicies::Standard(BasePolicies::Deny) => Self::Deny,
            MembershipPolicies::Standard(BasePolicies::AllowIfAdminOrSuperAdmin) => Self::Admin,
            MembershipPolicies::Standard(BasePolicies::AllowIfSuperAdmin) => Self::SuperAdmin,
            _ => Self::Other,
        }
    }
}

impl From<&MetadataPolicies> for PermissionPolicy {
    fn from(value: &MetadataPolicies) -> Self {
        match value {
            MetadataPolicies::Standard(MetadataBasePolicies::Allow) => Self::Allow,
            MetadataPolicies::Standard(MetadataBasePolicies::Deny) => Self::Deny,
            MetadataPolicies::Standard(MetadataBasePolicies::AllowIfActorAdminOrSuperAdmin) => {
                Self::Admin
            }
            MetadataPolicies::Standard(MetadataBasePolicies::AllowIfActorSuperAdmin) => {
                Self::SuperAdmin
            }
            _ => Self::Other,
        }
    }
}

impl From<&PermissionsPolicies> for PermissionPolicy {
    fn from(value: &PermissionsPolicies) -> Self {
        match value {
            PermissionsPolicies::Standard(PermissionsBasePolicies::Deny) => Self::Deny,
            PermissionsPolicies::Standard(
                PermissionsBasePolicies::AllowIfActorAdminOrSuperAdmin,
            ) => Self::Admin,
            PermissionsPolicies::Standard(PermissionsBasePolicies::AllowIfActorSuperAdmin) => {
                Self::SuperAdmin
            }
            _ => Self::Other,
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct ConversationState {
    pub is_active: bool,
    pub consent_state: ConsentState,
    pub paused_for_version: Option<String>,
    pub is_disappearing_enabled: bool,
    pub disappearing_settings: Option<DisappearingSettings>,
    pub notifications_enabled: bool,
    pub commit_log_fork_status: CommitLogForkStatus,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct GroupState {
    pub common: ConversationState,
    pub name: String,
    pub image_url: String,
    pub description: String,
    pub app_data: String,
    pub membership_state: MembershipState,
    pub admins: Vec<InboxID>,
    pub super_admins: Vec<InboxID>,
    pub permissions: GroupPermissions,
}

impl ConversationState {
    pub(crate) fn from_snapshot(value: &ConversationStateSnapshot) -> Self {
        Self {
            is_active: value.is_active,
            consent_state: value.consent_state.into(),
            paused_for_version: value.paused_for_version.clone(),
            is_disappearing_enabled: value.is_disappearing_enabled,
            disappearing_settings: value.disappearing_settings.map(|settings| {
                DisappearingSettings {
                    from: Timestamp(settings.from_ns),
                    retention_ns: settings.in_ns,
                }
            }),
            notifications_enabled: value.notifications_enabled,
            commit_log_fork_status: match value.commit_log_fork_status {
                Some(true) => CommitLogForkStatus::Forked,
                Some(false) => CommitLogForkStatus::NotForked,
                None => CommitLogForkStatus::Unknown,
            },
        }
    }
}

impl GroupState {
    pub(crate) fn from_snapshot(value: ConversationStateSnapshot) -> Result<Self, XmtpError> {
        let common = ConversationState::from_snapshot(&value);
        let metadata = value
            .group
            .ok_or_else(|| XmtpError::invalid("not a group"))?;
        let inbox_ids = |values: Vec<String>| -> Result<Vec<InboxID>, XmtpError> {
            values.into_iter().map(InboxID::try_from).collect()
        };
        Ok(Self {
            common,
            name: metadata.name,
            image_url: metadata.image_url,
            description: metadata.description,
            app_data: metadata.app_data,
            membership_state: match metadata.membership_state {
                xmtp_db::group::GroupMembershipState::Allowed => MembershipState::Allowed,
                xmtp_db::group::GroupMembershipState::Rejected => MembershipState::Rejected,
                xmtp_db::group::GroupMembershipState::Pending => MembershipState::Pending,
                xmtp_db::group::GroupMembershipState::Restored => MembershipState::Restored,
                xmtp_db::group::GroupMembershipState::PendingRemove => {
                    MembershipState::PendingRemove
                }
            },
            admins: inbox_ids(metadata.admins)?,
            super_admins: inbox_ids(metadata.super_admins)?,
            permissions: {
                let policies = &metadata.permissions.policies;
                let metadata_policy = |field: MetadataField| {
                    policies
                        .update_metadata_policy
                        .get(field.as_str())
                        .map(PermissionPolicy::from)
                        .unwrap_or(PermissionPolicy::DoesNotExist)
                };
                GroupPermissions {
                    policy_type: match metadata.policy_type {
                        Some(xmtp_mls::groups::PreconfiguredPolicies::Default) => {
                            GroupPolicyType::AllMembers
                        }
                        Some(xmtp_mls::groups::PreconfiguredPolicies::AdminsOnly) => {
                            GroupPolicyType::AdminOnly
                        }
                        None => GroupPolicyType::Custom,
                    },
                    policy_set: PermissionPolicySet {
                        add_member: (&policies.add_member_policy).into(),
                        remove_member: (&policies.remove_member_policy).into(),
                        add_admin: (&policies.add_admin_policy).into(),
                        remove_admin: (&policies.remove_admin_policy).into(),
                        update_name: metadata_policy(MetadataField::GroupName),
                        update_description: metadata_policy(MetadataField::Description),
                        update_image: metadata_policy(MetadataField::GroupImageUrlSquare),
                        update_disappearing: {
                            // Disappearing-message policy spans two metadata
                            // fields (from and retention). Report the shared
                            // policy only when both agree; a mismatch means
                            // the group was left in a divergent state.
                            let from = metadata_policy(MetadataField::MessageDisappearFromNS);
                            let in_ns = metadata_policy(MetadataField::MessageDisappearInNS);
                            if from == in_ns {
                                from
                            } else {
                                PermissionPolicy::Other
                            }
                        },
                        update_app_data: metadata_policy(MetadataField::AppData),
                    },
                }
            },
        })
    }
}
