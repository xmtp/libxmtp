use super::*;
use openmls::group::{GroupId, MlsGroup};
use xmtp_db::group::{GroupQueryArgs, StoredGroup};
use xmtp_db::sql_key_store::SqlKeyStore;
use xmtp_mls_common::{
    group_metadata::GroupMetadata, group_mutable_metadata::GroupMutableMetadata,
};
use xmtp_proto::xmtp::device_sync::{
    backup_element::Element,
    group_backup::{
        ConversationTypeSave, GroupMembershipStateSave, ImmutableMetadataSave, MutableMetadataSave,
    },
};

#[xmtp_common::async_trait]
impl BackupRecordProvider for GroupSave {
    const BATCH_SIZE: i64 = 100;
    async fn backup_records<D>(
        db: Arc<D>,
        start_ns: Option<i64>,
        end_ns: Option<i64>,
        _exclude_disappearing_messages: bool,
        cursor: i64,
    ) -> Result<Vec<BackupElement>, StorageError>
    where
        Self: Sized,
        D: DbQuery + 'static,
    {
        let mut args = GroupQueryArgs::default();

        if let Some(start_ns) = start_ns {
            args.created_after_ns = Some(start_ns);
        }
        if let Some(end_ns) = end_ns {
            args.created_before_ns = Some(end_ns);
        }

        args.limit = Some(Self::BATCH_SIZE);

        let batch = db.find_groups_by_id_paged(args, cursor)?;
        let storage = SqlKeyStore::new(db);
        let records = batch
            .into_iter()
            .filter_map(|record| {
                if record.conversation_type.is_virtual() {
                    return None;
                }
                let mls_group =
                    MlsGroup::load(&storage, &GroupId::from_slice(&record.id)).ok()??;
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
            welcome_id: group.sequence_id,
            rotated_at_ns: group.rotated_at_ns,
            conversation_type: conversation_type as i32,
            dm_id: group.dm_id,
            last_message_ns: group.last_message_ns,
            message_disappear_from_ns: group.message_disappear_from_ns,
            message_disappear_in_ns: group.message_disappear_in_ns,
            paused_for_version: group.paused_for_version,
            metadata: Some(ImmutableMetadataSave {
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
