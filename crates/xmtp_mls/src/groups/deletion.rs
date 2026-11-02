//! Who may delete a message, how a deletion is recorded and applied, and how
//! an unsent own message is cancelled. Every deletion rule lives here.

use crate::context::XmtpSharedContext;
use crate::core::events::{InternalEvent, emit_deleted_messages};
use crate::groups::{DeleteMessageError, GroupError, MlsGroup};
use openmls::group::MlsGroup as OpenMlsGroup;
use std::collections::{HashSet, VecDeque};
use xmtp_common::snippet::Snippet;
use xmtp_db::group_intent::{IntentKind, IntentState};
use xmtp_db::group_message::{ContentType, DeliveryStatus, GroupMessageKind, StoredGroupMessage};
use xmtp_db::message_deletion::StoredMessageDeletion;
use xmtp_db::prelude::*;
use xmtp_db::{DbQuery, TransactionOutcome, XmtpMlsStorageProvider};
use xmtp_events::{ClientEvent, DeletionCause, EventContext, EventWriter};
use xmtp_proto::types::GroupId;

/// Whether `deleter` may delete `target`. Used by the request, receive, own
/// echo and import.
// implements: PROC-037, PROC-056, CTYPE-018
pub(crate) fn may_delete(
    group_id: &GroupId,
    deleter: &str,
    is_super_admin: bool,
    target: &StoredGroupMessage,
) -> Result<(), DeleteMessageError> {
    // A stitched DM reads rows from every group of the set.
    if &target.group_id != group_id {
        return Err(DeleteMessageError::NotAuthorized);
    }
    if target.deleted_by_inbox_id.is_some() {
        return Err(DeleteMessageError::MessageAlreadyDeleted);
    }
    if target.sender_inbox_id != deleter && !is_super_admin {
        return Err(DeleteMessageError::NotAuthorized);
    }
    // Reads the wire type, never the cached one.
    if !target.has_deletable_content() {
        return Err(DeleteMessageError::NonDeletableMessage);
    }
    Ok(())
}

/// Where a deletion message came from.
pub(crate) enum Source<'a> {
    /// Processed from the group's topic or this installation's own echo.
    Topic {
        super_admin: bool,
        cause: DeletionCause,
    },
    /// Stored by an archive import. Proves no super-admin status.
    Archive { imported: &'a Imported },
}

/// How many of an import's most recently inserted messages its later
/// deletion evidence may still mark. A pre-8.0 archive carries a deletion
/// message after its target; a current archive puts records before messages
/// and marks rows itself, so it needs none of this window.
const IMPORT_EVIDENCE_WINDOW: usize = 10_000;

/// The latest rows an archive import inserted, at most
/// [`IMPORT_EVIDENCE_WINDOW`]. Archive evidence (a mark, a record, a deletion
/// message) marks only these: a row the client held before the import, or
/// one this import inserted too long before, is marked only by a record from
/// the topic.
// implements: ARCH-028
#[derive(Default)]
pub(crate) struct Imported {
    ids: HashSet<Vec<u8>>,
    order: VecDeque<Vec<u8>>,
}

impl Imported {
    pub(crate) fn insert(&mut self, id: &[u8]) {
        if !self.ids.insert(id.to_vec()) {
            return;
        }
        self.order.push_back(id.to_vec());
        if self.order.len() > IMPORT_EVIDENCE_WINDOW
            && let Some(oldest) = self.order.pop_front()
        {
            self.ids.remove(&oldest);
        }
    }

    fn holds(&self, id: &[u8]) -> bool {
        self.ids.contains(id)
    }
}

/// Discards every event. An archive import emits no per-row event;
/// `archive.restored` reports it once.
// implements: EVENT-004
pub(crate) struct DiscardEvents;

impl EventWriter<InternalEvent> for DiscardEvents {
    fn emit_with_context(
        &self,
        _client: Option<ClientEvent>,
        _internal: Option<InternalEvent>,
        _context: EventContext,
    ) {
    }
}

