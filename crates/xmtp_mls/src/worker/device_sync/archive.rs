use super::DeviceSyncError;
use futures::{Stream, StreamExt};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
pub use xmtp_archive::*;
use xmtp_configuration::LEGACY_GROUP_ID_BYTES;
use xmtp_db::{
    StorageError,
    TransactionOutcome::Continue,
    consent_record::{ConsentKey, ConsentWrite, Owed, parse_consent_save, write_consent},
    group::ConversationType,
    group_message::StoredGroupMessage,
    message_deletion::StoredMessageDeletion,
    prelude::*,
};
use xmtp_mls_core::{events::InternalEvent, state_tx::state_write, worker::NeedsDbReconnect};
use xmtp_mls_engine::groups::deletion::{
    ArchivedSelfDeletions, DiscardEvents, Source, apply_deletion, apply_stored_records,
    import_record,
};
use xmtp_mls_engine::groups::{MlsGroup, context::XmtpSharedContext};
use xmtp_proto::types::GroupId;
use xmtp_proto::xmtp::device_sync::{
    BackupElement, backup_element::Element, consent_backup::ConsentSave,
};

use xmtp_events::{ArchiveRestored, ClientEvent, EventWriter};
#[derive(Default)]
struct ImportContext {
    changed: bool,
    /// The import stored a deletion record or marked a row. It is not a
    /// change other installations are owed, but the app hears of
    /// it once through `archive.restored`.
    deletions_stored: bool,
    /// The archive's usable consent records, applied after all its elements.
    /// Archive frames are ciphertext and do not compress, so these grow with
    /// the archive's own size.
    consent: Vec<ConsentSave>,
    /// The archive's 16-byte DM groups, whatever their restore did.
    dm_groups: HashSet<GroupId>,
    /// Whether the client holds each group the archive's messages name as a
    /// sync or one-shot conversation.
    internal: HashMap<GroupId, bool>,
    /// The archive's deletion messages, which delete a message this import
    /// creates when its own sender sent them.
    self_deletions: ArchivedSelfDeletions,
}

impl ImportContext {
    fn is_internal(&mut self, db: &impl DbQuery, group_id: GroupId) -> Result<bool, StorageError> {
        if let Some(internal) = self.internal.get(&group_id) {
            return Ok(*internal);
        }
        let internal = db
            .find_group(&group_id)?
            .is_some_and(|group| group.conversation_type.is_virtual());
        self.internal.insert(group_id, internal);
        Ok(internal)
    }
}

/// Import an archive. `first` reads it once for its deletion messages;
/// `reopen` then reads it again for the import, so a pre-8.0 self-deletion
/// is restored whichever comes first, it or its target.
pub async fn insert_archive<F, Fut>(
    first: &mut ArchiveImporter,
    reopen: F,
    context: &impl XmtpSharedContext,
) -> Result<(), DeviceSyncError>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<ArchiveImporter, ArchiveError>>,
{
    let self_deletions = collect_self_deletions(first).await?;
    let mut importer = reopen().await?;
    insert_elements(&mut importer, context, self_deletions).await
}

/// The archive's deletion messages. A read failure fails the scan, so the
/// import never runs with some of them missing; an element that reads but
/// does not convert is left to the import, which reports it.
pub(super) async fn collect_self_deletions<S, E>(
    elements: &mut S,
) -> Result<ArchivedSelfDeletions, E>
where
    S: Stream<Item = Result<BackupElement, E>> + Unpin,
{
    let mut found = ArchivedSelfDeletions::default();
    while let Some(element) = elements.next().await {
        if let Some(Element::GroupMessage(save)) = element?.element
            && let Ok(message) = StoredGroupMessage::try_from(save)
        {
            found.note(&message);
        }
    }
    Ok(found)
}

/// Import archive elements already read, as an archive holding them imports.
#[cfg(any(test, feature = "test-utils"))]
#[doc(hidden)]
pub async fn insert_elements_for_test<S, E>(
    elements: &mut S,
    context: &impl XmtpSharedContext,
) -> Result<(), DeviceSyncError>
where
    S: Stream<Item = Result<BackupElement, E>> + Unpin,
    DeviceSyncError: From<E>,
{
    insert_elements(elements, context, ArchivedSelfDeletions::default()).await
}

// implements: ARCH-027
pub(super) async fn insert_elements<S, E>(
    elements: &mut S,
    context: &impl XmtpSharedContext,
    self_deletions: ArchivedSelfDeletions,
) -> Result<(), DeviceSyncError>
where
    S: Stream<Item = Result<BackupElement, E>> + Unpin,
    DeviceSyncError: From<E>,
{
    let mut import = ImportContext {
        self_deletions,
        ..Default::default()
    };
    let mut first_error = None;
    while let Some(element) = elements.next().await {
        let element = match element {
            Ok(element) => element,
            // A broken stream ends the import.
            Err(e) => {
                first_error.get_or_insert(DeviceSyncError::from(e));
                break;
            }
        };
        // A failed element does not stop the rest: a pre-8.0 archive lists
        // consent and later groups after a message whose group it left out.
        if let Err(e) = insert(element, context, &mut import) {
            if e.needs_db_reconnect() {
                return Err(e);
            }
            first_error.get_or_insert(e);
        }
    }
    // Consent after groups, even after a failed element.
    let consent = state_write(context.mls_storage(), |tx| {
        let storage = tx.storage();
        let db = storage.db();
        let changed = import_consent(
            &db,
            context.inbox_id(),
            &import,
            xmtp_common::time::now_ns(),
        )?;
        Ok::<_, StorageError>(Continue(changed))
    });
    match consent {
        Ok(changed) => import.changed |= changed.into_continued(),
        Err(e) if e.db_needs_connection() => return Err(e.into()),
        Err(e) => {
            first_error.get_or_insert(e.into());
        }
    }
    let result = first_error.map_or(Ok(()), Err);
    if import.changed {
        context.task_channels().mark_notification_changed();
        context.events().emit(
            Some(ClientEvent::ArchiveRestored(ArchiveRestored {
                complete: result.is_ok(),
            })),
            // Imported records are owed to the other installations.
            Some(InternalEvent::OutboxChanged),
        );
    } else if import.deletions_stored {
        context.events().emit(
            Some(ClientEvent::ArchiveRestored(ArchiveRestored {
                complete: result.is_ok(),
            })),
            None,
        );
    }
    result
}

