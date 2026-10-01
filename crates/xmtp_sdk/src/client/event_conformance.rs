//! Read and fill the real listener queue in conformance builds only.
use super::*;
use xmtp_events::EventWriter;

#[derive(uniffi::Record)]
pub struct SdkConformanceListenerCounts {
    pub registered: bool,
    pub queued: u64,
    pub in_flight: u64,
    pub discarded: u64,
}

#[xmtp_macro::sdk_export]
impl Client {
    pub fn sdk_conformance_emit_hmac_events(&self, count: u32) -> Result<u32, XmtpError> {
        let _call = self.ensure_open()?;
        for _ in 0..count {
            self.inner.context.events().emit(
                Some(xmtp_events::ClientEvent::HmacKeysUpdated(
                    xmtp_events::HmacKeysUpdated,
                )),
                None,
            );
        }
        Ok(count)
    }

    pub fn sdk_conformance_listener_counts(
        &self,
        id: crate::ListenerId,
    ) -> SdkConformanceListenerCounts {
        let counts = self.listeners.conformance_queue_counts(id);
        let (queued, in_flight, discarded) = counts.unwrap_or_default();
        SdkConformanceListenerCounts {
            registered: counts.is_some(),
            queued: queued as u64,
            in_flight: in_flight as u64,
            discarded,
        }
    }
}
