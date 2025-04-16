use crate::configuration::GROUP_PERMISSIONS_EXTENSION_ID;
use crate::groups::group_permissions::GroupMutablePermissions;
use crate::groups::group_permissions::PolicySet;
use crate::groups::mls_ext::MlsGroupExt;
use crate::groups::mls_ext::PublishIntentData;
use crate::GroupError;

use openmls::group::MlsGroup;
use openmls::prelude::Extension;
use openmls::prelude::Extensions;
use openmls::prelude::UnknownExtension;
use prost::{bytes::Bytes, Message};
use tls_codec::Serialize;
use xmtp_proto::xmtp::mls::database::{
    update_permission_data::{self, Version as UpdatePermissionVersion, V1 as UpdatePermissionV1},
    UpdatePermissionData,
};

use super::IntentError;
use crate::groups::{
    group_permissions::{MembershipPolicies, MetadataPolicies, PermissionsPolicies},
    mls_ext::GroupIntent,
};

#[repr(i32)]
#[derive(Debug, Clone, PartialEq)]
pub enum PermissionUpdateType {
    AddMember = 1,      // Matches ADD_MEMBER in Protobuf
    RemoveMember = 2,   // Matches REMOVE_MEMBER in Protobuf
    AddAdmin = 3,       // Matches ADD_ADMIN in Protobuf
    RemoveAdmin = 4,    // Matches REMOVE_ADMIN in Protobuf
    UpdateMetadata = 5, // Matches UPDATE_METADATA in Protobuf
}

#[repr(i32)]
#[derive(Debug, Clone, PartialEq)]
pub enum PermissionPolicyOption {
    Allow = 1,          // Matches ADD_MEMBER in Protobuf
    Deny = 2,           // Matches REMOVE_MEMBER in Protobuf
    AdminOnly = 3,      // Matches ADD_ADMIN in Protobuf
    SuperAdminOnly = 4, // Matches REMOVE_ADMIN in Protobuf
}

#[derive(Debug, Clone)]
pub struct UpdatePermissionIntentData {
    pub update_type: PermissionUpdateType,
    pub policy_option: PermissionPolicyOption,
    pub metadata_field_name: Option<String>,
}

