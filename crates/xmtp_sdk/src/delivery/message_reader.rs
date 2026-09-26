use parking_lot::Mutex;
use std::sync::Arc;
use tokio::sync::Mutex as AsyncMutex;
use tokio_util::sync::CancellationToken;
use xmtp_mls::context::XmtpSharedContext;
use xmtp_mls::messages::enrichment::enrich_messages_with_stored;
use xmtp_mls::subscriptions::{
    local_delivery::{
        DeliveryAcknowledgement, DeliveryScope, LocalDeliveryError, LocalDeliveryFilter,
    },
    message_reader::{MessageReader as CoreMessageReader, MessageReaderControl},
};
use xmtp_proto::types::GroupId;

use crate::{ConnectionState, Message, XmtpError, conversation::on_sdk_worker};

#[derive(uniffi::Object)]
pub struct MessageReader {
    reader: Arc<AsyncMutex<CoreMessageReader<xmtp_mls::MlsContext>>>,
    request_lock: AsyncMutex<()>,
    control: MessageReaderControl,
    state: Arc<Mutex<ReaderState>>,
    context: xmtp_mls::MlsContext,
    client_key: u64,
    #[cfg(test)]
    pub(crate) handoff_gate: Arc<Mutex<Option<Arc<HandoffGate>>>>,
    #[cfg(test)]
    corrupt_next_message: std::sync::atomic::AtomicBool,
}

struct ReaderState {
    ended: bool,
    previous: Option<DeliveryAcknowledgement<xmtp_mls::MlsContext>>,
    pending: Option<PendingMessage>,
}

struct PendingMessage {
    message: Message,
    acknowledgement: DeliveryAcknowledgement<xmtp_mls::MlsContext>,
}

#[cfg(test)]
pub(crate) struct HandoffGate {
    pub(crate) arrived: tokio::sync::Notify,
    pub(crate) release: tokio::sync::Notify,
}

impl MessageReader {
    pub(crate) fn open(
        context: xmtp_mls::MlsContext,
        group_id: GroupId,
        client_key: u64,
    ) -> Result<Arc<Self>, XmtpError> {
        let reader = CoreMessageReader::new(
            context.clone(),
            DeliveryScope::Groups(vec![group_id]),
            LocalDeliveryFilter::default(),
            None,
        )
        .map_err(super::delivery_error)?;
        let control = reader.control();
        Ok(Arc::new(Self {
            reader: Arc::new(AsyncMutex::new(reader)),
            request_lock: AsyncMutex::new(()),
            control,
            state: Arc::new(Mutex::new(ReaderState {
                ended: false,
                previous: None,
                pending: None,
            })),
            context,
            client_key,
            #[cfg(test)]
            handoff_gate: Arc::new(Mutex::new(None)),
            #[cfg(test)]
            corrupt_next_message: std::sync::atomic::AtomicBool::new(false),
        }))
    }

    #[cfg(test)]
    pub(crate) fn is_ended_for_test(&self) -> bool {
        self.state.lock().ended
    }

    #[cfg(test)]
    pub(crate) fn update_scope_for_test(&self, group_ids: Vec<GroupId>) {
        self.control.update_scope(DeliveryScope::Groups(group_ids));
    }

    #[cfg(test)]
    pub(crate) fn control_for_test(&self) -> MessageReaderControl {
        self.control.clone()
    }

    #[cfg(test)]
    pub(crate) fn corrupt_next_message_for_test(&self) {
        self.corrupt_next_message
            .store(true, std::sync::atomic::Ordering::Release);
    }
}

impl Drop for MessageReader {
    fn drop(&mut self) {
        self.control.close();
    }
}

