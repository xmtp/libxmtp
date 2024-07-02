use std::collections::HashMap;

use openmls::prelude::{
    tls_codec::{Error as TlsCodecError, Serialize},
    MlsMessageOut,
};
use prost::{bytes::Bytes, DecodeError, Message};
use thiserror::Error;

use xmtp_proto::xmtp::mls::database::{
    addresses_or_installation_ids::AddressesOrInstallationIds as AddressesOrInstallationIdsProto,
    post_commit_action::{
        Installation as InstallationProto, Kind as PostCommitActionKind,
        SendWelcomes as SendWelcomesProto,
    },
    send_message_data::{Version as SendMessageVersion, V1 as SendMessageV1},
    update_admin_lists_data::{Version as UpdateAdminListsVersion, V1 as UpdateAdminListsV1},
    update_group_membership_data::{
        Version as UpdateGroupMembershipVersion, V1 as UpdateGroupMembershipV1,
    },
    update_metadata_data::{Version as UpdateMetadataVersion, V1 as UpdateMetadataV1},
    update_permission_data::{Version as UpdatePermissionVersion, V1 as UpdatePermissionV1},
    AccountAddresses, AddressesOrInstallationIds as AddressesOrInstallationIdsProtoWrapper,
    InstallationIds, PostCommitAction as PostCommitActionProto, SendMessageData,
    UpdateAdminListsData, UpdateGroupMembershipData, UpdateMetadataData, UpdatePermissionData,
};

use crate::{
    types::Address,
    verified_key_package_v2::{KeyPackageVerificationError, VerifiedKeyPackageV2},
};

use super::{
    group_membership::GroupMembership,
    group_mutable_metadata::MetadataField,
    group_permissions::{MembershipPolicies, MetadataPolicies, PermissionsPolicies},
};

