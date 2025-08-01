use super::{
    group_membership::GroupMembership,
    group_permissions::{MembershipPolicies, MetadataPolicies, PermissionsPolicies},
    mls_ext::{WrapperAlgorithm, WrapperEncryptionExtension},
};
use crate::{
    configuration::GROUP_KEY_ROTATION_INTERVAL_NS,
    verified_key_package_v2::{KeyPackageVerificationError, VerifiedKeyPackageV2},
};
use openmls::prelude::{
    MlsMessageOut,
    tls_codec::{Error as TlsCodecError, Serialize},
};
use prost::{DecodeError, Message, bytes::Bytes};
use std::collections::{HashMap, HashSet};
use thiserror::Error;
use xmtp_common::types::Address;
use xmtp_mls_common::group_mutable_metadata::MetadataField;
use xmtp_proto::xmtp::mls::database::{
    AccountAddresses, AddressesOrInstallationIds as AddressesOrInstallationIdsProtoWrapper,
    InstallationIds, PostCommitAction as PostCommitActionProto, SendMessageData,
    UpdateAdminListsData, UpdateGroupMembershipData, UpdateMetadataData, UpdatePermissionData,
    addresses_or_installation_ids::AddressesOrInstallationIds as AddressesOrInstallationIdsProto,
    post_commit_action::{
        Installation as InstallationProto, Kind as PostCommitActionKind,
        SendWelcomes as SendWelcomesProto,
    },
    send_message_data::{V1 as SendMessageV1, Version as SendMessageVersion},
    update_admin_lists_data::{V1 as UpdateAdminListsV1, Version as UpdateAdminListsVersion},
    update_group_membership_data::{
        V1 as UpdateGroupMembershipV1, Version as UpdateGroupMembershipVersion,
    },
    update_metadata_data::{V1 as UpdateMetadataV1, Version as UpdateMetadataVersion},
    update_permission_data::{self, V1 as UpdatePermissionV1, Version as UpdatePermissionVersion},
};

mod queue;
pub use queue::*;

