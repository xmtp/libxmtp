//! Message delivery tokens kept through the mobile SDK consumer boundary.

mod status;
pub use status::*;

#[cfg(test)]
mod tests;

use std::sync::Arc;
use tokio::sync::{Mutex, oneshot};
use xmtp_db::{StorageError, delivery::QueryDelivery, stream_storage::StreamStorageError};
use xmtp_mls::{
    MlsContext,
    context::XmtpSharedContext,
    subscriptions::{
        SubscribeError,
        incoming::IncomingCoordinator,
        local_delivery::{
            DeliveryAcknowledgement, DeliveryCursor, DeliveryScope, LocalDelivery,
            LocalDeliveryError, LocalDeliveryFilter, LocalDeliveryItem,
        },
        message_reader::{MessageReader, MessageReaderControl},
    },
};

use super::{
    FfiConsentState, FfiConversationType, FfiMessage, FfiMessageCallback, FfiStreamCloser,
};
use crate::FfiError;

/// A position in one database's local delivery order, not a network cursor.
#[derive(Clone, uniffi::Record)]
pub struct FfiDeliveryCursor {
    /// The 16-byte database identity. Cursors from another database are invalid.
    pub database_id: Vec<u8>,
    /// Local delivery sequence. Zero starts replay before the first retained item.
    pub delivery_sequence: u64,
}

impl From<DeliveryCursor> for FfiDeliveryCursor {
    fn from(cursor: DeliveryCursor) -> Self {
        Self {
            database_id: cursor.database_id.to_vec(),
            delivery_sequence: cursor.delivery_sequence,
        }
    }
}

impl TryFrom<FfiDeliveryCursor> for DeliveryCursor {
    type Error = FfiError;

    fn try_from(cursor: FfiDeliveryCursor) -> Result<Self, Self::Error> {
        let database_id = cursor
            .database_id
            .try_into()
            .map_err(|_| FfiError::from(StorageError::from(StreamStorageError::ForeignCursor)))?;
        Ok(Self {
            database_id,
            delivery_sequence: cursor.delivery_sequence,
        })
    }
}

