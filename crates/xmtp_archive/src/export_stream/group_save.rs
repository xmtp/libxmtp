use super::*;
use openmls::group::MlsGroup;
use xmtp_db::group::{GroupMembershipState, GroupQueryArgs, StoredGroup};
use xmtp_db::sql_key_store::SqlKeyStore;
use xmtp_mls_common::{
    group_metadata::{GroupMetadata, extract_group_metadata},
    group_mutable_metadata::{GroupMutableMetadata, merge_dict_into_mutable_metadata_lossy},
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
        state: Arc<BackupProviderState<D>>,
    ) -> Result<Vec<BackupElement>, StorageError>
    where
        Self: Sized,
        D: DbQuery + 'static,
    {
        let mut args = GroupQueryArgs::default();

        if let Some(start_ns) = state.opts.start_ns {
            args.created_after_ns = Some(start_ns);
        }
        if let Some(end_ns) = state.opts.end_ns {
            args.created_before_ns = Some(end_ns);
        }

        args.limit = Some(Self::BATCH_SIZE);

        let cursor = state.cursor.load(Ordering::SeqCst);
        let batch = state.db.find_groups_by_id_paged(args, cursor)?;
        let mut records = Vec::with_capacity(batch.len());
        for record in batch {
            if record.conversation_type.is_virtual() {
                continue;
            }
            // A Restored conversation re-exports its archived record, with the
            // presence of its metadata message unchanged. Only the physical
            // id, membership, and merged activity come from the current row.
            // implements: ARCH-025
            if record.membership_state == GroupMembershipState::Restored
                && let Some(history) = state.db.restored_group_history(&record.id)?
            {
                let membership_state: GroupMembershipStateSave = record.membership_state.into();
                records.push(BackupElement {
                    element: Some(Element::Group(GroupSave {
                        id: record.id.to_vec(),
                        membership_state: membership_state as i32,
                        last_message_ns: record.last_message_ns,
                        ..history
                    })),
                });
                continue;
            }
            if let Some(element) = live_group_element(record, &state.db) {
                records.push(element);
            }
        }

        Ok(records)
    }
}

/// A group's validated live metadata, or `None` with a log when its MLS
/// state is unreadable.
fn live_group_element<D: DbQuery + 'static>(
    record: StoredGroup,
    db: &Arc<D>,
) -> Option<BackupElement> {
    let storage = SqlKeyStore::new(db);
    let group_id = record.id;
    let mls_group = match MlsGroup::load(&storage, &group_id.to_openmls()) {
        Ok(Some(mls_group)) => mls_group,
        Ok(None) => {
            tracing::warn!(
                group_id = %group_id,
                "skipping group in backup: no MLS group state found"
            );
            return None;
        }
        Err(e) => {
            tracing::warn!(
                group_id = %group_id,
                error = %e,
                "skipping group in backup: failed to load MLS group state"
            );
            return None;
        }
    };
    let extensions = mls_group.extensions();

    let immutable_metadata = extract_group_metadata(extensions)
        .inspect_err(|e| {
            tracing::warn!(
                group_id = %group_id,
                error = %e,
                "skipping group in backup: unreadable group metadata"
            );
        })
        .ok()?;
    let mut mutable_metadata =
        GroupMutableMetadata::new(std::collections::HashMap::new(), Vec::new(), Vec::new());
    // Per-field degrade, never per-group: a malformed
    // component loses that one field, not the whole group.
    // Dropping the group would orphan its exported messages
    // and make the restore foreign-key check fail.
    for e in merge_dict_into_mutable_metadata_lossy(&mut mutable_metadata, extensions) {
        tracing::warn!(
            group_id = %group_id,
            error = %e,
            "skipping malformed metadata component in backup; \
             group still exported"
        );
    }

    Some(BackupElement {
        element: Some(Element::Group(GroupSave::new(
            record,
            immutable_metadata,
            mutable_metadata,
        ))),
    })
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
            id: group.id.to_vec(),
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