#[derive(Debug, Error)]
pub enum IntentError {
    #[error("decode error: {0}")]
    Decode(#[from] DecodeError),
    #[error("key package verification: {0}")]
    KeyPackageVerification(#[from] KeyPackageVerificationError),
    #[error("TLS Codec error: {0}")]
    TlsError(#[from] TlsCodecError),
    #[error(transparent)]
    Storage(#[from] xmtp_db::StorageError),
    #[error("missing update permission")]
    MissingUpdatePermissionVersion,
    #[error("missing payload")]
    MissingPayload,
    #[error("missing update admin version")]
    MissingUpdateAdminVersion,
    #[error("missing post commit action")]
    MissingPostCommit,
    #[error("unsupported permission version")]
    UnsupportedPermissionVersion,
    #[error("unknown permission update type")]
    UnknownPermissionUpdateType,
    #[error("unknown value for PermissionPolicyOption")]
    UnknownPermissionPolicyOption,
    #[error("unknown value for AdminListActionType")]
    UnknownAdminListAction,
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
        SendMessageData {
            version: Some(SendMessageVersion::V1(SendMessageV1 {
                payload_bytes: self.message.clone(),
            })),
        }
        .encode_to_vec()
    }

    pub(crate) fn from_bytes(data: &[u8]) -> Result<Self, IntentError> {
        let msg = SendMessageData::decode(data)?;
        let payload_bytes = match msg.version {
            Some(SendMessageVersion::V1(v1)) => v1.payload_bytes,
            None => return Err(IntentError::MissingPayload),
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
            _ => Err(IntentError::MissingPayload),
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

    pub fn new_update_conversation_message_disappear_from_ns(from_ns: i64) -> Self {
        Self {
            field_name: MetadataField::MessageDisappearFromNS.to_string(),
            field_value: from_ns.to_string(),
        }
    }
    pub fn new_update_conversation_message_disappear_in_ns(in_ns: i64) -> Self {
        Self {
            field_name: MetadataField::MessageDisappearInNS.to_string(),
            field_value: in_ns.to_string(),
        }
    }

    pub fn new_update_group_min_version_to_match_self(min_version: String) -> Self {
        Self {
            field_name: MetadataField::MinimumSupportedProtocolVersion.to_string(),
            field_value: min_version,
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
            None => return Err(IntentError::MissingPayload),
        };
        let field_value = match msg.version {
            Some(UpdateMetadataVersion::V1(ref v1)) => v1.field_value.clone(),
            None => return Err(IntentError::MissingPayload),
        };

        Ok(Self::new(field_name, field_value))
    }
}

#[derive(Debug, Default, Clone)]
pub struct UpdateGroupMembershipResult {
    pub added_members: HashMap<String, u64>,
    pub removed_members: Vec<String>,
    pub failed_installations: Vec<Vec<u8>>,
}

impl UpdateGroupMembershipResult {
    pub fn new(
        added_members: HashMap<String, u64>,
        removed_members: Vec<String>,
        failed_installations: Vec<Vec<u8>>,
    ) -> Self {
        Self {
            added_members,
            removed_members,
            failed_installations,
        }
    }
}

impl From<UpdateGroupMembershipIntentData> for UpdateGroupMembershipResult {
    fn from(value: UpdateGroupMembershipIntentData) -> Self {
        UpdateGroupMembershipResult::new(
            value.membership_updates,
            value.removed_members,
            value.failed_installations,
        )
    }
}

#[derive(Debug, Clone)]
pub(crate) struct UpdateGroupMembershipIntentData {
    pub membership_updates: HashMap<String, u64>,
    pub removed_members: Vec<String>,
    pub failed_installations: Vec<Vec<u8>>,
}

impl UpdateGroupMembershipIntentData {
    pub fn new(
        membership_updates: HashMap<String, u64>,
        removed_members: Vec<String>,
        failed_installations: Vec<Vec<u8>>,
    ) -> Self {
        Self {
            membership_updates,
            removed_members,
            failed_installations,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.membership_updates.is_empty()
            && self.removed_members.is_empty()
            && self.failed_installations.is_empty()
    }

    pub fn apply_to_group_membership(&self, group_membership: &GroupMembership) -> GroupMembership {
        tracing::info!("old group membership: {:?}", group_membership.members);
        let mut new_membership = group_membership.clone();
        for (inbox_id, sequence_id) in self.membership_updates.iter() {
            new_membership.add(inbox_id.clone(), *sequence_id);
        }

        for inbox_id in self.removed_members.iter() {
            new_membership.remove(inbox_id)
        }

        new_membership.failed_installations = new_membership
            .failed_installations
            .into_iter()
            .chain(self.failed_installations.iter().cloned())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();

        tracing::info!("updated group membership: {:?}", new_membership.members);
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
                failed_installations: intent.failed_installations,
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
            Ok(Self::new(
                v1.membership_updates,
                v1.removed_members,
                v1.failed_installations,
            ))
        } else {
            Err(IntentError::MissingPayload)
        }
    }
}

impl<'a> TryFrom<&'a [u8]> for UpdateGroupMembershipIntentData {
    type Error = IntentError;

    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
        if let UpdateGroupMembershipData {
            version: Some(UpdateGroupMembershipVersion::V1(v1)),
        } = UpdateGroupMembershipData::decode(data)?
        {
            Ok(Self::new(
                v1.membership_updates,
                v1.removed_members,
                v1.failed_installations,
            ))
        } else {
            Err(IntentError::MissingPayload)
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
    type Error = IntentError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(AdminListActionType::Add),
            2 => Ok(AdminListActionType::Remove),
            3 => Ok(AdminListActionType::AddSuper),
            4 => Ok(AdminListActionType::RemoveSuper),
            _ => Err(IntentError::UnknownAdminListAction),
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
                AdminListActionType::try_from(v1.admin_list_update_type)?
            }
            None => return Err(IntentError::MissingUpdateAdminVersion),
        };
        let inbox_id = match msg.version {
            Some(UpdateAdminListsVersion::V1(ref v1)) => v1.inbox_id.clone(),
            None => return Err(IntentError::MissingUpdateAdminVersion),
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

#[repr(i32)]
#[derive(Debug, Clone, PartialEq)]
pub enum PermissionPolicyOption {
    Allow = 1,          // Matches ADD_MEMBER in Protobuf
    Deny = 2,           // Matches REMOVE_MEMBER in Protobuf
    AdminOnly = 3,      // Matches ADD_ADMIN in Protobuf
    SuperAdminOnly = 4, // Matches REMOVE_ADMIN in Protobuf
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
                tracing::error!(
                    "PermissionPolicyOption::Allow is not allowed for PermissionsPolicies, set to super_admin only instead"
                );
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

#[derive(Debug, Clone)]
pub enum PostCommitAction {
    SendWelcomes(SendWelcomesAction),
}

#[derive(Debug, Clone)]
pub struct Installation {
    pub(crate) installation_key: Vec<u8>,
    pub(crate) hpke_public_key: Vec<u8>,
    pub(crate) welcome_wrapper_algorithm: WrapperAlgorithm,
}

impl Installation {
    pub fn from_verified_key_package(
        key_package: &VerifiedKeyPackageV2,
    ) -> Result<Self, IntentError> {
        let wrapper_encryption = key_package.wrapper_encryption()?.unwrap_or_else(|| {
            // Default to using the hpke init key as the pub key and Curve25519 as the algorithm
            // if no extension is present. This means you are on an older key package
            WrapperEncryptionExtension::new(
                WrapperAlgorithm::Curve25519,
                key_package.hpke_init_key(),
            )
        });

        Ok(Self {
            installation_key: key_package.installation_id(),
            hpke_public_key: wrapper_encryption.pub_key_bytes,
            welcome_wrapper_algorithm: wrapper_encryption.algorithm,
        })
    }
}

impl From<Installation> for InstallationProto {
    fn from(installation: Installation) -> Self {
        Self {
            installation_key: installation.installation_key,
            hpke_public_key: installation.hpke_public_key,
            welcome_wrapper_algorithm: installation.welcome_wrapper_algorithm.into(),
        }
    }
}

impl From<InstallationProto> for Installation {
    fn from(installation: InstallationProto) -> Self {
        Self {
            installation_key: installation.installation_key,
            hpke_public_key: installation.hpke_public_key,
            welcome_wrapper_algorithm: installation.welcome_wrapper_algorithm.into(),
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
        .encode_to_vec()
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
            None => Err(IntentError::MissingPostCommit),
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

impl TryFrom<Vec<u8>> for PostCommitAction {
    type Error = IntentError;

    fn try_from(data: Vec<u8>) -> Result<Self, Self::Error> {
        PostCommitAction::from_bytes(data.as_slice())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);
    use crate::context::XmtpSharedContext;
    use openmls::prelude::{MlsMessageBodyIn, MlsMessageIn, ProcessedMessageContent};
    use tls_codec::Deserialize;
    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_db::XmtpOpenMlsProviderRef;

    use xmtp_proto::xmtp::mls::api::v1::{GroupMessage, group_message};

    use crate::{builder::ClientBuilder, utils::TestMlsGroup};

    use super::*;

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_serialize_send_message() {
        let message = vec![1, 2, 3];
        let intent = SendMessageIntentData::new(message.clone());
        let as_bytes: Vec<u8> = intent.into();
        let restored_intent = SendMessageIntentData::from_bytes(as_bytes.as_slice()).unwrap();

        assert_eq!(restored_intent.message, message);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_serialize_update_membership() {
        let mut membership_updates = HashMap::new();
        membership_updates.insert("foo".to_string(), 123);

        let intent = UpdateGroupMembershipIntentData::new(
            membership_updates,
            vec!["bar".to_string()],
            vec![vec![1, 2, 3]],
        );

        let as_bytes: Vec<u8> = intent.clone().into();
        let restored_intent: UpdateGroupMembershipIntentData = as_bytes.try_into().unwrap();

        assert_eq!(
            intent.membership_updates,
            restored_intent.membership_updates
        );

        assert_eq!(intent.removed_members, restored_intent.removed_members);

        assert_eq!(
            intent.failed_installations,
            restored_intent.failed_installations
        );
    }

    #[xmtp_common::test]
    async fn test_serialize_update_metadata() {
        let intent = UpdateMetadataIntentData::new_update_group_name("group name".to_string());
        let as_bytes: Vec<u8> = intent.clone().into();
        let restored_intent: UpdateMetadataIntentData =
            UpdateMetadataIntentData::try_from(as_bytes).unwrap();

        assert_eq!(intent.field_value, restored_intent.field_value);
    }

    #[xmtp_common::test]
    async fn test_key_rotation_before_first_message() {
        let client_a = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let client_b = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        // client A makes a group with client B, and then sends a message to client B.
        let group_a = client_a.create_group(None, None).expect("create group");
        group_a
            .add_members_by_inbox_id(&[client_b.inbox_id()])
            .await
            .unwrap();
        group_a.send_message(b"First message from A").await.unwrap();

        // No key rotation needed, because A's commit to add B already performs a rotation.
        // Group should have a commit to add client B, followed by A's message.
        verify_num_payloads_in_group(&group_a, 2).await;

        // Client B sends a message to Client A
        let groups_b = client_b.sync_welcomes().await.unwrap();
        assert_eq!(groups_b.len(), 1);
        let group_b = groups_b[0].clone();
        group_b
            .send_message(b"First message from B")
            .await
            .expect("send message");

        // B must perform a key rotation before sending their first message.
        // Group should have a commit to add B, A's message, B's key rotation and then B's message.
        let payloads_a = verify_num_payloads_in_group(&group_a, 4).await;
        let payloads_b = verify_num_payloads_in_group(&group_b, 4).await;

        // Verify key rotation payload
        for i in 0..payloads_a.len() {
            assert_eq!(payloads_a[i].encode_to_vec(), payloads_b[i].encode_to_vec());
        }
        verify_commit_updates_leaf_node(&group_a, &payloads_a[2]);

        // Client B sends another message to Client A, and Client A sends another message to Client B.
        group_b
            .send_message(b"Second message from B")
            .await
            .expect("send message");
        group_a
            .send_message(b"Second message from A")
            .await
            .expect("send message");

        // Group should only have 2 additional messages - no more key rotations needed.
        verify_num_payloads_in_group(&group_a, 6).await;
        verify_num_payloads_in_group(&group_b, 6).await;
    }

    async fn verify_num_payloads_in_group(
        group: &TestMlsGroup,
        num_messages: usize,
    ) -> Vec<GroupMessage> {
        let messages = group
            .context
            .api()
            .query_group_messages(group.group_id.clone(), None)
            .await
            .unwrap();
        assert_eq!(messages.len(), num_messages);
        messages
    }

    fn verify_commit_updates_leaf_node(group: &TestMlsGroup, payload: &GroupMessage) {
        let msgv1 = match &payload.version {
            Some(group_message::Version::V1(value)) => value,
            _ => panic!("error msgv1"),
        };

        let mls_message_in = MlsMessageIn::tls_deserialize_exact(&msgv1.data).unwrap();
        let mls_message = match mls_message_in.extract() {
            MlsMessageBodyIn::PrivateMessage(mls_message) => mls_message,
            _ => panic!("error mls_message"),
        };

        let storage = group.context.mls_storage();
        let decrypted_message = group
            .load_mls_group_with_lock(storage, |mut mls_group| {
                Ok(mls_group
                    .process_message(&XmtpOpenMlsProviderRef::new(storage), mls_message.clone())
                    .unwrap())
            })
            .unwrap();

        let staged_commit = match decrypted_message.into_content() {
            ProcessedMessageContent::StagedCommitMessage(staged_commit) => *staged_commit,
            _ => panic!("error staged_commit"),
        };

        // Check there is indeed some updated leaf node, which means the key update works.
        let path_update_leaf_node = staged_commit.update_path_leaf_node();
        assert!(path_update_leaf_node.is_some());
    }
}