impl UpdatePermissionIntentData {
    pub fn new(
        update_type: PermissionUpdateType,
        policy_option: PermissionPolicyOption,
        metadata_field_name: Option<String>,
    ) -> Self {
        Self {
            update_type,
            policy_option,
            metadata_field_name,
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl GroupIntent for UpdatePermissionIntentData {
    async fn publish_data(
        self: Box<Self>,
        provider: &xmtp_db::XmtpOpenMlsProvider,
        context: &crate::client::XmtpMlsLocalContext,
        group: &mut MlsGroup,
        should_push: bool,
    ) -> Result<Option<crate::groups::mls_ext::PublishIntentData>, crate::groups::GroupError> {
        let extensions = self.build_extensions(group)?;
        let (commit, _, _) = group.update_group_context_extensions(
            provider,
            extensions,
            &context.identity.installation_keys,
        )?;
        let commit_bytes = commit.tls_serialize_detached()?;

        PublishIntentData::builder()
            .payload(commit_bytes)
            .staged_commit(group.get_and_clear_pending_commit(provider)?)
            .should_push(should_push)
            .build()
            .map_err(GroupError::from)
            .map(Option::Some)
    }

    fn build_extensions(&self, group: &MlsGroup) -> Result<Extensions, GroupError> {
        let existing_policy_set: GroupMutablePermissions = group.try_into()?;
        let existing_policy_set = existing_policy_set.policies;
        let new_policy_set = match self.update_type {
            PermissionUpdateType::AddMember => PolicySet {
                add_member_policy: self.policy_option.clone().into(),
                ..existing_policy_set
            },
            PermissionUpdateType::RemoveMember => PolicySet {
                remove_member_policy: self.policy_option.clone().into(),
                ..existing_policy_set
            },
            PermissionUpdateType::AddAdmin => PolicySet {
                add_admin_policy: self.policy_option.clone().into(),
                ..existing_policy_set
            },
            PermissionUpdateType::RemoveAdmin => PolicySet {
                remove_admin_policy: self.policy_option.clone().into(),
                ..existing_policy_set
            },
            PermissionUpdateType::UpdateMetadata => {
                let mut metadata_policy = existing_policy_set.update_metadata_policy.clone();
                metadata_policy.insert(
                    self.metadata_field_name
                        .clone()
                        .ok_or(GroupError::MissingMetadataField {
                            name: "metadata_field_name".into(),
                        })?,
                    self.policy_option.clone().into(),
                );
                PolicySet {
                    update_metadata_policy: metadata_policy,
                    ..existing_policy_set
                }
            }
        };
        let new_group_permissions: Vec<u8> =
            GroupMutablePermissions::new(new_policy_set).try_into()?;
        let unknown_gc_extension = UnknownExtension(new_group_permissions);
        let extension = Extension::Unknown(GROUP_PERMISSIONS_EXTENSION_ID, unknown_gc_extension);
        let mut extensions = group.extensions().clone();
        extensions.add_or_replace(extension);
        Ok(extensions)
    }
}

impl From<UpdatePermissionIntentData> for Vec<u8> {
    fn from(intent: UpdatePermissionIntentData) -> Self {
        let mut buf = Vec::new();
        let update_type = intent.update_type as i32;
        let policy_option = intent.policy_option as i32;

        UpdatePermissionData {
            version: Some(UpdatePermissionVersion::V1(UpdatePermissionV1 {
                permission_update_type: update_type,
                permission_policy_option: policy_option,
                metadata_field_name: intent.metadata_field_name,
            })),
        }
        .encode(&mut buf)
        .expect("encode error");

        buf
    }
}

impl TryFrom<Vec<u8>> for UpdatePermissionIntentData {
    type Error = IntentError;

    fn try_from(data: Vec<u8>) -> Result<Self, Self::Error> {
        let msg = UpdatePermissionData::decode(Bytes::from(data))?;
        let Some(UpdatePermissionVersion::V1(update_permission_data::V1 {
            permission_update_type,
            permission_policy_option,
            metadata_field_name,
        })) = msg.version
        else {
            return Err(IntentError::UnsupportedPermissionVersion);
        };
        let update_type: PermissionUpdateType = permission_update_type.try_into()?;
        let policy_option: PermissionPolicyOption = permission_policy_option.try_into()?;
        Ok(Self::new(update_type, policy_option, metadata_field_name))
    }
}

impl TryFrom<i32> for PermissionUpdateType {
    type Error = IntentError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(PermissionUpdateType::AddMember),
            2 => Ok(PermissionUpdateType::RemoveMember),
            3 => Ok(PermissionUpdateType::AddAdmin),
            4 => Ok(PermissionUpdateType::RemoveAdmin),
            5 => Ok(PermissionUpdateType::UpdateMetadata),
            _ => Err(IntentError::UnknownPermissionUpdateType),
        }
    }
}

impl TryFrom<i32> for PermissionPolicyOption {
    type Error = IntentError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(PermissionPolicyOption::Allow),
            2 => Ok(PermissionPolicyOption::Deny),
            3 => Ok(PermissionPolicyOption::AdminOnly),
            4 => Ok(PermissionPolicyOption::SuperAdminOnly),
            _ => Err(IntentError::UnknownPermissionPolicyOption),
        }
    }
}

impl From<PermissionPolicyOption> for MembershipPolicies {
    fn from(value: PermissionPolicyOption) -> Self {
        match value {
            PermissionPolicyOption::Allow => MembershipPolicies::allow(),
            PermissionPolicyOption::Deny => MembershipPolicies::deny(),
            PermissionPolicyOption::AdminOnly => MembershipPolicies::allow_if_actor_admin(),
            PermissionPolicyOption::SuperAdminOnly => {
                MembershipPolicies::allow_if_actor_super_admin()
            }
        }
    }
}

impl From<PermissionPolicyOption> for MetadataPolicies {
    fn from(value: PermissionPolicyOption) -> Self {
        match value {
            PermissionPolicyOption::Allow => MetadataPolicies::allow(),
            PermissionPolicyOption::Deny => MetadataPolicies::deny(),
            PermissionPolicyOption::AdminOnly => MetadataPolicies::allow_if_actor_admin(),
            PermissionPolicyOption::SuperAdminOnly => {
                MetadataPolicies::allow_if_actor_super_admin()
            }
        }
    }
}

impl From<PermissionPolicyOption> for PermissionsPolicies {
    fn from(value: PermissionPolicyOption) -> Self {
        match value {
            PermissionPolicyOption::Allow => {
                tracing::error!("PermissionPolicyOption::Allow is not allowed for PermissionsPolicies, set to super_admin only instead");
                PermissionsPolicies::allow_if_actor_super_admin()
            }
            PermissionPolicyOption::Deny => PermissionsPolicies::deny(),
            PermissionPolicyOption::AdminOnly => PermissionsPolicies::allow_if_actor_admin(),
            PermissionPolicyOption::SuperAdminOnly => {
                PermissionsPolicies::allow_if_actor_super_admin()
            }
        }
    }
}
