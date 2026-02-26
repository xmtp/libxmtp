use std::fmt::Display;

use openmls::{extensions::Extensions, group::GroupContext};
use prost::Message;
use serde::Serialize;
use thiserror::Error;
use xmtp_common::ErrorCode;

use xmtp_id::InboxId;
use xmtp_proto::xmtp::mls::message_contents::{
    ConversationType as ConversationTypeProto, DmMembers as DmMembersProto,
    GroupMetadataV1 as GroupMetadataProto, Inbox as InboxProto, OneshotMessage,
};

use xmtp_db::group::ConversationType;

#[derive(Debug, Error, ErrorCode)]
pub enum GroupMetadataError {
    #[error("serialization: {0}")]
    Serialization(#[from] prost::EncodeError),
    #[error("deserialization: {0}")]
    Deserialization(#[from] prost::DecodeError),
    #[error("invalid conversation type")]
    InvalidConversationType,
    #[error("missing extension")]
    MissingExtension,
    #[error("invalid dm members")]
    InvalidDmMembers,
    #[error("missing a dm member")]
    MissingDmMember,
    #[error(transparent)]
    #[error_code(inherit)]
    Conversion(#[from] xmtp_proto::ConversionError),
}

/// `GroupMetadata` is immutable and created at the time of group creation.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupMetadata {
    pub conversation_type: ConversationType,
    // TODO: Remove this once transition is completed
    pub creator_inbox_id: String,
    pub dm_members: Option<DmMembers<InboxId>>,
    pub oneshot_message: Option<OneshotMessage>,
}

impl GroupMetadata {
    pub fn new(
        conversation_type: ConversationType,
        creator_inbox_id: String,
        dm_members: Option<DmMembers<InboxId>>,
        oneshot_message: Option<OneshotMessage>,
    ) -> Self {
        Self {
            conversation_type,
            creator_inbox_id,
            dm_members,
            oneshot_message,
        }
    }
}

impl TryFrom<GroupMetadata> for Vec<u8> {
    type Error = GroupMetadataError;

    fn try_from(value: GroupMetadata) -> Result<Self, Self::Error> {
        let conversation_type: ConversationTypeProto = value.conversation_type.into();
        let proto_val = GroupMetadataProto {
            conversation_type: conversation_type as i32,
            creator_inbox_id: value.creator_inbox_id.clone(),
            creator_account_address: "".to_string(), // TODO: remove from proto
            dm_members: value.dm_members.clone().map(|dm| dm.into()),
            oneshot_message: value.oneshot_message,
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
        let dm_members = value.dm_members.map(DmMembers::try_from).transpose()?;
        Ok(Self::new(
            value.conversation_type.try_into()?,
            value.creator_inbox_id,
            dm_members,
            value.oneshot_message,
        ))
    }
}

impl TryFrom<&Extensions<GroupContext>> for GroupMetadata {
    type Error = GroupMetadataError;

    fn try_from(extensions: &Extensions<GroupContext>) -> Result<Self, Self::Error> {
        let data = extensions
            .immutable_metadata()
            .ok_or(GroupMetadataError::MissingExtension)?;
        data.metadata().try_into()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DmMembers<Id: AsRef<str>> {
    pub member_one_inbox_id: Id,
    pub member_two_inbox_id: Id,
}

impl<'a> DmMembers<String> {
    pub fn as_ref(&'a self) -> DmMembers<&'a str> {
        DmMembers {
            member_one_inbox_id: &*self.member_one_inbox_id,
            member_two_inbox_id: &*self.member_two_inbox_id,
        }
    }
}

impl<Id> From<DmMembers<Id>> for DmMembersProto
where
    Id: AsRef<str>,
{
    fn from(value: DmMembers<Id>) -> Self {
        DmMembersProto {
            dm_member_one: Some(InboxProto {
                inbox_id: value.member_one_inbox_id.as_ref().to_string(),
            }),
            dm_member_two: Some(InboxProto {
                inbox_id: value.member_two_inbox_id.as_ref().to_string(),
            }),
        }
    }
}

impl<Id> From<&DmMembers<Id>> for String
where
    Id: AsRef<str>,
{
    fn from(members: &DmMembers<Id>) -> Self {
        members.to_string()
    }
}

impl<Id> From<DmMembers<Id>> for String
where
    Id: AsRef<str>,
{
    fn from(members: DmMembers<Id>) -> Self {
        members.to_string()
    }
}

impl<Id> Display for DmMembers<Id>
where
    Id: AsRef<str>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut inbox_ids = [
            self.member_one_inbox_id.as_ref(),
            self.member_two_inbox_id.as_ref(),
        ]
        .into_iter()
        .map(str::to_lowercase)
        .collect::<Vec<_>>();
        inbox_ids.sort();

        write!(f, "dm:{}", inbox_ids.join(":"))
    }
}

impl TryFrom<DmMembersProto> for DmMembers<InboxId> {
    type Error = GroupMetadataError;

    fn try_from(value: DmMembersProto) -> Result<Self, Self::Error> {
        Ok(Self {
            member_one_inbox_id: value
                .dm_member_one
                .ok_or(GroupMetadataError::MissingDmMember)?
                .inbox_id,
            member_two_inbox_id: value
                .dm_member_two
                .ok_or(GroupMetadataError::MissingDmMember)?
                .inbox_id,
        })
    }
}

pub fn extract_group_metadata(
    extensions: &Extensions<GroupContext>,
) -> Result<GroupMetadata, GroupMetadataError> {
    let extension = extensions
        .immutable_metadata()
        .ok_or(GroupMetadataError::MissingExtension)?;

    extension.metadata().try_into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[xmtp_common::test]
    fn test_dm_members_sort() {
        let members = DmMembers {
            member_one_inbox_id: "thats_me".to_string(),
            member_two_inbox_id: "some_wise_guy".to_string(),
        };

        let members2 = DmMembers {
            member_one_inbox_id: "some_wise_guy".to_string(),
            member_two_inbox_id: "thats_me".to_string(),
        };

        assert_eq!(members.to_string(), members2.to_string());
    }
}