/// None selects all conversations. An empty list selects none.
pub(super) fn delivery_scope(group_ids: Option<Vec<Vec<u8>>>) -> Result<DeliveryScope, FfiError> {
    match group_ids {
        None => Ok(DeliveryScope::All),
        Some(groups) => groups
            .into_iter()
            .map(xmtp_proto::types::GroupId::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map(DeliveryScope::Groups)
            .map_err(|error| FfiError::generic(error.to_string())),
    }
}

/// Translate selection fields without changing durable delivery progress.
pub(super) fn delivery_filter(
    conversation_type: Option<FfiConversationType>,
    consent_states: Option<Vec<FfiConsentState>>,
) -> LocalDeliveryFilter {
    LocalDeliveryFilter {
        conversation_type: conversation_type.map(Into::into),
        consent_states: consent_states.map(|states| states.into_iter().map(Into::into).collect()),
    }
}

/// One shared token for one item. Releasing it does not acknowledge the item.
#[derive(uniffi::Object)]
pub struct FfiDeliveryAcknowledgement {
    inner: DeliveryAcknowledgement<MlsContext>,
}

#[uniffi::export]
impl FfiDeliveryAcknowledgement {
    /// Check immediately before the app callback or iterator handoff, not before queue insertion.
    /// False means the selection changed. Discard this item and wait for a new selection.
    pub fn check_owner(&self) -> Result<bool, FfiError> {
        match self.inner.check_owner() {
            Ok(()) => Ok(true),
            Err(LocalDeliveryError::SelectionChanged) => Ok(false),
            Err(error) => Err(error.into()),
        }
    }

    /// Persist only after callback success or when the iterator requests its next item.
    pub fn acknowledge(&self) -> Result<(), FfiError> {
        self.inner.acknowledge().map_err(Into::into)
    }

    /// Stop this delivery without advancing the saved delivery position.
    pub fn reject(&self) {
        self.inner.reject();
    }
}

/// One selected message. SDK queues must retain its token until app handoff.
#[derive(Clone, uniffi::Record)]
pub struct FfiMessageDelivery {
    pub message: FfiMessage,
    /// The selected item's local position; selection alone does not advance D.
    pub cursor: FfiDeliveryCursor,
    /// Check ownership at handoff. Acknowledge only at the SDK consumer boundary.
    pub acknowledgement: Arc<FfiDeliveryAcknowledgement>,
}

impl From<LocalDeliveryItem<MlsContext>> for FfiMessageDelivery {
    fn from(item: LocalDeliveryItem<MlsContext>) -> Self {
        Self {
            message: item.message.into(),
            cursor: item.cursor.into(),
            acknowledgement: Arc::new(FfiDeliveryAcknowledgement {
                inner: item.acknowledgement,
            }),
        }
    }
}

/// One history item and its local replay position. History reads do not acknowledge it.
#[derive(Clone, uniffi::Record)]
pub struct FfiHistoryMessage {
    pub message: FfiMessage,
    pub cursor: FfiDeliveryCursor,
}

/// Messages and replay position read from the same database snapshot.
#[derive(Clone, uniffi::Record)]
pub struct FfiMessageHistorySnapshot {
    pub messages: Vec<FfiHistoryMessage>,
    /// Start replay here to receive items committed after this snapshot.
    pub cursor: FfiDeliveryCursor,
}

/// Keep selection and the history/live boundary in one database snapshot.
pub(super) fn history_snapshot(
    context: &MlsContext,
    scope: DeliveryScope,
    filter: LocalDeliveryFilter,
    limit: u32,
) -> Result<FfiMessageHistorySnapshot, FfiError> {
    let snapshot = LocalDelivery::history_snapshot(context, &scope, &filter, limit)?;
    Ok(FfiMessageHistorySnapshot {
        messages: snapshot
            .messages
            .into_iter()
            .map(|item| FfiHistoryMessage {
                message: item.message.into(),
                cursor: item.cursor.into(),
            })
            .collect(),
        cursor: snapshot.cursor.into(),
    })
}

/// Create a database-bound cursor before all retained local deliveries.
pub(super) fn beginning_cursor(context: &MlsContext) -> Result<FfiDeliveryCursor, FfiError> {
    Ok(FfiDeliveryCursor {
        database_id: context.db().stream_database_id()?.to_vec(),
        delivery_sequence: 0,
    })
}

/// Explicit local reader. SDK wrappers keep acknowledgement tokens away from application code.
#[derive(uniffi::Object)]
pub struct FfiMessageReader {
    reader: Mutex<MessageReader<MlsContext>>,
    control: MessageReaderControl,
}

impl FfiMessageReader {
    /// Enable bidi receipt. A supplied cursor opens replay without advancing default D.
    pub(super) fn open(
        context: MlsContext,
        scope: DeliveryScope,
        filter: LocalDeliveryFilter,
        from: Option<FfiDeliveryCursor>,
    ) -> Result<Arc<Self>, FfiError> {
        let from = from.map(TryInto::try_into).transpose()?;
        let _coordinator = IncomingCoordinator::enable_bidi_transport(&context);
        let reader = MessageReader::new(context, scope, filter, from)?;
        let control = reader.control();
        Ok(Arc::new(Self {
            reader: Mutex::new(reader),
            control,
        }))
    }
}

impl Drop for FfiMessageReader {
    fn drop(&mut self) {
        self.control.close();
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl FfiMessageReader {
    /// Select one item without acknowledging it or ending its host ownership.
    pub async fn next_delivery(&self) -> Result<Option<FfiMessageDelivery>, FfiError> {
        self.reader
            .lock()
            .await
            .next_delivery()
            .await
            .map(|item| item.map(Into::into))
            .map_err(Into::into)
    }

    /// Release the receipt lease and fence host delivery without acknowledging pending items.
    pub fn end(&self) {
        self.control.close();
    }

    /// Replace conversation scope and invalidate any stale queued selection.
    pub fn update_scope(&self, group_ids: Option<Vec<Vec<u8>>>) -> Result<(), FfiError> {
        self.control.update_scope(delivery_scope(group_ids)?);
        Ok(())
    }

    /// Replace delivery filters without changing the reader's saved position.
    pub fn update_filter(
        &self,
        conversation_type: Option<FfiConversationType>,
        consent_states: Option<Vec<FfiConsentState>>,
    ) {
        self.control
            .update_filter(delivery_filter(conversation_type, consent_states));
    }

    /// Read network receipt and processing progress, independently of app acknowledgement.
    pub fn catch_up_snapshot(&self) -> FfiMessageCatchUpSnapshot {
        self.control.catch_up_snapshot().into()
    }

    /// Wait for status to change or the reader to close, then return its latest status.
    pub async fn catch_up_changed(&self) -> FfiMessageCatchUpSnapshot {
        self.control.changed().await;
        self.catch_up_snapshot()
    }
}

struct MessageStreamClosed(Arc<dyn FfiMessageCallback>);

impl Drop for MessageStreamClosed {
    fn drop(&mut self) {
        self.0.on_close();
    }
}

/// Keep bidi receipt active while the SDK owns one delivery token.
/// Calling the SDK callback only transfers ownership; it does not acknowledge delivery.
pub(super) fn stream_messages(
    context: MlsContext,
    scope: DeliveryScope,
    filter: LocalDeliveryFilter,
    callback: Arc<dyn FfiMessageCallback>,
) -> FfiStreamCloser {
    let _coordinator = IncomingCoordinator::enable_bidi_transport(&context);
    let reader = MessageReader::new(context, scope, filter, None);
    let control = reader.as_ref().ok().map(MessageReader::control);
    let (ready, ready_rx) = oneshot::channel();
    let closed = MessageStreamClosed(Arc::clone(&callback));
    let handle = xmtp_common::spawn(Some(ready_rx), async move {
        let _closed = closed;
        let _ = ready.send(());
        let mut reader = match reader {
            Ok(reader) => reader,
            Err(error) => {
                callback.on_error(error.into());
                return Ok::<(), SubscribeError>(());
            }
        };
        loop {
            match reader.next_delivery().await {
                Ok(Some(item)) => {
                    let delivery = FfiMessageDelivery::from(item);
                    let acknowledgement = Arc::clone(&delivery.acknowledgement);
                    if let Err(error) = callback.on_message(delivery) {
                        acknowledgement.reject();
                        callback.on_error(error);
                        break;
                    }
                }
                Ok(None) => break,
                Err(error) => {
                    callback.on_error(error.into());
                    break;
                }
            }
        }
        reader.close();
        Ok(())
    });
    let mut closer = FfiStreamCloser::new(handle);
    closer.message_control = control;
    closer
}
