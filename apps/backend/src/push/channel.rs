//! Provider outcomes and the immutable configuration used by one delivery.

use crate::db::PushChannel;
use std::sync::Arc;
use xmtp_common::time::Duration;
use xmtp_push_types::PushPayload;

pub(crate) mod http;

pub(crate) const ATTEMPT_TIMEOUT: Duration = Duration::from_secs(10);
pub(crate) const RETRY_DELAY: Duration = Duration::from_secs(1);
pub(crate) const MAX_RETRY_DELAY: Duration = Duration::from_secs(300);

/// Do not format this type: its fields contain recipient secrets and addresses.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct DeliveryConfig {
    pub recipient_id: Vec<u8>,
    pub channel: PushChannel,
    pub delivery: String,
    pub signing_key: Option<Vec<u8>>,
    pub metadata: Vec<u8>,
}

#[derive(Clone)]
pub(crate) struct Delivery {
    pub config: DeliveryConfig,
    pub payload: PushPayload,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Delivered,
    Transient {
        retry_after: Option<Duration>,
    },
    Rejected,
    #[allow(dead_code)] // APNs and FCM use this outcome when their senders land.
    Mismatch,
    #[allow(dead_code)] // HTTPS reaches terminal state through repeated GoneTransient.
    Terminal,
    GoneTransient,
}

#[async_trait::async_trait]
pub(crate) trait Sender: Send + Sync {
    async fn send(&self, delivery: &Delivery) -> Outcome;
}

#[derive(Clone, Default)]
pub(crate) struct Senders(pub [Option<Arc<dyn Sender>>; 3]);

impl Senders {
    pub fn get(&self, channel: PushChannel) -> Option<Arc<dyn Sender>> {
        self.0[channel as usize - 1].clone()
    }
}

impl PushChannel {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Apns => "apns",
            Self::Fcm => "fcm",
            Self::Http => "http",
        }
    }
}
