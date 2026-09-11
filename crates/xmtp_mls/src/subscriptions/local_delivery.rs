//! Local message delivery. Network receipt and MLS processing run independently.

mod acknowledgement;
mod types;

pub use acknowledgement::DeliveryAcknowledgement;
pub(crate) use types::LocalDeliveryConfig;
pub use types::{LocalDeliveryError, LocalDeliveryFilter};
pub use xmtp_db::delivery::{DeliveryCursor, DeliveryScope, DeliverySnapshot};

use acknowledgement::{AcknowledgementState, DeliverySession, PendingAcknowledgement};
use futures::{Stream, StreamExt};
use parking_lot::Mutex;
use std::{collections::VecDeque, sync::Arc};
use tokio::sync::{Notify, broadcast};
use xmtp_common::{
    StreamHandle,
    time::{now_ns, sleep},
};
use xmtp_db::{
    Fetch, StorageError,
    consent_record::{ConsentState, ConsentType},
    delivery::{DeliveryMessage, QueryDelivery},
    group::StoredGroup,
    group_message::StoredGroupMessage,
    prelude::*,
};
use xmtp_proto::types::GroupId;

use super::{LocalEvents, SubscribeError};
use crate::context::XmtpSharedContext;

type Result<T> = std::result::Result<T, LocalDeliveryError>;

/// One stored message and its unacknowledged application handoff.
pub struct LocalDeliveryItem<Context: XmtpSharedContext> {
    /// Retained, published content selected from the local database.
    pub message: StoredGroupMessage,
    /// Resume strictly after this item. Reading the cursor does not acknowledge it.
    pub cursor: DeliveryCursor,
    /// Host-side token; acknowledge only after callback return or the next-item request.
    pub acknowledgement: DeliveryAcknowledgement<Context>,
}

#[derive(Clone)]
struct Selection {
    scope: DeliveryScope,
    filter: LocalDeliveryFilter,
    revision: u64,
}

/// Change future selection while a callback or iterator holds its current item.
#[derive(Clone)]
pub struct LocalDeliveryControl {
    selection: Arc<Mutex<Selection>>,
    changed: Arc<Notify>,
    closer: Arc<dyn CloseDelivery>,
}

trait CloseDelivery: xmtp_common::MaybeSend + xmtp_common::MaybeSync {
    fn close_delivery(&self);
}

impl<C: XmtpSharedContext> CloseDelivery for DeliverySession<C> {
    fn close_delivery(&self) {
        self.close();
    }
}

impl LocalDeliveryControl {
    pub fn close(&self) {
        self.closer.close_delivery();
    }

    /// Change selected groups without consuming excluded groups' backlog (STR-092).
    pub fn update_scope(&self, scope: DeliveryScope) {
        let mut selection = self.selection.lock();
        selection.scope = scope;
        selection.revision = selection.revision.wrapping_add(1);
        self.changed.notify_one();
    }

    /// Change future selection; previously scanned rows stay consumed (STR-093).
    pub fn update_filter(&self, filter: LocalDeliveryFilter) {
        let mut selection = self.selection.lock();
        selection.filter = filter;
        selection.revision = selection.revision.wrapping_add(1);
        self.changed.notify_one();
    }

    pub fn scope(&self) -> DeliveryScope {
        self.selection.lock().scope.clone()
    }
}

/// One local reader with at most one item awaiting application acknowledgement.
pub struct LocalDelivery<Context: XmtpSharedContext> {
    session: Arc<DeliverySession<Context>>,
    config: LocalDeliveryConfig,
    control: LocalDeliveryControl,
    events: broadcast::Receiver<LocalEvents>,
    candidates: VecDeque<DeliveryMessage>,
    candidate_revision: u64,
    /// At most one handoff can wait for acknowledgement (STR-094).
    pending: Option<Arc<PendingAcknowledgement>>,
    /// `Some` selects independent replay; it never reads or changes default D.
    replay_position: Option<DeliveryCursor>,
    renewal: Option<Box<dyn StreamHandle<StreamOutput = ()>>>,
}

