use parking_lot::Mutex;
use std::sync::Arc;
use tokio::sync::Mutex as AsyncMutex;
use xmtp_events::Subscription;
use xmtp_mls::subscriptions::internal::InternalEvent;

use super::ClientEvent;
use crate::XmtpError;

#[derive(uniffi::Object)]
pub struct EventReader {
    subscription: Arc<Subscription<InternalEvent>>,
    read_lock: AsyncMutex<()>,
    ended: Mutex<bool>,
    #[cfg(test)]
    pub(crate) handoff_gate: Mutex<Option<Arc<crate::reader::HandoffGate>>>,
}

impl EventReader {
    pub(crate) fn new(subscription: Subscription<InternalEvent>) -> Arc<Self> {
        Arc::new(Self {
            subscription: Arc::new(subscription),
            read_lock: AsyncMutex::new(()),
            ended: Mutex::new(false),
            #[cfg(test)]
            handoff_gate: Mutex::new(None),
        })
    }
}

impl Drop for EventReader {
    fn drop(&mut self) {
        self.subscription.close();
    }
}

#[xmtp_macro::sdk_export]
impl EventReader {
    /// The lease keeps a taken event in the 1024-event bound until this read returns.
    pub async fn next(&self) -> Result<Option<ClientEvent>, XmtpError> {
        let _read = self.read_lock.lock().await;
        if *self.ended.lock() {
            return Ok(None);
        }
        let Some(lease) = self.subscription.next_for_callback().await else {
            return Ok(None);
        };
        #[cfg(test)]
        let gate = self.handoff_gate.lock().take();
        #[cfg(test)]
        if let Some(gate) = gate {
            gate.arrived.notify_one();
            gate.release.notified().await;
        }
        let event = lease.event.client.clone().map(Into::into);
        if *self.ended.lock() || self.subscription.is_closed() {
            return Ok(None);
        }
        Ok(event)
    }

    pub async fn end(&self) -> Result<(), XmtpError> {
        *self.ended.lock() = true;
        self.subscription.close();
        Ok(())
    }
}
