use super::*;
use crate::{
    groups::{group_metadata::GroupMetadata, group_mutable_metadata::GroupMutableMetadata},
    storage::{
        group::{ConversationType, GroupMembershipState, StoredGroup},
        schema::groups,
        StorageError,
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
    fn backup_records(
        streamer: &BackupRecordStreamer<Self>,
    ) -> Result<Vec<BackupElement>, StorageError>
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

        query = query.limit(Self::BATCH_SIZE).offset(streamer.cursor);

        let batch = streamer
            .provider
            .conn_ref()
            .raw_query_read(|conn| query.load::<StoredGroup>(conn))?;

        let storage = streamer.provider.storage();
        let records = batch
            .into_iter()
            .filter_map(|record| {
                let mls_group =
                    OpenMlsGroup::load(storage, &GroupId::from_slice(&record.id)).ok()??;
                let immutable = mls_group.extensions().immutable_metadata()?;

                let immutable_metadata = GroupMetadata::try_from(immutable.metadata()).ok()?;
                let mutable_metadata = GroupMutableMetadata::try_from(&mls_group).ok()?;

                Some(BackupElement {
                    element: Some(Element::Group(GroupSave::new(
                        record,
                        immutable_metadata,
                        mutable_metadata,
                    ))),
                })
            })
            .collect();

        Ok(records)
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
            paused_for_version: value.paused_for_version,
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

trait GroupSaveExt {
    fn new(
        group: StoredGroup,
        immutable_metadata: GroupMetadata,
        mutable_metadata: GroupMutableMetadata,
    ) -> Self;
}
impl GroupSaveExt for GroupSave {
    fn new(
        group: StoredGroup,
        immutable_metadata: GroupMetadata,
        mutable_metadata: GroupMutableMetadata,
    ) -> Self {
        let membership_state: GroupMembershipStateSave = group.membership_state.into();
        let conversation_type: ConversationTypeSave = group.conversation_type.into();

        Self {
            id: group.id,
            created_at_ns: group.created_at_ns,
            membership_state: membership_state as i32,
            installations_last_checked: group.installations_last_checked,
            added_by_inbox_id: group.added_by_inbox_id,
            welcome_id: group.welcome_id,
            rotated_at_ns: group.rotated_at_ns,
            conversation_type: conversation_type as i32,
            dm_id: group.dm_id,
            last_message_ns: group.last_message_ns,
            message_disappear_from_ns: group.message_disappear_from_ns,
            message_disappear_in_ns: group.message_disappear_in_ns,
            paused_for_version: group.paused_for_version,
            metdata: Some(ImmutableMetadataSave {
                creator_inbox_id: immutable_metadata.creator_inbox_id,
            }),
            mutable_metadata: Some(MutableMetadataSave {
                super_admin_list: mutable_metadata.super_admin_list,
                attributes: mutable_metadata.attributes,
                admin_list: mutable_metadata.admin_list,
            }),
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
