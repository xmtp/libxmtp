use parking_lot::Mutex;
use std::sync::Arc;
use tokio::sync::Mutex as AsyncMutex;
use xmtp_mls::subscriptions::{
    local_delivery::{
        DeliveryAcknowledgement, DeliveryScope, LocalDeliveryError, LocalDeliveryFilter,
    },
    message_reader::{MessageReader as CoreMessageReader, MessageReaderControl},
};
use xmtp_proto::types::GroupId;

use crate::{Message, XmtpError};

#[derive(uniffi::Object)]
pub struct MessageReader {
    reader: AsyncMutex<CoreMessageReader<xmtp_mls::MlsContext>>,
    control: MessageReaderControl,
    state: Mutex<ReaderState>,
    client_key: u64,
    #[cfg(test)]
    pub(crate) handoff_gate: Mutex<Option<Arc<HandoffGate>>>,
}

struct ReaderState {
    ended: bool,
    previous: Option<DeliveryAcknowledgement<xmtp_mls::MlsContext>>,
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
            context,
            DeliveryScope::Groups(vec![group_id]),
            LocalDeliveryFilter::default(),
            None,
        )
        .map_err(XmtpError::unknown)?;
        let control = reader.control();
        Ok(Arc::new(Self {
            reader: AsyncMutex::new(reader),
            control,
            state: Mutex::new(ReaderState {
                ended: false,
                previous: None,
            }),
            client_key,
            #[cfg(test)]
            handoff_gate: Mutex::new(None),
        }))
    }

    #[cfg(test)]
    pub(crate) fn is_ended_for_test(&self) -> bool {
        self.state.lock().ended
    }
}

impl Drop for MessageReader {
    fn drop(&mut self) {
        self.control.close();
    }
}

#[xmtp_macro::sdk_export]
impl MessageReader {
    /// Acknowledge the previous item when the next read starts.
    pub async fn next(&self) -> Result<Option<Message>, XmtpError> {
        let mut reader = self.reader.lock().await;
        {
            let mut state = self.state.lock();
            if state.ended {
                return Ok(None);
            }
            if let Some(previous) = state.previous.take()
                && let Err(error) = previous.acknowledge()
                && !selection_changed(&error)
            {
                return Err(XmtpError::unknown(error));
            }
        }
        loop {
            let item = match reader.next_delivery().await {
                Ok(item) => item,
                Err(_) if self.state.lock().ended => return Ok(None),
                Err(error) => return Err(XmtpError::unknown(error)),
            };
            let Some(item) = item else { return Ok(None) };
            #[cfg(test)]
            let gate = self.handoff_gate.lock().take();
            #[cfg(test)]
            if let Some(gate) = gate {
                gate.arrived.notify_one();
                gate.release.notified().await;
            }
            if self.state.lock().ended {
                item.acknowledgement.reject();
                return Ok(None);
            }
            match item.acknowledgement.check_owner() {
                Ok(()) => {}
                Err(error) if selection_changed(&error) => {
                    item.acknowledgement.reject();
                    continue;
                }
                Err(_) if self.state.lock().ended => {
                    item.acknowledgement.reject();
                    return Ok(None);
                }
                Err(error) => return Err(XmtpError::unknown(error)),
            }
            let message = Message::from_stored(item.message, self.client_key)?;
            let mut state = self.state.lock();
            if state.ended {
                item.acknowledgement.reject();
                return Ok(None);
            }
            state.previous = Some(item.acknowledgement);
            return Ok(Some(message));
        }
    }

    pub async fn end(&self) -> Result<(), XmtpError> {
        {
            let mut state = self.state.lock();
            state.ended = true;
            self.control.close();
            if let Some(previous) = state.previous.take() {
                previous.reject();
            }
        }
        // Wait for a pending handoff before reporting that the reader ended.
        let _reader = self.reader.lock().await;
        Ok(())
    }
}

pub(crate) fn selection_changed(error: &LocalDeliveryError) -> bool {
    matches!(error, LocalDeliveryError::SelectionChanged)
}
