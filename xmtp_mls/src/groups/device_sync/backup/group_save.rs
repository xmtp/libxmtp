use crate::storage::group::{ConversationType, GroupMembershipState, StoredGroup};
use xmtp_proto::xmtp::device_sync::group_backup::{
    ConversationTypeSave, GroupMembershipStateSave, StoredGroupSave,
};

impl From<StoredGroupSave> for StoredGroup {
    fn from(value: StoredGroupSave) -> Self {
        let membership_state = value.membership_state().into();
        let conversation_type = value.conversation_type().into();

        Self {
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
        }
    }
}

impl From<GroupMembershipStateSave> for GroupMembershipState {
    fn from(value: GroupMembershipStateSave) -> Self {
        match value {
            GroupMembershipStateSave::Allowed => Self::Allowed,
            GroupMembershipStateSave::Pending => Self::Pending,
            GroupMembershipStateSave::Rejected => Self::Rejected,
        }
    }
}

impl From<ConversationTypeSave> for ConversationType {
    fn from(value: ConversationTypeSave) -> Self {
        match value {
            ConversationTypeSave::Dm => Self::Dm,
            ConversationTypeSave::Group => Self::Group,
            ConversationTypeSave::Sync => Self::Sync,
        }
    }
}

impl From<StoredGroup> for StoredGroupSave {
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