/// Record a deletion message and mark its stored target. Idempotent.
/// Returns true when it stored a record or marked a row.
// implements: PROC-037
pub(crate) fn apply_deletion(
    db: &impl DbQuery,
    events: &impl EventWriter<InternalEvent>,
    deletion: &StoredGroupMessage,
    source: Source,
) -> Result<bool, xmtp_db::ConnectionError> {
    if deletion.kind != GroupMessageKind::Application
        || deletion.content_type != ContentType::DeleteMessage
    {
        return Ok(false);
    }
    // A topic copy is Published by definition. An archived one must say so.
    if matches!(source, Source::Archive { .. })
        && deletion.delivery_status != DeliveryStatus::Published
    {
        return Ok(false);
    }
    // Store time already decoded the DeleteMessage and checked its target id.
    let Some(target_id) = deletion.reference_id.as_deref() else {
        return Ok(false);
    };
    let (super_admin, authenticated) = match source {
        Source::Topic { super_admin, .. } => (super_admin, true),
        Source::Archive { .. } => (false, false),
    };
    let new = StoredMessageDeletion::of(deletion, target_id, super_admin, authenticated);
    let existed = db.get_message_deletion(&new.id)?;
    let record = db.record_deletion(&new)?;
    let recorded = existed.as_ref() != Some(&record);
    // A missing target stays unmarked; an import that stores it later finds the record.
    let Some(target) = db.get_group_message(&record.deleted_message_id)? else {
        return Ok(recorded);
    };
    let marked = match source {
        Source::Topic { cause, .. } => apply_record(db, events, &record, target, cause)?,
        Source::Archive { imported } => apply_stored_records(db, events, &target, imported)?,
    };
    Ok(recorded || marked)
}

/// Import only: mark a row this import stored or matched from its earliest
/// authorized record; a row held before the import, only from a record from
/// the topic. Receive never needs this: the topic orders a target before its
/// deletion. Returns true when it marked the row.
// implements: ARCH-028, PROC-037
pub(crate) fn apply_stored_records(
    db: &impl DbQuery,
    events: &impl EventWriter<InternalEvent>,
    row: &StoredGroupMessage,
    imported: &Imported,
) -> Result<bool, xmtp_db::ConnectionError> {
    if row.kind != GroupMessageKind::Application || row.deleted_by_inbox_id.is_some() {
        return Ok(false);
    }
    let held = !imported.holds(&row.id);
    let Some(record) = db.first_deletion_of(row, held)? else {
        return Ok(false);
    };
    apply_record(db, events, &record, row.clone(), DeletionCause::Deleted)
}

/// Store an archive's record, then mark a stored target it authorizes.
/// Returns true when it stored a record or marked a row.
// implements: ARCH-028
pub(crate) fn import_record(
    db: &impl DbQuery,
    events: &impl EventWriter<InternalEvent>,
    record: &StoredMessageDeletion,
    imported: &Imported,
) -> Result<bool, xmtp_db::ConnectionError> {
    let existed = db.get_message_deletion(&record.id)?;
    let stored = db.record_deletion(record)?;
    let recorded = existed.as_ref() != Some(&stored);
    // Mark from the earliest authorized record stored so far. The export orders
    // records by time, so one archive attributes to its earliest; a mark never
    // changes after.
    let marked = match db.get_group_message(&stored.deleted_message_id)? {
        Some(target) => apply_stored_records(db, events, &target, imported)?,
        None => false,
    };
    Ok(recorded || marked)
}

fn apply_record(
    db: &impl DbQuery,
    events: &impl EventWriter<InternalEvent>,
    record: &StoredMessageDeletion,
    target: StoredGroupMessage,
    cause: DeletionCause,
) -> Result<bool, xmtp_db::ConnectionError> {
    if let Err(error) = may_delete(
        &record.group_id,
        &record.deleted_by_inbox_id,
        record.is_super_admin_deletion,
        &target,
    ) {
        // The deletion message stays an ordinary message.
        tracing::debug!(
            target = target.id.as_slice().snippet(),
            "deletion not applied: {error}"
        );
        return Ok(false);
    }
    mark(db, events, target, &record.deleted_by_inbox_id, cause)
}

