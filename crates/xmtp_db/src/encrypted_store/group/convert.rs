use super::*;
use xmtp_proto::ConversionError;
use xmtp_proto::xmtp::device_sync::group_backup::GroupMembershipStateSave;

impl TryFrom<GroupMembershipStateSave> for GroupMembershipState {
    type Error = ConversionError;
    fn try_from(value: GroupMembershipStateSave) -> Result<Self, Self::Error> {
        let membership_state = match value {
            GroupMembershipStateSave::Allowed => Self::Allowed,
            GroupMembershipStateSave::Pending => Self::Pending,
            GroupMembershipStateSave::Rejected => Self::Rejected,
            GroupMembershipStateSave::Restored => Self::Restored,
            GroupMembershipStateSave::PendingRemove => Self::PendingRemove,
            _ => {
                return Err(ConversionError::Unspecified("group_membership_state"));
            }
        };
        Ok(membership_state)
    }
}

impl From<GroupMembershipState> for GroupMembershipStateSave {
    fn from(value: GroupMembershipState) -> Self {
        match value {
            GroupMembershipState::Allowed => Self::Allowed,
            GroupMembershipState::Pending => Self::Pending,
            GroupMembershipState::Rejected => Self::Rejected,
            GroupMembershipState::Restored => Self::Restored,
            GroupMembershipState::PendingRemove => Self::PendingRemove,
        }
    }
}
