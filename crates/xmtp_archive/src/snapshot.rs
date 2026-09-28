//! The export snapshot: every selected element, read in one SQLite read
//! transaction and measured at one export time.
//!
//! [`read`] runs `BEGIN` (deferred, read-only) on one connection, runs the
//! selection queries below, loads each group's MLS state through that same
//! connection, and commits before it returns. The transaction therefore lasts
//! only as long as the reads, never across an await or while the caller
//! consumes the archive; under WAL, writers proceed concurrently. The snapshot
//! is materialized in memory, so memory grows with the selected history.

use crate::{
    ArchiveError, UnreadableGroup,
    archive_options::{ArchiveOptions, BackupElementSelection},
};
use openmls::group::MlsGroup;
use xmtp_db::{
    ConnectionExt, TransactionalKeyStore, XmtpMlsStorageProvider,
    consent_record::StoredConsentRecord,
    diesel::{Connection, SqliteConnection, prelude::*},
    group::{ConversationType, StoredGroup},
    group_message::{GroupMessageKind, StoredGroupMessage},
    schema::{consent_records, group_messages, groups},
};
use xmtp_mls_common::{
    group_metadata::extract_group_metadata,
    group_mutable_metadata::{GroupMutableMetadata, merge_dict_into_mutable_metadata_lossy},
};
use xmtp_proto::xmtp::device_sync::{
    BackupElement,
    backup_element::Element,
    group_backup::{
        ConversationTypeSave, GroupMembershipStateSave, GroupSave, ImmutableMetadataSave,
        MutableMetadataSave,
    },
};

/// Every element `opts` selects, as stored when the transaction began: all
/// eligible groups, then their messages, then consent. Fails, rather than
/// omitting it, on any eligible group whose MLS state or immutable metadata
/// cannot be read.
pub(crate) fn read(
    db: &impl ConnectionExt,
    opts: &ArchiveOptions,
    exported_at_ns: i64,
) -> Result<Vec<BackupElement>, ArchiveError> {
    db.raw_query(|conn| {
        Ok(conn.transaction(|conn| read_in_transaction(conn, opts, exported_at_ns)))
    })?
}

fn read_in_transaction(
    conn: &mut SqliteConnection,
    opts: &ArchiveOptions,
    exported_at_ns: i64,
) -> Result<Vec<BackupElement>, ArchiveError> {
    let selects = |s| opts.elements.contains(&s);
    let mut elements = Vec::new();
    if selects(BackupElementSelection::Messages) {
        let groups = groups::table
            .filter(groups::conversation_type.ne_all(ConversationType::virtual_types()))
            .order(groups::id)
            .load::<StoredGroup>(conn)?;
        let store = conn.key_store();
        for group in groups {
            elements.push(Element::Group(group_save(&store, group)?));
        }
        let mut messages = group_messages::table
            .inner_join(groups::table)
            .filter(groups::conversation_type.ne_all(ConversationType::virtual_types()))
            .filter(group_messages::kind.eq(GroupMessageKind::Application))
            .select(StoredGroupMessage::as_select())
            .order(group_messages::id)
            .into_boxed();
        if let Some(start_ns) = opts.start_ns {
            messages = messages.filter(group_messages::sent_at_ns.gt(start_ns));
        }
        if let Some(end_ns) = opts.end_ns {
            messages = messages.filter(group_messages::sent_at_ns.le(end_ns));
        }
        messages = if opts.exclude_disappearing_messages {
            messages.filter(group_messages::expire_at_ns.is_null())
        } else {
            messages.filter(
                group_messages::expire_at_ns
                    .is_null()
                    .or(group_messages::expire_at_ns.gt(exported_at_ns)),
            )
        };
        let messages = messages.load(conn)?;
        elements.extend(
            messages
                .into_iter()
                .map(|m| Element::GroupMessage(m.into())),
        );
    }
    if selects(BackupElementSelection::Consent) {
        let consent = consent_records::table
            .order((consent_records::entity_type, consent_records::entity))
            .load::<StoredConsentRecord>(conn)?;
        elements.extend(consent.into_iter().map(|c| Element::Consent(c.into())));
    }
    Ok(elements
        .into_iter()
        .map(|e| BackupElement { element: Some(e) })
        .collect())
}