fn insert(
    element: BackupElement,
    context: &impl XmtpSharedContext,
    import: &mut ImportContext,
) -> Result<(), DeviceSyncError> {
    let Some(element) = element.element else {
        return Ok(());
    };

    match element {
        // An unusable record is dropped here, so each held record is small.
        Element::Consent(save) => {
            if parse_consent_save(&save).is_some() {
                import.consent.push(save);
            }
        }
        // The exporter writes no sync or one-shot conversation, so an import
        // makes none and adds no message to one: device sync applies only
        // what the backend delivered.
        Element::Group(save)
            if ConversationType::try_from(save.conversation_type())
                .is_ok_and(|kind| kind.is_virtual()) => {}
        Element::Group(save) => {
            // Registered before the restore, so a re-import that finds the
            // group held combines the same rows.
            if save.dm_id.is_some()
                && save.id.len() == LEGACY_GROUP_ID_BYTES
                && let Ok(id) = GroupId::try_from(save.id.as_slice())
            {
                import.dm_groups.insert(id);
            }
            import.changed |= MlsGroup::restore_from_archive(context, &save)?;
        }
        // implements: ARCH-028
        Element::MessageDeletion(save) => {
            let record: StoredMessageDeletion = save.try_into()?;
            if !import.is_internal(&context.db(), record.group_id)? {
                import.deletions_stored |= state_write(context.mls_storage(), |tx| {
                    let stored = import_record(&tx.storage().db(), &DiscardEvents, &record)?;
                    Ok::<_, StorageError>(Continue(stored))
                })?
                .into_continued();
            }
        }
        // implements: ARCH-013, ARCH-028
        Element::GroupMessage(message) => {
            // Field 19 makes a new row marked and empty. A known row keeps
            // its state: archive data changes a row only in the insert that
            // creates it.
            let message: StoredGroupMessage = message.try_into()?;
            if import.is_internal(&context.db(), message.group_id)? {
                return Ok(());
            }
            import.self_deletions.note(&message);
            let self_deletions = &import.self_deletions;
            let (inserted, deletions) = state_write(context.mls_storage(), |tx| {
                let storage = tx.storage();
                let db = storage.db();
                let inserted = message.store_or_ignore_changed(&db)?;
                // A pre-8.0 self-deletion of the row this insert created.
                let mut deletions = inserted && self_deletions.apply_to_inserted(&db, &message)?;
                // The evidence is the incoming copy: a stored `Unpublished`
                // copy of the same id must not hide a later `Published` one.
                // The stored row itself stays unchanged.
                deletions |= apply_deletion(&db, &DiscardEvents, &message, Source::Archive)?;
                if let Some(row) = db.get_group_message(&message.id)? {
                    deletions |= apply_stored_records(&db, &DiscardEvents, &row)?;
                }
                Ok::<_, StorageError>(Continue((inserted, deletions)))
            })?
            .into_continued();
            import.changed |= inserted;
            import.deletions_stored |= deletions;
        }
        _ => {}
    }

    Ok(())
}

/// Apply one archive's consent records, after its groups. When the archive
/// carries a 16-byte DM with a peer, its records for every 16-byte DM held
/// with that peer and its inbox record for the peer become one record. For
/// any other peer, a record naming a 16-byte DM held from elsewhere moves to
/// the peer and merges by time. Every other record merges as named. Returns
/// whether anything changed.
// implements: CONS-051, CONS-054, CONS-061
fn import_consent(
    db: &impl DbQuery,
    own_inbox: &str,
    import: &ImportContext,
    now_ns: i64,
) -> Result<bool, StorageError> {
    // The peers of the 16-byte DMs this archive carries.
    let mut carried = BTreeSet::new();
    for id in &import.dm_groups {
        if let Some((peer, _)) = ConsentKey::group(id.as_ref()).pre8_dm(db, own_inbox)? {
            carried.insert(peer);
        }
    }
    let mut plain: BTreeMap<ConsentKey, Vec<ConsentSave>> = BTreeMap::new();
    let mut pre8: BTreeMap<ConsentKey, Vec<ConsentSave>> = BTreeMap::new();
    for save in &import.consent {
        // An unusable record is skipped.
        let Some((key, _)) = parse_consent_save(save) else {
            continue;
        };
        // A record naming a held 16-byte DM is its peer's.
        let key = key.pre8_dm(db, own_inbox)?.map_or(key, |(peer, _)| peer);
        let rows = if carried.contains(&key) {
            pre8.entry(key)
        } else {
            plain.entry(key)
        };
        rows.or_default().push(save.clone());
    }
    for (peer, rows) in pre8 {
        let combined = pre8::pre8_peer_consent(&peer.entity, &rows);
        plain.insert(peer, combined.into_iter().collect());
    }
    let mut changed = false;
    for (key, saves) in plain {
        for save in saves {
            let Some((_, value)) = parse_consent_save(&save) else {
                continue;
            };
            let write = ConsentWrite::from_archive(value, now_ns);
            changed |= write_consent(db, &key, write, Owed::Mark)?.is_some();
        }
    }
    Ok(changed)
}
