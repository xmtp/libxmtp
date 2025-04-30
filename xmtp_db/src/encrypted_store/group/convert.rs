use super::*;
use xmtp_proto::ConversionError;
use xmtp_proto::xmtp::device_sync::group_backup::{
    ConversationTypeSave, GroupMembershipStateSave, GroupSave,
};

use xmtp_proto::xmtp::mls::message_contents::ConversationType as ConversationTypeProto;

impl TryFrom<GroupSave> for StoredGroup {
    type Error = ConversionError;
    fn try_from(value: GroupSave) -> Result<Self, Self::Error> {
        let membership_state = value.membership_state().try_into()?;
        let conversation_type = value.conversation_type().try_into()?;

        Ok(Self {
            id: value.id,
            created_at_ns: value.created_at_ns,
            membership_state,
            installations_last_checked: value.installations_last_checked,
            added_by_inbox_id: value.added_by_inbox_id,
            welcome_id: value.welcome_id,
            rotated_at_ns: value.rotated_at_ns,
            conversation_type,
            dm_id: value.dm_id,
            last_message_ns: value.last_message_ns,
            message_disappear_from_ns: value.message_disappear_from_ns,
            message_disappear_in_ns: value.message_disappear_in_ns,
            paused_for_version: None, // TODO: Add this to the backup
            maybe_forked: false,
            fork_details: String::new(),
        })
    }
}

impl TryFrom<GroupMembershipStateSave> for GroupMembershipState {
    type Error = ConversionError;
    fn try_from(value: GroupMembershipStateSave) -> Result<Self, Self::Error> {
        let membership_state = match value {
            GroupMembershipStateSave::Allowed => Self::Allowed,
            GroupMembershipStateSave::Pending => Self::Pending,
            GroupMembershipStateSave::Rejected => Self::Rejected,
            GroupMembershipStateSave::Unspecified => {
                return Err(ConversionError::Unspecified("group_membership_state"));
            }
        };
        Ok(membership_state)
    }
}

impl TryFrom<ConversationTypeSave> for ConversationType {
    type Error = ConversionError;
    fn try_from(value: ConversationTypeSave) -> Result<Self, Self::Error> {
        let conversation_type = match value {
            ConversationTypeSave::Dm => Self::Dm,
            ConversationTypeSave::Group => Self::Group,
            ConversationTypeSave::Sync => Self::Sync,
            ConversationTypeSave::Unspecified => {
                return Err(ConversionError::Unspecified("conversation_type"));
            }
        };
        Ok(conversation_type)
    }
}

impl From<StoredGroup> for GroupSave {
    fn from(value: StoredGroup) -> Self {
        let membership_state: GroupMembershipStateSave = value.membership_state.into();
        let conversation_type: ConversationTypeSave = value.conversation_type.into();

        Self {
            id: value.id,
            created_at_ns: value.created_at_ns,
            membership_state: membership_state as i32,
            installations_last_checked: value.installations_last_checked,
            added_by_inbox_id: value.added_by_inbox_id,
            welcome_id: value.welcome_id,
            rotated_at_ns: value.rotated_at_ns,
            conversation_type: conversation_type as i32,
            dm_id: value.dm_id,
            last_message_ns: value.last_message_ns,
            message_disappear_from_ns: value.message_disappear_from_ns,
            message_disappear_in_ns: value.message_disappear_in_ns,
        }
    }
}

impl From<GroupMembershipState> for GroupMembershipStateSave {
    fn from(value: GroupMembershipState) -> Self {
        match value {
            GroupMembershipState::Allowed => Self::Allowed,
            GroupMembershipState::Pending => Self::Pending,
            GroupMembershipState::Rejected => Self::Rejected,
        }
    }
}
impl From<ConversationType> for ConversationTypeSave {
    fn from(value: ConversationType) -> Self {
        match value {
            ConversationType::Dm => Self::Dm,
            ConversationType::Group => Self::Group,
            ConversationType::Sync => Self::Sync,
        }
    }
}

/**
 * XMTP supports the following types of conversation
 *
 * *Group*: A conversation with 1->N members and complex permissions and roles
 * *DM*: A conversation between 2 members with simplified permissions
 * *Sync*: A conversation between all the devices of a single member with simplified permissions
 */
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
    type Error = xmtp_proto::ConversionError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        Ok(match value {
            1 => Self::Group,
            2 => Self::Dm,
            3 => Self::Sync,
            n => {
                return Err(ConversionError::InvalidValue {
                    item: "ConversationType",
                    expected: "number between 1 - 3",
                    got: n.to_string(),
                });
            }
        })
    }
}
