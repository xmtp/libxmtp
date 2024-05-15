use openmls::group::MlsGroup as OpenMlsGroup;
use prost::Message;
use thiserror::Error;

use xmtp_proto::xmtp::mls::message_contents::{
    ConversationType as ConversationTypeProto, GroupMetadataV1 as GroupMetadataProto,
};

#[derive(Debug, Error)]
pub enum GroupMetadataError {
    #[error("serialization: {0}")]
    Serialization(#[from] prost::EncodeError),
    #[error("deserialization: {0}")]
    Deserialization(#[from] prost::DecodeError),
    #[error("invalid conversation type")]
    InvalidConversationType,
    #[error("missing extension")]
    MissingExtension,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GroupMetadata {
    pub conversation_type: ConversationType,
    // TODO: Remove this once transition is completed
    pub creator_account_address: String,
    pub creator_inbox_id: String,
}

impl GroupMetadata {
    pub fn new(
        conversation_type: ConversationType,
        // TODO: Remove this once transition is completed
        creator_account_address: String,
        creator_inbox_id: String,
    ) -> Self {
        Self {
            conversation_type,
            creator_account_address,
            creator_inbox_id,
        }
    }

    pub(crate) fn from_proto(proto: GroupMetadataProto) -> Result<Self, GroupMetadataError> {
        Ok(Self::new(
            proto.conversation_type.try_into()?,
            proto.creator_account_address.clone(),
            proto.creator_inbox_id.clone(),
        ))
    }

    pub(crate) fn to_proto(&self) -> Result<GroupMetadataProto, GroupMetadataError> {
        let conversation_type: ConversationTypeProto = self.conversation_type.clone().into();
        Ok(GroupMetadataProto {
            conversation_type: conversation_type as i32,
            creator_inbox_id: self.creator_inbox_id.clone(),
            creator_account_address: self.creator_account_address.clone(),
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

impl TryFrom<&Vec<u8>> for GroupMetadata {
    type Error = GroupMetadataError;

    fn try_from(value: &Vec<u8>) -> Result<Self, Self::Error> {
        let proto_val = GroupMetadataProto::decode(value.as_slice())?;
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
    Sync,
}

impl From<ConversationType> for ConversationTypeProto {
    fn from(value: ConversationType) -> Self {
        match value {
            ConversationType::Group => Self::Group,
            ConversationType::Dm => Self::Dm,
            ConversationType::Sync => Self::Sync,
        }
    }
}

impl TryFrom<i32> for ConversationType {
    type Error = GroupMetadataError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        Ok(match value {
            1 => Self::Group,
            2 => Self::Dm,
            3 => Self::Sync,
            _ => return Err(GroupMetadataError::InvalidConversationType),
        })
    }
}

pub fn extract_group_metadata(group: &OpenMlsGroup) -> Result<GroupMetadata, GroupMetadataError> {
    let extension = group
        .export_group_context()
        .extensions()
        .immutable_metadata()
        .ok_or(GroupMetadataError::MissingExtension)?;

    extension.metadata().try_into()
}
