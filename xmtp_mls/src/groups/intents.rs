use std::collections::HashMap;

use openmls::prelude::{
    tls_codec::{Error as TlsCodecError, Serialize},
    MlsMessageOut,
};
use prost::{bytes::Bytes, DecodeError, Message};
use thiserror::Error;

use xmtp_proto::xmtp::mls::database::{
    add_members_data::{Version as AddMembersVersion, V1 as AddMembersV1},
    addresses_or_installation_ids::AddressesOrInstallationIds as AddressesOrInstallationIdsProto,
    post_commit_action::{
        Installation as InstallationProto, Kind as PostCommitActionKind,
        SendWelcomes as SendWelcomesProto,
    },
    remove_members_data::{Version as RemoveMembersVersion, V1 as RemoveMembersV1},
    send_message_data::{Version as SendMessageVersion, V1 as SendMessageV1},
    update_group_membership_data::{
        Version as UpdateGroupMembershipVersion, V1 as UpdateGroupMembershipV1,
    },
    update_metadata_data::{Version as UpdateMetadataVersion, V1 as UpdateMetadataV1},
    AccountAddresses, AddMembersData,
    AddressesOrInstallationIds as AddressesOrInstallationIdsProtoWrapper, InstallationIds,
    PostCommitAction as PostCommitActionProto, RemoveMembersData, SendMessageData,
    UpdateGroupMembershipData, UpdateMetadataData,
};

use crate::{
    types::Address,
    verified_key_package::{KeyPackageVerificationError, VerifiedKeyPackage},
};

use super::group_mutable_metadata::MetadataField;

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
pub struct AddMembersIntentData {
    pub address_or_id: AddressesOrInstallationIds,
}

impl AddMembersIntentData {
    pub fn new(address_or_id: AddressesOrInstallationIds) -> Self {
        Self { address_or_id }
    }

    pub(crate) fn to_bytes(&self) -> Result<Vec<u8>, IntentError> {
        let mut buf = Vec::new();
        AddMembersData {
            version: Some(AddMembersVersion::V1(AddMembersV1 {
                addresses_or_installation_ids: Some(self.address_or_id.clone().into()),
            })),
        }
        .encode(&mut buf)
        .expect("encode error");

        Ok(buf)
    }

    pub(crate) fn from_bytes(data: &[u8]) -> Result<Self, IntentError> {
        let msg = AddMembersData::decode(data)?;
        let address_or_id = match msg.version {
            Some(AddMembersVersion::V1(v1)) => v1
                .addresses_or_installation_ids
                .ok_or(IntentError::Generic("missing payload".to_string()))?,
            None => return Err(IntentError::Generic("missing payload".to_string())),
        };

        Ok(Self::new(address_or_id.try_into()?))
    }
}

impl TryFrom<AddMembersIntentData> for Vec<u8> {
    type Error = IntentError;

    fn try_from(intent: AddMembersIntentData) -> Result<Self, Self::Error> {
        intent.to_bytes()
    }
}

#[derive(Debug, Clone)]
pub struct RemoveMembersIntentData {
    pub address_or_id: AddressesOrInstallationIds,
}

impl RemoveMembersIntentData {
    pub fn new(address_or_id: AddressesOrInstallationIds) -> Self {
        Self { address_or_id }
    }

    pub(crate) fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();

        RemoveMembersData {
            version: Some(RemoveMembersVersion::V1(RemoveMembersV1 {
                addresses_or_installation_ids: Some(self.address_or_id.clone().into()),
            })),
        }
        .encode(&mut buf)
        .expect("encode error");

        buf
    }

    pub(crate) fn from_bytes(data: &[u8]) -> Result<Self, IntentError> {
        let msg = RemoveMembersData::decode(data)?;
        let address_or_id = match msg.version {
            Some(RemoveMembersVersion::V1(v1)) => v1
                .addresses_or_installation_ids
                .ok_or(IntentError::Generic("missing payload".to_string()))?,
            None => return Err(IntentError::Generic("missing payload".to_string())),
        };

        Ok(Self::new(address_or_id.try_into()?))
    }
}

impl From<RemoveMembersIntentData> for Vec<u8> {
    fn from(intent: RemoveMembersIntentData) -> Self {
        intent.to_bytes()
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
    pub fn from_verified_key_package(key_package: &VerifiedKeyPackage) -> Self {
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
    use xmtp_cryptography::utils::generate_local_wallet;

    use super::*;
    use crate::InboxOwner;

    #[test]
    fn test_serialize_send_message() {
        let message = vec![1, 2, 3];
        let intent = SendMessageIntentData::new(message.clone());
        let as_bytes: Vec<u8> = intent.into();
        let restored_intent = SendMessageIntentData::from_bytes(as_bytes.as_slice()).unwrap();

        assert_eq!(restored_intent.message, message);
    }

    #[tokio::test]
    async fn test_serialize_add_members() {
        let wallet = generate_local_wallet();
        let account_address = wallet.get_address();

        let intent = AddMembersIntentData::new(vec![account_address.clone()].into());
        let as_bytes: Vec<u8> = intent.clone().try_into().unwrap();
        let restored_intent = AddMembersIntentData::from_bytes(as_bytes.as_slice()).unwrap();

        assert_eq!(intent.address_or_id, restored_intent.address_or_id);
    }

    #[tokio::test]
    async fn test_serialize_update_metadata() {
        let intent = UpdateMetadataIntentData::new_update_group_name("group name".to_string());
        let as_bytes: Vec<u8> = intent.clone().try_into().unwrap();
        let restored_intent: UpdateMetadataIntentData =
            UpdateMetadataIntentData::try_from(as_bytes).unwrap();

        assert_eq!(intent.field_value, restored_intent.field_value);
    }
}