impl<Context> LocalDelivery<Context>
where
    Context: XmtpSharedContext + 'static,
{
    /// Open a default owner, or independent replay when `from` is supplied.
    pub(crate) fn new(
        context: Context,
        scope: DeliveryScope,
        filter: LocalDeliveryFilter,
        from: Option<DeliveryCursor>,
        config: LocalDeliveryConfig,
    ) -> Result<Self> {
        config.validate()?;
        if context.is_closed() {
            return Err(LocalDeliveryError::Closed);
        }
        // Subscribe before the first database read. Polling also covers missed and cross-process writes.
        let events = context.local_events().subscribe();
        let now = now_ns();
        let owner = if let Some(cursor) = from {
            context
                .db()
                .replay_delivery_messages(cursor, &scope, now, 0)?;
            None
        } else {
            let mut registered = context.delivery_owner().lock();
            if context.is_closed() {
                return Err(LocalDeliveryError::Closed);
            }
            let owner = context
                .db()
                .acquire_delivery_owner_with_clock(config.lease_duration_ns()?, now_ns)?;
            *registered = Some(owner);
            Some(owner)
        };
        let session = Arc::new(DeliverySession::new(
            context,
            owner,
            from.map(|cursor| cursor.database_id),
        ));
        let renewal = if owner.is_some() {
            let session = Arc::clone(&session);
            Some(Box::new(xmtp_common::spawn(None, async move {
                loop {
                    tokio::select! {
                        _ = session.cancel.cancelled() => { session.close(); break; },
                        _ = sleep(config.renew_interval) => {}
                    }
                    let result = config
                        .lease_duration_ns()
                        .and_then(|lease_ns| session.renew(lease_ns).map_err(Into::into));
                    if let Err(error) = result {
                        session.fail(error);
                        break;
                    }
                }
            })) as Box<dyn StreamHandle<StreamOutput = ()>>)
        } else {
            None
        };
        Ok(Self {
            session: Arc::clone(&session),
            config,
            control: LocalDeliveryControl {
                selection: Arc::new(Mutex::new(Selection {
                    scope,
                    filter,
                    revision: 0,
                })),
                changed: Arc::new(Notify::new()),
                closer: session,
            },
            events,
            candidates: VecDeque::new(),
            candidate_revision: 0,
            pending: None,
            replay_position: from,
            renewal,
        })
    }

    pub fn control(&self) -> LocalDeliveryControl {
        self.control.clone()
    }

    /// Close without acknowledging the last item. Its content remains eligible after restart.
    pub fn close(&mut self) {
        self.session.close();
    }

    /// Return one item with an explicit acknowledgement token for callback and host-queue adapters.
    /// The next call waits for that token. It never acknowledges the previous item itself.
    pub async fn next_delivery(&mut self) -> Result<Option<LocalDeliveryItem<Context>>> {
        let result = self.next_inner().await;
        if result.is_err() {
            self.session.close();
        }
        result
    }

    async fn next_inner(&mut self) -> Result<Option<LocalDeliveryItem<Context>>> {
        if let Some(pending) = self.pending.clone() {
            loop {
                let notified = pending.changed.notified();
                {
                    let mut state = pending.state.lock();
                    if matches!(*state, AcknowledgementState::Waiting)
                        && self.control.selection.lock().revision != pending.revision
                    {
                        *state = AcknowledgementState::Reselect;
                    }
                }
                match *pending.state.lock() {
                    AcknowledgementState::Acknowledged => {
                        if self.replay_position.is_some() {
                            self.replay_position = Some(pending.cursor);
                        }
                        self.pending = None;
                        break;
                    }
                    AcknowledgementState::Reselect => {
                        self.pending = None;
                        self.candidates.clear();
                        break;
                    }
                    AcknowledgementState::Rejected => {
                        return Err(LocalDeliveryError::AcknowledgementRejected);
                    }
                    AcknowledgementState::Failed => {
                        return Err(LocalDeliveryError::AcknowledgementFailed);
                    }
                    AcknowledgementState::Waiting | AcknowledgementState::Dispatched => {}
                }
                tokio::select! {
                    _ = notified => {},
                    _ = self.control.changed.notified() => {},
                    _ = self.session.cancel.cancelled() => return self.session.end_result(),
                }
            }
        }
        loop {
            if self.session.is_closed() {
                return self.session.end_result();
            }
            let selection = self.control.selection.lock().clone();
            if selection.revision != self.candidate_revision {
                self.candidates.clear();
                self.candidate_revision = selection.revision;
            }
            if self.candidates.is_empty() {
                let now = now_ns();
                self.candidates = {
                    let db = self.session.context.db();
                    if let Some(position) = self.replay_position {
                        db.replay_delivery_messages_bounded(
                            position,
                            &selection.scope,
                            now,
                            self.config.batch_size,
                            self.config.max_bytes,
                        )?
                    } else {
                        db.default_delivery_messages_bounded(
                            self.session.owner.ok_or(LocalDeliveryError::Closed)?,
                            &selection.scope,
                            now,
                            self.config.batch_size,
                            self.config.max_bytes,
                        )?
                    }
                    .into()
                };
                if self.candidates.is_empty() {
                    tokio::select! {
                        _ = self.session.cancel.cancelled() => return self.session.end_result(),
                        _ = self.control.changed.notified() => {},
                        _ = self.events.recv() => {},
                        _ = sleep(self.config.poll_interval) => {},
                    }
                    continue;
                }
            }
            let mut scanned = 0;
            while let Some(candidate) = self.candidates.pop_front() {
                // Re-read selection for each item. A scope change never consumes excluded groups.
                let selection = self.control.selection.lock().clone();
                if selection.revision != self.candidate_revision {
                    self.candidates.clear();
                    self.candidate_revision = selection.revision;
                    break;
                }
                self.session.check_owner()?;
                let retained = self.session.context.db().delivery_message_is_retained(
                    &candidate.message.id,
                    candidate.cursor,
                    now_ns(),
                )?;
                if !retained
                    || !matches_filter(
                        &self.session.context,
                        candidate.message.group_id,
                        &selection.filter,
                    )?
                {
                    if !self.skip_candidate(&candidate, selection.revision)? {
                        self.candidates.clear();
                        break;
                    }
                    scanned += 1;
                    if scanned >= self.config.batch_size {
                        break;
                    }
                    continue;
                }
                // A read transaction or filter check can outlive a lease. Check again at handoff.
                self.session.check_owner()?;
                let pending = Arc::new(PendingAcknowledgement::new(
                    candidate.cursor,
                    selection.revision,
                ));
                let acknowledgement = DeliveryAcknowledgement::new(
                    Arc::clone(&self.session),
                    Arc::clone(&pending),
                    candidate.message.group_id,
                    self.control.clone(),
                    candidate.message.id.clone(),
                );
                self.pending = Some(pending);
                return Ok(Some(LocalDeliveryItem {
                    message: candidate.message,
                    cursor: candidate.cursor,
                    acknowledgement,
                }));
            }
            tokio::task::yield_now().await;
        }
    }

    /// Order filtered progress with scope changes. Excluded scopes never advance D.
    fn skip_candidate(&mut self, candidate: &DeliveryMessage, revision: u64) -> Result<bool> {
        let selection = self.control.selection.lock();
        if selection.revision != revision {
            return Ok(false);
        }
        self.session.check_owner()?;
        if let Some(owner) = self.session.owner {
            self.session.context.db().acknowledge_delivery_with_clock(
                owner,
                candidate.message.group_id,
                candidate.cursor,
                now_ns,
            )?;
        } else {
            self.replay_position = Some(candidate.cursor);
        }
        Ok(true)
    }

    /// Rust iterator adapter. A new poll acknowledges the previously returned item.
    /// Dropping this stream while it is suspended at `yield` leaves that item unacknowledged.
    pub fn into_stream(self) -> impl Stream<Item = super::Result<StoredGroupMessage>> {
        self.into_cursor_stream()
            .map(|item| item.map(|item| item.message).map_err(SubscribeError::from))
    }

    /// Cursor-bearing Rust replay/default iterator with the same acknowledgement boundary.
    pub fn into_cursor_stream(self) -> impl Stream<Item = Result<DeliveryMessage>> {
        futures::stream::unfold(
            Some((self, None::<DeliveryAcknowledgement<Context>>)),
            |state| async move {
                let (mut reader, previous) = state?;
                if let Some(previous) = previous
                    && let Err(error) = previous.acknowledge()
                {
                    return Some((Err(error), None));
                }
                loop {
                    match reader.next_delivery().await {
                        Ok(Some(LocalDeliveryItem {
                            message,
                            cursor,
                            acknowledgement,
                        })) => match acknowledgement.check_owner() {
                            Ok(()) => {
                                return Some((
                                    Ok(DeliveryMessage { message, cursor }),
                                    Some((reader, Some(acknowledgement))),
                                ));
                            }
                            Err(LocalDeliveryError::SelectionChanged) => continue,
                            Err(error) => return Some((Err(error), None)),
                        },
                        Ok(None) => return None,
                        Err(error) => return Some((Err(error), None)),
                    }
                }
            },
        )
    }

    /// Read history and its resume boundary in one database snapshot (STR-099).
    pub fn history_snapshot(
        context: &Context,
        scope: &DeliveryScope,
        filter: &LocalDeliveryFilter,
        limit: u32,
    ) -> Result<DeliverySnapshot> {
        let settings = context.incoming_runtime().policy();
        Ok(context.db().delivery_history_snapshot_filtered(
            scope,
            filter,
            now_ns(),
            limit.min(settings.max_local_read_rows),
            settings.max_local_read_bytes,
        )?)
    }
}

impl<Context: XmtpSharedContext> Drop for LocalDelivery<Context> {
    fn drop(&mut self) {
        self.session.close();
        if let Some(task) = &self.renewal {
            task.end();
        }
    }
}

fn matches_filter<Context: XmtpSharedContext>(
    context: &Context,
    group_id: GroupId,
    filter: &LocalDeliveryFilter,
) -> Result<bool> {
    let db = context.db();
    if let Some(kind) = filter.conversation_type {
        let group: Option<StoredGroup> = db.fetch(&group_id)?;
        if group.is_none_or(|group| group.conversation_type != kind) {
            return Ok(false);
        }
    }
    if let Some(states) = &filter.consent_states {
        let consent = db
            .get_consent_record(hex::encode(group_id), ConsentType::ConversationId)
            .map_err(StorageError::from)?
            .map_or(ConsentState::Unknown, |record| record.state);
        if !states.contains(&consent) {
            return Ok(false);
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests;
