use openmls::{extensions::Extensions, group::MlsGroup as OpenMlsGroup};
use prost::Message;
use thiserror::Error;

use xmtp_proto::xmtp::mls::message_contents::{
    ConversationType as ConversationTypeProto, DmMembers as DmMembersProto,
    GroupMetadataV1 as GroupMetadataProto, Inbox as InboxProto,
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
    pub creator_inbox_id: String,
    pub dm_members: Option<DmMembers>,
}

impl GroupMetadata {
    pub fn new(
        conversation_type: ConversationType,
        creator_inbox_id: String,
        dm_members: Option<DmMembers>,
    ) -> Self {
        Self {
            conversation_type,
            creator_inbox_id,
            dm_members,
        }
    }
}

impl TryFrom<GroupMetadata> for Vec<u8> {
    type Error = GroupMetadataError;

    fn try_from(value: GroupMetadata) -> Result<Self, Self::Error> {
        let conversation_type: ConversationTypeProto = value.conversation_type.clone().into();
        let proto_val = GroupMetadataProto {
            conversation_type: conversation_type as i32,
            creator_inbox_id: value.creator_inbox_id.clone(),
            creator_account_address: "".to_string(), // TODO: remove from proto
            dm_members: value.dm_members.clone().map(|dm| dm.into()),
        };
        let mut buf: Vec<u8> = Vec::new();
        proto_val.encode(&mut buf)?;

        Ok(buf)
    }
}

impl TryFrom<&Vec<u8>> for GroupMetadata {
    type Error = GroupMetadataError;

    fn try_from(value: &Vec<u8>) -> Result<Self, Self::Error> {
        let proto_val = GroupMetadataProto::decode(value.as_slice())?;
        proto_val.try_into()
    }
}

impl TryFrom<GroupMetadataProto> for GroupMetadata {
    type Error = GroupMetadataError;

    fn try_from(value: GroupMetadataProto) -> Result<Self, Self::Error> {
        let dm_members = if value.dm_members.is_some() {
            Some(DmMembers::try_from(value.dm_members.unwrap())?)
        } else {
            None
        };
        Ok(Self::new(
            value.conversation_type.try_into()?,
            value.creator_inbox_id.clone(),
            dm_members,
        ))
    }
}

impl TryFrom<&Extensions> for GroupMetadata {
    type Error = GroupMetadataError;

    fn try_from(extensions: &Extensions) -> Result<Self, Self::Error> {
        let data = extensions
            .immutable_metadata()
            .ok_or(GroupMetadataError::MissingExtension)?;
        data.metadata().try_into()
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

#[derive(Debug, Clone, PartialEq)]
pub struct DmMembers {
    pub member_one_inbox_id: String,
    pub member_two_inbox_id: String,
}

impl From<DmMembers> for DmMembersProto {
    fn from(value: DmMembers) -> Self {
        DmMembersProto {
            dm_member_one: Some(InboxProto {
                inbox_id: value.member_one_inbox_id.clone(),
            }),
            dm_member_two: Some(InboxProto {
                inbox_id: value.member_two_inbox_id.clone(),
            }),
        }
    }
}

impl TryFrom<DmMembersProto> for DmMembers {
    type Error = GroupMetadataError;

    fn try_from(value: DmMembersProto) -> Result<Self, Self::Error> {
        Ok(Self {
            member_one_inbox_id: value.dm_member_one.unwrap().inbox_id.clone(),
            member_two_inbox_id: value.dm_member_two.unwrap().inbox_id.clone(),
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
