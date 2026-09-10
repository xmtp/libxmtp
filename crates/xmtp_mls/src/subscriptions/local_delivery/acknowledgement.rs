use parking_lot::Mutex;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;
use xmtp_common::time::now_ns;
use xmtp_db::{
    StorageError,
    delivery::{DeliveryCursor, DeliveryOwner, QueryDelivery},
};
use xmtp_proto::types::GroupId;

use super::{LocalDeliveryControl, LocalDeliveryError, Result};
use crate::context::XmtpSharedContext;

pub(super) struct DeliverySession<Context: XmtpSharedContext> {
    pub(super) context: Context,
    pub(super) owner: Option<DeliveryOwner>,
    replay_database_id: Option<[u8; 16]>,
    pub(super) cancel: CancellationToken,
    closed: AtomicBool,
    failure: Mutex<Option<LocalDeliveryError>>,
}

impl<Context: XmtpSharedContext> DeliverySession<Context> {
    pub(super) fn new(
        context: Context,
        owner: Option<DeliveryOwner>,
        replay_database_id: Option<[u8; 16]>,
    ) -> Self {
        let cancel = context.cancellation_token().child_token();
        Self {
            context,
            owner,
            replay_database_id,
            cancel,
            closed: AtomicBool::new(false),
            failure: Mutex::new(None),
        }
    }