/// The group element for `group`, with metadata read from its MLS state.
fn group_save(
    store: &impl XmtpMlsStorageProvider,
    group: StoredGroup,
) -> Result<GroupSave, ArchiveError> {
    let group_id = group.id;
    let unreadable = |source: UnreadableGroup| ArchiveError::UnreadableGroup { group_id, source };
    let mls_group = MlsGroup::load(store, &group.id.to_openmls())
        .map_err(|e| unreadable(e.into()))?
        .ok_or_else(|| unreadable(UnreadableGroup::MissingState))?;
    let extensions = mls_group.extensions();
    let immutable = extract_group_metadata(extensions).map_err(|e| unreadable(e.into()))?;
    let mut mutable = GroupMutableMetadata::new(Default::default(), Vec::new(), Vec::new());
    // A malformed optional component loses that field, not the group.
    for e in merge_dict_into_mutable_metadata_lossy(&mut mutable, extensions) {
        tracing::warn!(group_id = %group_id, error = %e, "exporting group without a malformed metadata component");
    }
    let membership_state: GroupMembershipStateSave = group.membership_state.into();
    let conversation_type: ConversationTypeSave = group.conversation_type.into();
    Ok(GroupSave {
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
            creator_inbox_id: immutable.creator_inbox_id,
        }),
        mutable_metadata: Some(MutableMetadataSave {
            attributes: mutable.attributes,
            admin_list: mutable.admin_list,
            super_admin_list: mutable.super_admin_list,
        }),
    })
}

#[cfg(test)]
mod tests {
    use crate::{
        ArchiveError, ArchiveImporter,
        archive_options::{ArchiveOptions, BackupElementSelection},
        exporter::ArchiveExporter,
    };
    use futures::{
        AsyncReadExt, StreamExt,
        io::{BufReader, Cursor},
    };
    use xmtp_db::{
        Store, TestDb, XmtpTestDb,
        consent_record::{ConsentState, ConsentType, StoredConsentRecord},
        group::{ConversationType, GroupMembershipState, StoredGroup},
        prelude::*,
    };
    use xmtp_proto::{
        types::GroupId,
        xmtp::device_sync::{backup_element::Element, consent_backup::ConsentStateSave},
    };

    const KEY: [u8; 32] = [7; 32];

    fn group(id: [u8; 16], conversation_type: ConversationType) -> StoredGroup {
        StoredGroup::builder()
            .id(GroupId::from(id))
            .created_at_ns(1)
            .membership_state(GroupMembershipState::Allowed)
            .added_by_inbox_id("adder")
            .conversation_type(conversation_type)
            .build()
            .unwrap()
    }

    fn consent(entity: &str, state: ConsentState) -> StoredConsentRecord {
        StoredConsentRecord::new(ConsentType::InboxId, state, entity.into())
    }

    fn options(elements: &[BackupElementSelection]) -> ArchiveOptions {
        ArchiveOptions {
            elements: elements.to_vec(),
            start_ns: Some(1_000),
            end_ns: Some(2_000),
            exclude_disappearing_messages: false,
        }
    }

    /// Every element after the metadata frame.
    async fn elements(mut exporter: ArchiveExporter) -> Vec<Element> {
        let mut bytes = Vec::new();
        exporter.read_to_end(&mut bytes).await.unwrap();
        let reader = Box::pin(BufReader::new(Cursor::new(bytes)));
        ArchiveImporter::load(reader, &KEY)
            .await
            .unwrap()
            .map(|e| e.unwrap().element.unwrap())
            .collect()
            .await
    }

    /// An export reads every eligible conversation at one export time: an
    /// eligible group it cannot read fails the export even when it was
    /// created outside the window, has no messages, and sorts after more than
    /// a page of excluded internal conversations. An explicit empty selection
    /// reads nothing, and writes after the export starts are not in it.
    /// Restore coverage for readable conversations lives in xmtp_mls.
    // verifies: ARCH-007, ARCH-017
    #[xmtp_common::test(unwrap_try = true)]
    async fn archive_snapshot_is_complete() {
        let store = TestDb::create_ephemeral_store().await;
        let db = store.db();
        for i in 0..150u8 {
            let kind = [ConversationType::Sync, ConversationType::Oneshot][usize::from(i % 2)];
            let mut id = [0; 16];
            id[15] = i;
            group(id, kind).store(&db)?;
        }
        let unreadable = group([0xff; 16], ConversationType::Group);
        unreadable.store(&db)?;
        consent("alix", ConsentState::Allowed).store(&db)?;

        let failure = ArchiveExporter::new(options(&[BackupElementSelection::Messages]), &db, &KEY);
        assert!(
            matches!(
                failure,
                Err(ArchiveError::UnreadableGroup { group_id, .. }) if group_id == unreadable.id
            ),
            "export skipped an unreadable eligible group"
        );

        let empty = ArchiveExporter::new(options(&[]), &db, &KEY)?;
        assert!(empty.metadata().elements.is_empty());
        assert_eq!(elements(empty).await, vec![]);

        let exporter =
            ArchiveExporter::new(options(&[BackupElementSelection::Consent]), &db, &KEY)?;
        db.insert_or_replace_consent_records(&[
            consent("alix", ConsentState::Denied),
            consent("bo", ConsentState::Allowed),
        ])?;
        let exported = elements(exporter).await;
        assert!(
            matches!(
                exported.as_slice(),
                [Element::Consent(c)] if c.entity == "alix" && c.state == ConsentStateSave::Allowed as i32
            ),
            "export did not keep its snapshot: {exported:?}"
        );
    }
}
