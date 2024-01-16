use openmls::group::MlsGroup as OpenMlsGroup;
use prost::Message;
use thiserror::Error;
use xmtp_proto::xmtp::mls::message_contents::{
    ConversationType as ConversationTypeProto, GroupMetadataV1 as GroupMetadataProto,
};

use super::group_permissions::{PolicyError, PolicySet};

#[derive(Debug, Error)]
pub enum GroupMetadataError {
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
}

#[derive(Debug, Clone, PartialEq)]
pub struct GroupMetadata {
    pub conversation_type: ConversationType,
    pub creator_account_address: String,
    pub policies: PolicySet,
}

impl GroupMetadata {
    pub fn new(
        conversation_type: ConversationType,
        creator_account_address: String,
        policies: PolicySet,
    ) -> Self {
        Self {
            conversation_type,
            creator_account_address,
            policies,
        }
    }

    pub(crate) fn from_proto(proto: GroupMetadataProto) -> Result<Self, GroupMetadataError> {
        if proto.policies.is_none() {
            return Err(GroupMetadataError::MissingPolicies);
        }
        let policies = proto.policies.unwrap();

        Ok(Self::new(
            proto.conversation_type.try_into()?,
            proto.creator_account_address.clone(),
            PolicySet::from_proto(policies)?,
        ))
    }

    pub(crate) fn to_proto(&self) -> Result<GroupMetadataProto, GroupMetadataError> {
        let conversation_type: ConversationTypeProto = self.conversation_type.clone().into();
        Ok(GroupMetadataProto {
            conversation_type: conversation_type as i32,
            creator_account_address: self.creator_account_address.clone(),
            policies: Some(self.policies.to_proto()?),
        })
    }
}

impl TryFrom<GroupMetadata> for Vec<u8> {
    type Error = GroupMetadataError;

    fn try_from(value: GroupMetadata) -> Result<Self, Self::Error> {
        let mut buf = Vec::new();
        let proto_val = value.to_proto()?;
        proto_val.encode(&mut buf)?;

        Ok(buf)
    }
}

impl TryFrom<&[u8]> for GroupMetadata {
    type Error = GroupMetadataError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let proto_val = GroupMetadataProto::decode(value)?;
        Self::from_proto(proto_val)
    }
}

impl TryFrom<GroupMetadataProto> for GroupMetadata {
    type Error = GroupMetadataError;

    fn try_from(value: GroupMetadataProto) -> Result<Self, Self::Error> {
        Self::from_proto(value)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConversationType {
    Group,
    Dm,
}

impl From<ConversationType> for ConversationTypeProto {
    fn from(value: ConversationType) -> Self {
        match value {
            ConversationType::Group => Self::Group,
            ConversationType::Dm => Self::Dm,
        }
    }
}

impl TryFrom<i32> for ConversationType {
    type Error = GroupMetadataError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        Ok(match value {
            1 => Self::Group,
            2 => Self::Dm,
            _ => return Err(GroupMetadataError::InvalidConversationType),
        })
    }
}

pub fn extract_group_metadata(group: &OpenMlsGroup) -> Result<GroupMetadata, GroupMetadataError> {
    let extension = group
        .export_group_context()
        .extensions()
        .protected_metadata()
        .ok_or(GroupMetadataError::MissingExtension)?;

    extension.metadata().try_into()
}