/// The one write that deletes: mark, remove the content, emit once.
/// Returns true when this call marked the row.
// implements: PROC-037
fn mark(
    db: &impl DbQuery,
    events: &impl EventWriter<InternalEvent>,
    target: StoredGroupMessage,
    deleted_by: &str,
    cause: DeletionCause,
) -> Result<bool, xmtp_db::ConnectionError> {
    // `deleted_by_inbox_id IS NULL` in the UPDATE makes the first write win.
    if db.mark_message_deleted(&target.id, deleted_by)? == 0 {
        return Ok(false);
    }
    let marked = StoredGroupMessage {
        deleted_by_inbox_id: Some(deleted_by.to_owned()),
        // The old bytes drop here and are wiped.
        decrypted_message_bytes: Vec::new(),
        ..target
    };
    emit_deleted_messages(events, vec![marked], cause, Some(deleted_by.to_owned()), db)?;
    Ok(true)
}

impl<Context: XmtpSharedContext> MlsGroup<Context> {
    /// Whether a deletion message's sender is a super admin at this point in
    /// the group's topic. Receive and own echo call it while processing that
    /// message, so every installation reaches the same judgement.
    // implements: PROC-037
    pub(crate) fn deletion_admin_bit(
        &self,
        mls_group: &OpenMlsGroup,
        row: &StoredGroupMessage,
    ) -> bool {
        row.content_type == ContentType::DeleteMessage
            && self
                .is_super_admin_without_lock(mls_group, row.sender_inbox_id.clone())
                .unwrap_or(false)
    }

    /// Remove this installation's unsent message and its queued sends in one
    /// write. `Ok(false)`: not an unsent own message; the caller queues a
    /// deletion.
    // implements: SEND-026
    pub(crate) fn cancel_unsent_message(&self, id: &[u8]) -> Result<bool, GroupError> {
        crate::state_tx::state_write_with_events(
            self.context.mls_storage(),
            self.context.events(),
            |tx, events| {
                // The writer and group lock order this write against
                // prepare_publish_attempt, which re-fetches the intent under them.
                tx.with_group(self.group_id, |_group, storage| {
                    let db = storage.db();
                    let Some(message) = db.get_group_message(id)? else {
                        return Ok(TransactionOutcome::Continue(false));
                    };
                    if !message.is_unsent_from(self.context.installation_id().as_slice()) {
                        return Ok(TransactionOutcome::Continue(false));
                    }
                    let mut queued = Vec::new();
                    for intent in db.find_group_intents(
                        self.group_id,
                        Some(vec![
                            IntentState::ToPublish,
                            IntentState::Published,
                            IntentState::Committed,
                        ]),
                        Some(vec![IntentKind::SendMessage]),
                    )? {
                        if crate::groups::message_id::calculate_message_id_for_intent(&intent)?
                            .as_deref()
                            != Some(id)
                        {
                            continue;
                        }
                        // A saved attempt may already be on the backend.
                        if intent.state != IntentState::ToPublish
                            || db.prepared_envelopes(intent.id)?.is_some()
                        {
                            return Err(DeleteMessageError::MessageInFlight.into());
                        }
                        queued.push(intent.id);
                    }
                    for intent_id in queued {
                        db.delete_group_intent(intent_id)?;
                    }
                    db.delete_message_by_id(id)?;
                    emit_deleted_messages(
                        events,
                        vec![message],
                        DeletionCause::DeletedLocally,
                        None,
                        &db,
                    )?;
                    Ok(TransactionOutcome::Continue(true))
                })
            },
        )
        .map(TransactionOutcome::into_continued)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The import window holds the latest inserted ids, and no more.
    // verifies: ARCH-028
    #[xmtp_common::test]
    fn import_window_is_bounded() {
        let mut imported = Imported::default();
        for n in 0..=IMPORT_EVIDENCE_WINDOW as u32 {
            imported.insert(&n.to_be_bytes());
        }
        assert!(!imported.holds(&0u32.to_be_bytes()));
        assert!(imported.holds(&1u32.to_be_bytes()));
        assert!(imported.holds(&(IMPORT_EVIDENCE_WINDOW as u32).to_be_bytes()));
        assert_eq!(imported.ids.len(), IMPORT_EVIDENCE_WINDOW);
        assert_eq!(imported.order.len(), IMPORT_EVIDENCE_WINDOW);
    }
}