#[derive(Debug, Error)]
pub enum IntentError {
    #[error("decode error: {0}")]
    Decode(#[from] DecodeError),
    #[error("key package verification: {0}")]
    KeyPackageVerification(#[from] KeyPackageVerificationError),
    #[error("TLS Codec error: {0}")]
    TlsError(#[from] TlsCodecError),
    #[error("generic: {0}")]
    Generic(String),
}

#[derive(Debug, Clone)]
pub struct SendMessageIntentData {
    pub message: Vec<u8>,
}

impl SendMessageIntentData {
    pub fn new(message: Vec<u8>) -> Self {
        Self { message }
    }

    pub(crate) fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        SendMessageData {
            version: Some(SendMessageVersion::V1(SendMessageV1 {
                payload_bytes: self.message.clone(),
            })),
        }
        .encode(&mut buf)
        .unwrap();

        buf
    }

    pub(crate) fn from_bytes(data: &[u8]) -> Result<Self, IntentError> {
        let msg = SendMessageData::decode(data)?;
        let payload_bytes = match msg.version {
            Some(SendMessageVersion::V1(v1)) => v1.payload_bytes,
            None => return Err(IntentError::Generic("missing payload".to_string())),
        };

        Ok(Self::new(payload_bytes))
    }
}

impl From<SendMessageIntentData> for Vec<u8> {
    fn from(intent: SendMessageIntentData) -> Self {
        intent.to_bytes()
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum AddressesOrInstallationIds {
    AccountAddresses(Vec<String>),
    InstallationIds(Vec<Vec<u8>>),
}

impl From<AddressesOrInstallationIds> for AddressesOrInstallationIdsProtoWrapper {
    fn from(address_or_id: AddressesOrInstallationIds) -> Self {
        match address_or_id {
            AddressesOrInstallationIds::AccountAddresses(account_addresses) => {
                AddressesOrInstallationIdsProtoWrapper {
                    addresses_or_installation_ids: Some(
                        AddressesOrInstallationIdsProto::AccountAddresses(AccountAddresses {
                            account_addresses,
                        }),
                    ),
                }
            }
            AddressesOrInstallationIds::InstallationIds(installation_ids) => {
                AddressesOrInstallationIdsProtoWrapper {
                    addresses_or_installation_ids: Some(
                        AddressesOrInstallationIdsProto::InstallationIds(InstallationIds {
                            installation_ids,
                        }),
                    ),
                }
            }
        }
    }
}

impl TryFrom<AddressesOrInstallationIdsProtoWrapper> for AddressesOrInstallationIds {
    type Error = IntentError;

    fn try_from(wrapper: AddressesOrInstallationIdsProtoWrapper) -> Result<Self, Self::Error> {
        match wrapper.addresses_or_installation_ids {
            Some(AddressesOrInstallationIdsProto::AccountAddresses(addrs)) => Ok(
                AddressesOrInstallationIds::AccountAddresses(addrs.account_addresses),
            ),
            Some(AddressesOrInstallationIdsProto::InstallationIds(ids)) => Ok(
                AddressesOrInstallationIds::InstallationIds(ids.installation_ids),
            ),
            _ => Err(IntentError::Generic("missing payload".to_string())),
        }
    }
}

impl From<Vec<Address>> for AddressesOrInstallationIds {
    fn from(addrs: Vec<Address>) -> Self {
        AddressesOrInstallationIds::AccountAddresses(addrs)
    }
}

impl From<Vec<Vec<u8>>> for AddressesOrInstallationIds {
    fn from(installation_ids: Vec<Vec<u8>>) -> Self {
        AddressesOrInstallationIds::InstallationIds(installation_ids)
    }
}

#[derive(Debug, Clone)]
pub struct UpdateMetadataIntentData {
    pub field_name: String,
    pub field_value: String,
}

impl UpdateMetadataIntentData {
    pub fn new(field_name: String, field_value: String) -> Self {
        Self {
            field_name,
            field_value,
        }
    }

    pub fn new_update_group_name(group_name: String) -> Self {
        Self {
            field_name: MetadataField::GroupName.to_string(),
            field_value: group_name,
        }
    }

    pub fn new_update_group_image_url_square(group_image_url_square: String) -> Self {
        Self {
            field_name: MetadataField::GroupImageUrlSquare.to_string(),
            field_value: group_image_url_square,
        }
    }

    pub fn new_update_group_description(group_description: String) -> Self {
        Self {
            field_name: MetadataField::Description.to_string(),
            field_value: group_description,
        }
    }

    pub fn new_update_pinned_frame(pinned_frame: String) -> Self {
        Self {
            field_name: MetadataField::PinnedFrame.to_string(),
            field_value: pinned_frame,
        }
    }
}

impl From<UpdateMetadataIntentData> for Vec<u8> {
    fn from(intent: UpdateMetadataIntentData) -> Self {
        let mut buf = Vec::new();

        UpdateMetadataData {
            version: Some(UpdateMetadataVersion::V1(UpdateMetadataV1 {
                field_name: intent.field_name.to_string(),
                field_value: intent.field_value.clone(),
            })),
        }
        .encode(&mut buf)
        .expect("encode error");

        buf
    }
}

impl TryFrom<Vec<u8>> for UpdateMetadataIntentData {
    type Error = IntentError;

    fn try_from(data: Vec<u8>) -> Result<Self, Self::Error> {
        let msg = UpdateMetadataData::decode(Bytes::from(data))?;

        let field_name = match msg.version {
            Some(UpdateMetadataVersion::V1(ref v1)) => v1.field_name.clone(),
            None => return Err(IntentError::Generic("missing payload".to_string())),
        };
        let field_value = match msg.version {
            Some(UpdateMetadataVersion::V1(ref v1)) => v1.field_value.clone(),
            None => return Err(IntentError::Generic("missing payload".to_string())),
        };

        Ok(Self::new(field_name, field_value))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct UpdateGroupMembershipIntentData {
    pub membership_updates: HashMap<String, u64>,
    pub removed_members: Vec<String>,
}

impl UpdateGroupMembershipIntentData {
    pub fn new(membership_updates: HashMap<String, u64>, removed_members: Vec<String>) -> Self {
        Self {
            membership_updates,
            removed_members,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.membership_updates.is_empty() && self.removed_members.is_empty()
    }

    pub fn apply_to_group_membership(&self, group_membership: &GroupMembership) -> GroupMembership {
        log::info!("old group membership: {:?}", group_membership.members);
        let mut new_membership = group_membership.clone();
        for (inbox_id, sequence_id) in self.membership_updates.iter() {
            new_membership.add(inbox_id.clone(), *sequence_id);
        }

        for inbox_id in self.removed_members.iter() {
            new_membership.remove(inbox_id)
        }
        log::info!("updated group membership: {:?}", new_membership.members);
        new_membership
    }
}

impl From<UpdateGroupMembershipIntentData> for Vec<u8> {
    fn from(intent: UpdateGroupMembershipIntentData) -> Self {
        let mut buf = Vec::new();

        UpdateGroupMembershipData {
            version: Some(UpdateGroupMembershipVersion::V1(UpdateGroupMembershipV1 {
                membership_updates: intent.membership_updates,
                removed_members: intent.removed_members,
            })),
        }
        .encode(&mut buf)
        .expect("encode error");

        buf
    }
}

impl TryFrom<Vec<u8>> for UpdateGroupMembershipIntentData {
    type Error = IntentError;

    fn try_from(data: Vec<u8>) -> Result<Self, Self::Error> {
        if let UpdateGroupMembershipData {
            version: Some(UpdateGroupMembershipVersion::V1(v1)),
        } = UpdateGroupMembershipData::decode(data.as_slice())?
        {
            Ok(Self::new(v1.membership_updates, v1.removed_members))
        } else {
            Err(IntentError::Generic("missing payload".to_string()))
        }
    }
}

impl TryFrom<&Vec<u8>> for UpdateGroupMembershipIntentData {
    type Error = IntentError;

    fn try_from(data: &Vec<u8>) -> Result<Self, Self::Error> {
        if let UpdateGroupMembershipData {
            version: Some(UpdateGroupMembershipVersion::V1(v1)),
        } = UpdateGroupMembershipData::decode(data.as_slice())?
        {
            Ok(Self::new(v1.membership_updates, v1.removed_members))
        } else {
            Err(IntentError::Generic("missing payload".to_string()))
        }
    }
}

#[repr(i32)]
#[derive(Debug, Clone, PartialEq)]
pub enum AdminListActionType {
    Add = 1,         // Matches ADD_ADMIN in Protobuf
    Remove = 2,      // Matches REMOVE_ADMIN in Protobuf
    AddSuper = 3,    // Matches ADD_SUPER_ADMIN in Protobuf
    RemoveSuper = 4, // Matches REMOVE_SUPER_ADMIN in Protobuf
}

impl TryFrom<i32> for AdminListActionType {
    type Error = &'static str;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(AdminListActionType::Add),
            2 => Ok(AdminListActionType::Remove),
            3 => Ok(AdminListActionType::AddSuper),
            4 => Ok(AdminListActionType::RemoveSuper),
            _ => Err("Unknown value for AdminListActionType"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct UpdateAdminListIntentData {
    pub action_type: AdminListActionType,
    pub inbox_id: String,
}

impl UpdateAdminListIntentData {
    pub fn new(action_type: AdminListActionType, inbox_id: String) -> Self {
        Self {
            action_type,
            inbox_id,
        }
    }
}

impl From<UpdateAdminListIntentData> for Vec<u8> {
    fn from(intent: UpdateAdminListIntentData) -> Self {
        let mut buf = Vec::new();
        let action_type = intent.action_type as i32;

        UpdateAdminListsData {
            version: Some(UpdateAdminListsVersion::V1(UpdateAdminListsV1 {
                admin_list_update_type: action_type,
                inbox_id: intent.inbox_id,
            })),
        }
        .encode(&mut buf)
        .expect("encode error");

        buf
    }
}

impl TryFrom<Vec<u8>> for UpdateAdminListIntentData {
    type Error = IntentError;

    fn try_from(data: Vec<u8>) -> Result<Self, Self::Error> {
        let msg = UpdateAdminListsData::decode(Bytes::from(data))?;

        let action_type: AdminListActionType = match msg.version {
            Some(UpdateAdminListsVersion::V1(ref v1)) => {
                AdminListActionType::try_from(v1.admin_list_update_type)
                    .map_err(|e| IntentError::Generic(e.to_string()))?
            }
            None => {
                return Err(IntentError::Generic(
                    "missing update admin version".to_string(),
                ))
            }
        };
        let inbox_id = match msg.version {
            Some(UpdateAdminListsVersion::V1(ref v1)) => v1.inbox_id.clone(),
            None => {
                return Err(IntentError::Generic(
                    "missing update admin version".to_string(),
                ))
            }
        };

        Ok(Self::new(action_type, inbox_id))
    }
}

#[repr(i32)]
#[derive(Debug, Clone, PartialEq)]
pub enum PermissionUpdateType {
    AddMember = 1,      // Matches ADD_MEMBER in Protobuf
    RemoveMember = 2,   // Matches REMOVE_MEMBER in Protobuf
    AddAdmin = 3,       // Matches ADD_ADMIN in Protobuf
    RemoveAdmin = 4,    // Matches REMOVE_ADMIN in Protobuf
    UpdateMetadata = 5, // Matches UPDATE_METADATA in Protobuf
}

impl TryFrom<i32> for PermissionUpdateType {
    type Error = &'static str;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(PermissionUpdateType::AddMember),
            2 => Ok(PermissionUpdateType::RemoveMember),
            3 => Ok(PermissionUpdateType::AddAdmin),
            4 => Ok(PermissionUpdateType::RemoveAdmin),
            5 => Ok(PermissionUpdateType::UpdateMetadata),
            _ => Err("Unknown value for PermissionUpdateType"),
        }
    }
}

#[repr(i32)]
#[derive(Debug, Clone, PartialEq)]
pub enum PermissionPolicyOption {
    Allow = 1,          // Matches ADD_MEMBER in Protobuf
    Deny = 2,           // Matches REMOVE_MEMBER in Protobuf
    AdminOnly = 3,      // Matches ADD_ADMIN in Protobuf
    SuperAdminOnly = 4, // Matches REMOVE_ADMIN in Protobuf
}

impl TryFrom<i32> for PermissionPolicyOption {
    type Error = &'static str;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(PermissionPolicyOption::Allow),
            2 => Ok(PermissionPolicyOption::Deny),
            3 => Ok(PermissionPolicyOption::AdminOnly),
            4 => Ok(PermissionPolicyOption::SuperAdminOnly),
            _ => Err("Unknown value for PermissionPolicyOption"),
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
                log::error!("PermissionPolicyOption::Allow is not allowed for PermissionsPolicies, set to super_admin only instead");
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

        let update_type: PermissionUpdateType = match msg.version {
            Some(UpdatePermissionVersion::V1(ref v1)) => {
                PermissionUpdateType::try_from(v1.permission_update_type)
                    .map_err(|e| IntentError::Generic(e.to_string()))?
            }
            None => {
                return Err(IntentError::Generic(
                    "missing update permission version".to_string(),
                ))
            }
        };
        let policy_option: PermissionPolicyOption = match msg.version {
            Some(UpdatePermissionVersion::V1(ref v1)) => {
                PermissionPolicyOption::try_from(v1.permission_policy_option)
                    .map_err(|e| IntentError::Generic(e.to_string()))?
            }
            None => {
                return Err(IntentError::Generic(
                    "missing update permission version".to_string(),
                ))
            }
        };
        let metadata_field_name = match msg.version {
            Some(UpdatePermissionVersion::V1(ref v1)) => v1.metadata_field_name.clone(),
            None => None,
        };

        Ok(Self::new(update_type, policy_option, metadata_field_name))
    }
}

#[derive(Debug, Clone)]
pub enum PostCommitAction {
    SendWelcomes(SendWelcomesAction),
}

#[derive(Debug, Clone)]
pub struct Installation {
    pub(crate) installation_key: Vec<u8>,
    pub(crate) hpke_public_key: Vec<u8>,
}

impl Installation {
    pub fn from_verified_key_package(key_package: &VerifiedKeyPackageV2) -> Self {
        Self {
            installation_key: key_package.installation_id(),
            hpke_public_key: key_package.hpke_init_key(),
        }
    }
}

impl From<Installation> for InstallationProto {
    fn from(installation: Installation) -> Self {
        Self {
            installation_key: installation.installation_key,
            hpke_public_key: installation.hpke_public_key,
        }
    }
}

impl From<InstallationProto> for Installation {
    fn from(installation: InstallationProto) -> Self {
        Self {
            installation_key: installation.installation_key,
            hpke_public_key: installation.hpke_public_key,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SendWelcomesAction {
    pub installations: Vec<Installation>,
    pub welcome_message: Vec<u8>,
}

impl SendWelcomesAction {
    pub fn new(installations: Vec<Installation>, welcome_message: Vec<u8>) -> Self {
        Self {
            installations,
            welcome_message,
        }
    }

    pub(crate) fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        PostCommitActionProto {
            kind: Some(PostCommitActionKind::SendWelcomes(SendWelcomesProto {
                installations: self
                    .installations
                    .clone()
                    .into_iter()
                    .map(|i| i.into())
                    .collect(),
                welcome_message: self.welcome_message.clone(),
            })),
        }
        .encode(&mut buf)
        .unwrap();

        buf
    }
}

impl PostCommitAction {
    pub(crate) fn to_bytes(&self) -> Vec<u8> {
        match self {
            PostCommitAction::SendWelcomes(action) => action.to_bytes(),
        }
    }

    pub(crate) fn from_bytes(data: &[u8]) -> Result<Self, IntentError> {
        let decoded = PostCommitActionProto::decode(data)?;
        match decoded.kind {
            Some(PostCommitActionKind::SendWelcomes(proto)) => {
                Ok(Self::SendWelcomes(SendWelcomesAction::new(
                    proto.installations.into_iter().map(|i| i.into()).collect(),
                    proto.welcome_message,
                )))
            }
            None => Err(IntentError::Generic(
                "missing post commit action".to_string(),
            )),
        }
    }

    pub(crate) fn from_welcome(
        welcome: MlsMessageOut,
        installations: Vec<Installation>,
    ) -> Result<Self, IntentError> {
        let welcome_bytes = welcome.tls_serialize_detached()?;

        Ok(Self::SendWelcomes(SendWelcomesAction::new(
            installations,
            welcome_bytes,
        )))
    }
}

impl From<Vec<u8>> for PostCommitAction {
    fn from(data: Vec<u8>) -> Self {
        PostCommitAction::from_bytes(data.as_slice()).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_send_message() {
        let message = vec![1, 2, 3];
        let intent = SendMessageIntentData::new(message.clone());
        let as_bytes: Vec<u8> = intent.into();
        let restored_intent = SendMessageIntentData::from_bytes(as_bytes.as_slice()).unwrap();

        assert_eq!(restored_intent.message, message);
    }

    #[tokio::test]
    async fn test_serialize_update_membership() {
        let mut membership_updates = HashMap::new();
        membership_updates.insert("foo".to_string(), 123);

        let intent =
            UpdateGroupMembershipIntentData::new(membership_updates, vec!["bar".to_string()]);

        let as_bytes: Vec<u8> = intent.clone().into();
        let restored_intent: UpdateGroupMembershipIntentData = as_bytes.try_into().unwrap();

        assert_eq!(
            intent.membership_updates,
            restored_intent.membership_updates
        );

        assert_eq!(intent.removed_members, restored_intent.removed_members);
    }

    #[tokio::test]
    async fn test_serialize_update_metadata() {
        let intent = UpdateMetadataIntentData::new_update_group_name("group name".to_string());
        let as_bytes: Vec<u8> = intent.clone().into();
        let restored_intent: UpdateMetadataIntentData =
            UpdateMetadataIntentData::try_from(as_bytes).unwrap();

        assert_eq!(intent.field_value, restored_intent.field_value);
    }
}