#[xmtp_macro::sdk_export]
impl MessageReader {
    /// Acknowledge the prior item only when this read starts.
    pub async fn next(&self) -> Result<Option<Message>, XmtpError> {
        let _request = self.request_lock.lock().await;
        let request_cancel = CancellationToken::new();
        let _cancel_on_drop = super::CancelReadOnDrop(request_cancel.clone());
        let reader = self.reader.clone();
        let state = self.state.clone();
        let control = self.control.clone();
        let client_key = self.client_key;
        let context = self.context.clone();
        #[cfg(test)]
        let handoff_gate = self.handoff_gate.clone();
        #[cfg(test)]
        let mut corrupt_next_message = self
            .corrupt_next_message
            .swap(false, std::sync::atomic::Ordering::AcqRel);
        on_sdk_worker(self.context.clone(), async move {
            let mut reader = reader.lock().await;
            if request_cancel.is_cancelled() {
                return Ok(false);
            }
            {
                let mut state = state.lock();
                if state.ended {
                    return Ok(false);
                }
                if let Some(previous) = state.previous.take()
                    && let Err(error) = previous.acknowledge()
                    && !selection_changed(&error)
                {
                    state.ended = true;
                    control.close();
                    return Err(super::delivery_error(error));
                }
                if let Some(pending) = state.pending.take() {
                    match pending.acknowledgement.check_owner() {
                        Ok(()) => {
                            state.pending = Some(pending);
                            return Ok(true);
                        }
                        Err(error) if selection_changed(&error) => {
                            pending.acknowledgement.reject();
                        }
                        Err(error) => {
                            state.ended = true;
                            control.close();
                            return Err(super::delivery_error(error));
                        }
                    }
                }
            }
            loop {
                let item = match tokio::select! {
                    biased;
                    _ = request_cancel.cancelled() => return Ok(false),
                    result = reader.next_delivery() => result,
                } {
                    Ok(item) => item,
                    Err(_) if state.lock().ended => return Ok(false),
                    Err(error) => {
                        state.lock().ended = true;
                        control.close();
                        return Err(super::delivery_error(error));
                    }
                };
                let Some(item) = item else { return Ok(false) };
                #[cfg(test)]
                let mut item = item;
                #[cfg(test)]
                if corrupt_next_message {
                    item.message.id.clear();
                    corrupt_next_message = false;
                }
                #[cfg(test)]
                let gate = handoff_gate.lock().take();
                #[cfg(test)]
                if let Some(gate) = gate {
                    gate.arrived.notify_one();
                    gate.release.notified().await;
                }
                if state.lock().ended {
                    item.acknowledgement.reject();
                    return Ok(false);
                }
                match item.acknowledgement.check_owner() {
                    Ok(()) => {}
                    Err(error) if selection_changed(&error) => {
                        item.acknowledgement.reject();
                        continue;
                    }
                    Err(_) if state.lock().ended => {
                        item.acknowledgement.reject();
                        return Ok(false);
                    }
                    Err(error) => {
                        state.lock().ended = true;
                        control.close();
                        return Err(super::delivery_error(error));
                    }
                }
                let enriched = match enrich_messages_with_stored(
                    context.db(),
                    &item.message.group_id,
                    vec![item.message.clone()],
                ) {
                    Ok(enriched) => enriched,
                    Err(error) => {
                        state.lock().ended = true;
                        control.close();
                        item.acknowledgement.reject();
                        return Err(super::enrichment_error(error));
                    }
                };
                let message = match enriched.into_iter().next() {
                    Some(value) => Message::from_enriched(
                        value.stored,
                        value.decoded,
                        value.parent_stored,
                        client_key,
                    )
                    .or_else(|_| Message::from_stored(item.message.clone(), client_key)),
                    None => Message::from_stored(item.message.clone(), client_key),
                };
                let message = match message {
                    Ok(message) => message,
                    Err(error) => {
                        state.lock().ended = true;
                        control.close();
                        item.acknowledgement.reject();
                        return Err(error);
                    }
                };
                let mut state = state.lock();
                if state.ended {
                    item.acknowledgement.reject();
                    return Ok(false);
                }
                state.pending = Some(PendingMessage {
                    message,
                    acknowledgement: item.acknowledgement,
                });
                return Ok(true);
            }
        })
        .await
        .and_then(|ready| {
            if !ready {
                return Ok(None);
            }
            let mut state = self.state.lock();
            if state.ended {
                return Ok(None);
            }
            let pending = state
                .pending
                .take()
                .ok_or_else(|| XmtpError::unknown("message handoff missing"))?;
            state.previous = Some(pending.acknowledgement);
            Ok(Some(pending.message))
        })
    }

    pub async fn end(&self) -> Result<(), XmtpError> {
        let reader = self.reader.clone();
        let state = self.state.clone();
        let control = self.control.clone();
        on_sdk_worker(self.context.clone(), async move {
            {
                let mut state = state.lock();
                state.ended = true;
                control.close();
                if let Some(previous) = state.previous.take() {
                    previous.reject();
                }
                if let Some(pending) = state.pending.take() {
                    pending.acknowledgement.reject();
                }
            }
            let _reader = reader.lock().await;
            Ok(())
        })
        .await
    }

    pub fn connection_state(&self) -> ConnectionState {
        self.control.catch_up_snapshot().connection.into()
    }

    pub async fn connection_state_changed(
        &self,
        previous: ConnectionState,
    ) -> Result<ConnectionState, XmtpError> {
        let control = self.control.clone();
        let mut changes = control.observer();
        on_sdk_worker(self.context.clone(), async move {
            loop {
                let current = control.catch_up_snapshot().connection.into();
                if current != previous || current == ConnectionState::Closed {
                    return Ok(current);
                }
                changes.changed().await;
            }
        })
        .await
    }
}

pub(crate) fn selection_changed(error: &LocalDeliveryError) -> bool {
    matches!(error, LocalDeliveryError::SelectionChanged)
}
