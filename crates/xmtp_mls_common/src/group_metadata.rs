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
    /// Serialization error.
    ///
    /// Failed to encode metadata protobuf. Not retryable.
    #[error("serialization: {0}")]
    Serialization(#[from] prost::EncodeError),
    /// Deserialization error.
    ///
    /// Failed to decode metadata protobuf. Not retryable.
    #[error("deserialization: {0}")]
    Deserialization(#[from] prost::DecodeError),
    /// Invalid conversation type.
    ///
    /// Protobuf conversation type not recognized. Not retryable.
    #[error("invalid conversation type")]
    InvalidConversationType,
    /// Missing extension.
    ///
    /// Immutable metadata MLS extension not found. Not retryable.
    #[error("missing extension")]
    MissingExtension,
    /// Invalid DM members.
    ///
    /// DM member data is invalid. Not retryable.
    #[error("invalid dm members")]
    InvalidDmMembers,
    /// Missing DM member.
    ///
    /// A DM member field is not set. Not retryable.
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

/// Extract `GroupMetadata` from a group context.
///
/// **Capability-aware.** On migrated groups (post-bootstrap, where the
/// AppData dictionary contains the canonical `COMPONENT_REGISTRY` entry)
/// the metadata is reconstructed from the dict's `CONVERSATION_TYPE`,
/// `CREATOR_INBOX_ID`, `DM_MEMBERS`, and `ONESHOT_MESSAGE` components.
/// On unmigrated groups it is read from the legacy `ImmutableMetadata`
/// MLS extension. Callers don't need to know which path applies.
pub fn extract_group_metadata(
    extensions: &Extensions<GroupContext>,
) -> Result<GroupMetadata, GroupMetadataError> {
    if let Some(metadata) = read_group_metadata_from_dict(extensions)? {
        return Ok(metadata);
    }

    let extension = extensions
        .immutable_metadata()
        .ok_or(GroupMetadataError::MissingExtension)?;

    extension.metadata().try_into()
}

/// Read `GroupMetadata` from the AppData dictionary on a migrated group.
///
/// Returns `Ok(None)` for unmigrated groups (no `COMPONENT_REGISTRY`
/// entry in the dict, or no AppData dictionary at all) so the caller
/// can fall back to the legacy `ImmutableMetadata` extension. Returns
/// `Err` only on a malformed dict entry on a group that *is* migrated.
fn read_group_metadata_from_dict(
    extensions: &Extensions<GroupContext>,
) -> Result<Option<GroupMetadata>, GroupMetadataError> {
    use crate::app_data::component_id::ComponentId;
    use crate::inbox_id::InboxId as DictInboxId;
    use crate::tls_set::TlsSet;
    use tls_codec::Deserialize;

    let Some(ext) = extensions.app_data_dictionary() else {
        return Ok(None);
    };
    let dict = ext.dictionary();

    // Use COMPONENT_REGISTRY presence as the post-bootstrap marker. A
    // pre-bootstrap group should never carry a stray dict entry that
    // shadows the legacy extension.
    if !dict.contains(&ComponentId::COMPONENT_REGISTRY.as_u16()) {
        return Ok(None);
    }

    // On a migrated group these two are required. Falling back to the
    // legacy `ImmutableMetadata` extension would mean trusting a stale
    // (or absent) value, so surface the malformed dict instead.
    let Some(ct_bytes) = dict.get(&ComponentId::CONVERSATION_TYPE.as_u16()) else {
        return Err(GroupMetadataError::Conversion(
            xmtp_proto::ConversionError::Missing {
                item: "CONVERSATION_TYPE",
                r#type: "AppData dictionary entry",
            },
        ));
    };
    let Some(creator_bytes) = dict.get(&ComponentId::CREATOR_INBOX_ID.as_u16()) else {
        return Err(GroupMetadataError::Conversion(
            xmtp_proto::ConversionError::Missing {
                item: "CREATOR_INBOX_ID",
                r#type: "AppData dictionary entry",
            },
        ));
    };

    // CONVERSATION_TYPE: 4-byte big-endian i32 matching `ConversationTypeProto`.
    let ct_arr: [u8; 4] = ct_bytes
        .try_into()
        .map_err(|_| GroupMetadataError::InvalidConversationType)?;
    let conversation_type_i32 = i32::from_be_bytes(ct_arr);
    let conversation_type: ConversationType = conversation_type_i32.try_into()?;

    // CREATOR_INBOX_ID: versioned `InboxId` TLS form. Hex-encode for the
    // legacy `String` slot the rest of the codebase consumes.
    let creator_inbox_id = DictInboxId::tls_deserialize_exact(creator_bytes)
        .map_err(|e| {
            GroupMetadataError::Conversion(xmtp_proto::ConversionError::InvalidValue {
                item: "CREATOR_INBOX_ID",
                expected: "versioned InboxId TLS encoding",
                got: format!("{e}"),
            })
        })?
        .to_hex();

    // DM_MEMBERS: `TlsSet<InboxId>` with exactly two elements, or absent.
    let dm_members = match dict.get(&ComponentId::DM_MEMBERS.as_u16()) {
        Some(b) => {
            let set = TlsSet::<DictInboxId>::tls_deserialize_exact(b)
                .map_err(|_| GroupMetadataError::InvalidDmMembers)?;
            let ids: Vec<DictInboxId> = set.iter().copied().collect();
            if ids.len() != 2 {
                return Err(GroupMetadataError::InvalidDmMembers);
            }
            Some(DmMembers {
                member_one_inbox_id: ids[0].to_hex(),
                member_two_inbox_id: ids[1].to_hex(),
            })
        }
        None => None,
    };

    // ONESHOT_MESSAGE: prost-encoded `OneshotMessage`.
    let oneshot_message = match dict.get(&ComponentId::ONESHOT_MESSAGE.as_u16()) {
        Some(b) => Some(OneshotMessage::decode(b)?),
        None => None,
    };

    Ok(Some(GroupMetadata {
        conversation_type,
        creator_inbox_id,
        dm_members,
        oneshot_message,
    }))
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