    pub(super) fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire) || self.cancel.is_cancelled()
    }

    pub(super) fn check_owner(&self) -> Result<()> {
        if self.is_closed() {
            return Err(LocalDeliveryError::Closed);
        }
        if let Some(owner) = self.owner {
            self.context
                .db()
                .check_delivery_owner_with_clock(owner, now_ns)?;
        }
        if let Some(identity) = self.replay_database_id
            && self.context.db().stream_database_id()? != identity
        {
            return Err(StorageError::from(
                xmtp_db::stream_storage::StreamStorageError::ForeignCursor,
            )
            .into());
        }
        Ok(())
    }

    pub(super) fn renew(&self, lease_ns: i64) -> std::result::Result<(), StorageError> {
        if let Some(owner) = self.owner {
            self.context
                .db()
                .renew_delivery_owner_with_clock(owner, lease_ns, now_ns)?;
        }
        Ok(())
    }

    pub(super) fn close(&self) {
        if self.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        self.cancel.cancel();
        if let Some(owner) = self.owner {
            let mut registered = self.context.delivery_owner().lock();
            match self.context.db().release_delivery_owner(owner) {
                Ok(()) => {
                    if *registered == Some(owner) {
                        *registered = None;
                    }
                }
                Err(error) => {
                    tracing::warn!(%error, "Failed to release the message delivery owner")
                }
            }
        }
    }

    pub(super) fn fail(&self, error: LocalDeliveryError) {
        *self.failure.lock() = Some(error);
        self.close();
    }

    pub(super) fn end_result<T>(&self) -> Result<Option<T>> {
        match self.failure.lock().take() {
            Some(error) => Err(error),
            None => Ok(None),
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum AcknowledgementState {
    Waiting,
    Dispatched,
    Reselect,
    Acknowledged,
    Rejected,
    Failed,
}

pub(super) struct PendingAcknowledgement {
    pub(super) cursor: DeliveryCursor,
    pub(super) revision: u64,
    pub(super) state: Mutex<AcknowledgementState>,
    pub(super) changed: Notify,
}

impl PendingAcknowledgement {
    pub(super) fn new(cursor: DeliveryCursor, revision: u64) -> Self {
        Self {
            cursor,
            revision,
            state: Mutex::new(AcknowledgementState::Waiting),
            changed: Notify::new(),
        }
    }
}

/// An opaque, single-item token. Dropping it never acknowledges delivery.
pub struct DeliveryAcknowledgement<Context: XmtpSharedContext> {
    session: Arc<DeliverySession<Context>>,
    pending: Arc<PendingAcknowledgement>,
    group_id: GroupId,
    control: LocalDeliveryControl,
    message_id: Vec<u8>,
}

impl<Context: XmtpSharedContext> DeliveryAcknowledgement<Context> {
    pub(super) fn new(
        session: Arc<DeliverySession<Context>>,
        pending: Arc<PendingAcknowledgement>,
        group_id: GroupId,
        control: LocalDeliveryControl,
        message_id: Vec<u8>,
    ) -> Self {
        Self {
            session,
            pending,
            group_id,
            control,
            message_id,
        }
    }

    /// Call on the host thread immediately before a callback that was queued earlier.
    /// SelectionChanged discards the queued item. Continue reading without acknowledging it.
    pub fn check_owner(&self) -> Result<()> {
        self.begin_dispatch(&mut self.pending.state.lock())
    }

    fn begin_dispatch(&self, state: &mut AcknowledgementState) -> Result<()> {
        match *state {
            AcknowledgementState::Reselect => return Err(LocalDeliveryError::SelectionChanged),
            AcknowledgementState::Rejected => {
                return Err(LocalDeliveryError::AcknowledgementRejected);
            }
            AcknowledgementState::Failed => return Err(LocalDeliveryError::AcknowledgementFailed),
            AcknowledgementState::Dispatched | AcknowledgementState::Acknowledged => {
                return self.session.check_owner();
            }
            AcknowledgementState::Waiting => {}
        }
        // This lock makes dispatch and a scope change have one order.
        let selection = self.control.selection.lock();
        self.session.check_owner()?;
        let retained = self.session.context.db().delivery_message_is_retained(
            &self.message_id,
            self.pending.cursor,
            now_ns(),
        )?;
        if selection.revision != self.pending.revision
            || !retained
            || !super::matches_filter(&self.session.context, self.group_id, &selection.filter)?
        {
            *state = AcknowledgementState::Reselect;
            self.pending.changed.notify_one();
            return Err(LocalDeliveryError::SelectionChanged);
        }
        *state = AcknowledgementState::Dispatched;
        Ok(())
    }

    /// Persist after the callback returns successfully, or at the next iterator request.
    pub fn acknowledge(&self) -> Result<()> {
        let mut state = self.pending.state.lock();
        match *state {
            AcknowledgementState::Acknowledged => return Ok(()),
            AcknowledgementState::Rejected => {
                return Err(LocalDeliveryError::AcknowledgementRejected);
            }
            AcknowledgementState::Failed => return Err(LocalDeliveryError::AcknowledgementFailed),
            AcknowledgementState::Reselect => return Err(LocalDeliveryError::SelectionChanged),
            AcknowledgementState::Waiting | AcknowledgementState::Dispatched => {}
        }
        let result = self.begin_dispatch(&mut state).and_then(|()| {
            self.session.check_owner()?;
            if let Some(owner) = self.session.owner {
                self.session.context.db().acknowledge_delivery_with_clock(
                    owner,
                    self.group_id,
                    self.pending.cursor,
                    now_ns,
                )?;
            }
            Ok(())
        });
        match result {
            Err(LocalDeliveryError::SelectionChanged) => Err(LocalDeliveryError::SelectionChanged),
            Ok(()) => {
                *state = AcknowledgementState::Acknowledged;
                self.pending.changed.notify_one();
                Ok(())
            }
            Err(error) => {
                *state = AcknowledgementState::Failed;
                self.pending.changed.notify_one();
                self.session.close();
                Err(error)
            }
        }
    }

    /// A failed callback stops this reader and leaves its default delivery position unchanged.
    pub fn reject(&self) {
        let mut state = self.pending.state.lock();
        if matches!(*state, AcknowledgementState::Waiting)
            && self.control.selection.lock().revision != self.pending.revision
        {
            *state = AcknowledgementState::Reselect;
            self.pending.changed.notify_one();
        } else if matches!(
            *state,
            AcknowledgementState::Waiting | AcknowledgementState::Dispatched
        ) {
            *state = AcknowledgementState::Rejected;
            self.pending.changed.notify_one();
            self.session.close();
        }
    }
}

impl<Context: XmtpSharedContext> Drop for DeliveryAcknowledgement<Context> {
    fn drop(&mut self) {
        self.reject();
    }
}
