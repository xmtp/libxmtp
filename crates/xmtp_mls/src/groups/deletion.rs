//! Who may delete a message, how a deletion is recorded and applied, and how
//! an unsent own message is cancelled. Every deletion rule lives here.

use crate::context::XmtpSharedContext;
use crate::core::events::{InternalEvent, emit_deleted_messages};
use crate::groups::{DeleteMessageError, GroupError, MlsGroup};
use openmls::group::MlsGroup as OpenMlsGroup;
use std::collections::HashMap;
use xmtp_common::snippet::Snippet;
use xmtp_db::group_intent::{IntentKind, IntentState};
use xmtp_db::group_message::{ContentType, DeliveryStatus, GroupMessageKind, StoredGroupMessage};
use xmtp_db::message_deletion::StoredMessageDeletion;
use xmtp_db::prelude::*;
use xmtp_db::{DbQuery, TransactionOutcome, XmtpMlsStorageProvider};
use xmtp_events::{ClientEvent, DeletionCause, EventContext, EventWriter};
use xmtp_proto::types::GroupId;
use zeroize::Zeroize;

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
pub(crate) enum Source {
    /// Processed from the group's topic or this installation's own echo.
    Topic {
        super_admin: bool,
        cause: DeletionCause,
    },
    /// Stored by an archive import. Proves no super-admin status.
    Archive,
}

/// An archive's `Published` deletion messages: for each target, group and
/// deleter, the earliest one's time and id. One entry per such triple, so
/// memory grows with the archive's deletions, not its messages, and each
/// message costs constant time. A pre-8.0 archive has no marks or records:
/// these restore its self-deletions, and only for a message the same import
/// creates.
#[derive(Default)]
pub(crate) struct ArchivedSelfDeletions(HashMap<DeletionKey, (i64, Vec<u8>)>);

/// A deletion's target id, group and deleter inbox.
type DeletionKey = (Vec<u8>, GroupId, String);

impl ArchivedSelfDeletions {
    /// Keep `message` when it is an archived `Published` deletion message.
    pub(crate) fn note(&mut self, message: &StoredGroupMessage) {
        if message.kind != GroupMessageKind::Application
            || message.content_type != ContentType::DeleteMessage
            || message.delivery_status != DeliveryStatus::Published
        {
            return;
        }
        let Some(target) = message.reference_id.clone() else {
            return;
        };
        let key = (target, message.group_id, message.sender_inbox_id.clone());
        let candidate = (message.sent_at_ns, message.id.clone());
        let earliest = self.0.entry(key).or_insert_with(|| candidate.clone());
        if candidate < *earliest {
            *earliest = candidate;
        }
    }

    /// Mark `row`, which this import's insert just created, when the archive
    /// holds a deletion of it by its own sender. A deletion by anyone else,
    /// or of a message the client already held, is never applied. Returns
    /// true when it marked the row.
    // implements: ARCH-028
    pub(crate) fn apply_to_inserted(
        &self,
        db: &impl DbQuery,
        row: &StoredGroupMessage,
    ) -> Result<bool, xmtp_db::StorageError> {
        let key = (row.id.clone(), row.group_id, row.sender_inbox_id.clone());
        if !self.0.contains_key(&key) {
            return Ok(false);
        }
        if may_delete(&row.group_id, &row.sender_inbox_id, false, row).is_err() {
            return Ok(false);
        }
        mark(
            db,
            &DiscardEvents,
            row.clone(),
            &row.sender_inbox_id,
            DeletionCause::Deleted,
        )
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
) -> Result<bool, xmtp_db::StorageError> {
    if deletion.kind != GroupMessageKind::Application
        || deletion.content_type != ContentType::DeleteMessage
    {
        return Ok(false);
    }
    // A topic copy is Published by definition. An archived one must say so.
    if matches!(source, Source::Archive) && deletion.delivery_status != DeliveryStatus::Published {
        return Ok(false);
    }
    // Store time already decoded the DeleteMessage and checked its target id.
    let Some(target_id) = deletion.reference_id.as_deref() else {
        return Ok(false);
    };
    let (super_admin, authenticated) = match source {
        Source::Topic { super_admin, .. } => (super_admin, true),
        Source::Archive => (false, false),
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
        // Mark from the earliest authorized record, not this one: an earlier
        // record stored while the target was absent must name the deleter.
        Source::Topic { cause, .. } => match db.first_deletion_of(&target)? {
            Some(first) => apply_record(db, events, &first, target, cause)?,
            None => false,
        },
        // An archived deletion decides nothing: only a record from the topic
        // marks the target.
        Source::Archive => apply_stored_records(db, events, &target)?,
    };
    Ok(recorded || marked)
}

/// Import only: mark a row from its earliest authorized record from the
/// topic. An archived record or deletion message never marks a row by itself;
/// an archived row is marked only by its own field 19, at its insert. Receive
/// never needs this: the topic orders a target before its deletion. Returns
/// true when it marked the row.
// implements: ARCH-028, PROC-037
pub(crate) fn apply_stored_records(
    db: &impl DbQuery,
    events: &impl EventWriter<InternalEvent>,
    row: &StoredGroupMessage,
) -> Result<bool, xmtp_db::StorageError> {
    if row.kind != GroupMessageKind::Application || row.deleted_by_inbox_id.is_some() {
        return Ok(false);
    }
    let Some(record) = db.first_deletion_of(row)? else {
        return Ok(false);
    };
    apply_record(db, events, &record, row.clone(), DeletionCause::Deleted)
}

/// Store an archive's record, then apply the topic's records to a stored
/// target. Returns true when it stored a record or marked a row.
// implements: ARCH-028
pub(crate) fn import_record(
    db: &impl DbQuery,
    events: &impl EventWriter<InternalEvent>,
    record: &StoredMessageDeletion,
) -> Result<bool, xmtp_db::StorageError> {
    let existed = db.get_message_deletion(&record.id)?;
    let stored = db.record_deletion(record)?;
    let recorded = existed.as_ref() != Some(&stored);
    let marked = match db.get_group_message(&stored.deleted_message_id)? {
        Some(target) => apply_stored_records(db, events, &target)?,
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
) -> Result<bool, xmtp_db::StorageError> {
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
) -> Result<bool, xmtp_db::StorageError> {
    // `deleted_by_inbox_id IS NULL` in the UPDATE makes the first write win.
    if db.mark_message_deleted(&target.id, deleted_by)? == 0 {
        return Ok(false);
    }
    let mut marked = target;
    // Wipe the deleted content in place; dropping the `Vec` would leave it
    // in freed memory.
    marked.decrypted_message_bytes.zeroize();
    marked.deleted_by_inbox_id = Some(deleted_by.to_owned());
    // A send this installation still has queued would publish the old
    // content from its intent: remove each one not yet attempted, in this
    // write. A saved attempt may already be on the backend and stays.
    // implements: PROC-037
    if marked.delivery_status != DeliveryStatus::Published {
        for intent in db.find_group_intents(
            marked.group_id,
            Some(vec![IntentState::ToPublish]),
            Some(vec![IntentKind::SendMessage]),
        )? {
            let names_target = crate::groups::message_id::calculate_message_id_for_intent(&intent)
                .ok()
                .flatten()
                .is_some_and(|id| id == marked.id);
            if names_target && db.prepared_envelopes(intent.id)?.is_none() {
                db.delete_group_intent(intent.id)?;
            }
        }
    }
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
