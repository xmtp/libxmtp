use super::*;
use crate::{
    groups::{
        device_sync::DeviceSyncError, group_metadata::extract_group_metadata,
        group_mutable_metadata::GroupMutableMetadata,
    },
    storage::{
        group::{ConversationType, GroupMembershipState, StoredGroup},
        schema::groups,
        NotFound, StorageError,
    },
};
use diesel::prelude::*;
use openmls::group::{GroupId, MlsGroup as OpenMlsGroup};
use openmls_traits::OpenMlsProvider;
use xmtp_id::associations::DeserializationError;
use xmtp_proto::xmtp::device_sync::{
    backup_element::Element,
    group_backup::{
        ConversationTypeSave, GroupMembershipStateSave, GroupSave, ImmutableMetadataSave,
        MutableMetadataSave,
    },
};

impl BackupRecordProvider for GroupSave {
    const BATCH_SIZE: i64 = 100;
    fn backup_records(streamer: &BackupRecordStreamer<Self>) -> Vec<BackupElement>
    where
        Self: Sized,
    {
        let mut query = groups::table
            .filter(groups::conversation_type.ne(ConversationType::Sync))
            .order_by(groups::id)
            .into_boxed();

        if let Some(start_ns) = streamer.start_ns {
            query = query.filter(groups::created_at_ns.gt(start_ns));
        }
        if let Some(end_ns) = streamer.end_ns {
            query = query.filter(groups::created_at_ns.le(end_ns));
        }

        query = query.limit(Self::BATCH_SIZE).offset(streamer.offset);

        let batch = streamer
            .provider
            .conn_ref()
            .raw_query_read(|conn| query.load::<StoredGroup>(conn))
            .expect("Failed to load group records");

        batch
            .into_iter()
            .filter_map(|record| {
                record
                    .to_group_save(&streamer.provider)
                    .ok()
                    .map(|save| BackupElement {
                        element: Some(Element::Group(save)),
                    })
            })
            .collect()
    }
}

impl TryFrom<GroupSave> for StoredGroup {
    type Error = DeserializationError;
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
        })
    }
}

impl TryFrom<GroupMembershipStateSave> for GroupMembershipState {
    type Error = DeserializationError;
    fn try_from(value: GroupMembershipStateSave) -> Result<Self, Self::Error> {
        let membership_state = match value {
            GroupMembershipStateSave::Allowed => Self::Allowed,
            GroupMembershipStateSave::Pending => Self::Pending,
            GroupMembershipStateSave::Rejected => Self::Rejected,
            GroupMembershipStateSave::Unspecified => {
                return Err(DeserializationError::Unspecified("group_membership_state"))
            }
        };
        Ok(membership_state)
    }
}

impl TryFrom<ConversationTypeSave> for ConversationType {
    type Error = DeserializationError;
    fn try_from(value: ConversationTypeSave) -> Result<Self, Self::Error> {
        let conversation_type = match value {
            ConversationTypeSave::Dm => Self::Dm,
            ConversationTypeSave::Group => Self::Group,
            ConversationTypeSave::Sync => Self::Sync,
            ConversationTypeSave::Unspecified => {
                return Err(DeserializationError::Unspecified("conversation_type"))
            }
        };
        Ok(conversation_type)
    }
}

impl StoredGroup {
    fn to_group_save(self, provider: &XmtpOpenMlsProvider) -> Result<GroupSave, DeviceSyncError> {
        let membership_state: GroupMembershipStateSave = self.membership_state.into();
        let conversation_type: ConversationTypeSave = self.conversation_type.into();
        let mls_group = OpenMlsGroup::load(provider.storage(), &GroupId::from_slice(&self.id))
            .map_err(|_| StorageError::NotFound(NotFound::MlsGroup))?
            .ok_or(StorageError::NotFound(NotFound::MlsGroup))?;
        let metadata = extract_group_metadata(&mls_group)?;
        let mutable_metadata = GroupMutableMetadata::try_from(&mls_group)?;

        let save = GroupSave {
            id: self.id,
            created_at_ns: self.created_at_ns,
            membership_state: membership_state as i32,
            installations_last_checked: self.installations_last_checked,
            added_by_inbox_id: self.added_by_inbox_id,
            welcome_id: self.welcome_id,
            rotated_at_ns: self.rotated_at_ns,
            conversation_type: conversation_type as i32,
            dm_id: self.dm_id,
            last_message_ns: self.last_message_ns,
            message_disappear_from_ns: self.message_disappear_from_ns,
            message_disappear_in_ns: self.message_disappear_in_ns,
            metdata: Some(ImmutableMetadataSave {
                creator_inbox_id: metadata.creator_inbox_id,
            }),
            mutable_metadata: Some(mutable_metadata.into()),
        };

        Ok(save)
    }
}

impl From<GroupMutableMetadata> for MutableMetadataSave {
    fn from(value: GroupMutableMetadata) -> Self {
        Self {
            attributes: value.attributes,
            admin_list: value.admin_list,
            super_admin_list: value.super_admin_list,
        }
    }
}

impl From<MutableMetadataSave> for GroupMutableMetadata {
    fn from(value: MutableMetadataSave) -> Self {
        Self {
            attributes: value.attributes,
            admin_list: value.admin_list,
            super_admin_list: value.super_admin_list,
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
