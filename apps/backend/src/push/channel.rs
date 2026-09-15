//! Provider outcomes and the immutable configuration used by one delivery.

use crate::db::PushChannel;
use std::sync::Arc;
use xmtp_common::time::Duration;
use xmtp_push_types::PushPayload;

pub(crate) mod apns;
pub(crate) mod fcm;
pub(crate) mod http;

pub(crate) const ATTEMPT_TIMEOUT: Duration = Duration::from_secs(10);
pub(crate) const RETRY_DELAY: Duration = Duration::from_secs(1);
pub(crate) const MAX_RETRY_DELAY: Duration = Duration::from_secs(300);
pub(super) const MAX_RESPONSE_BYTES: usize = 16 * 1024;

/// Do not format this type: its fields contain recipient secrets and addresses.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct DeliveryConfig {
    pub recipient_id: Vec<u8>,
    pub secret_hash: Vec<u8>,
    pub channel: PushChannel,
    pub delivery: String,
    pub signing_key: Option<Vec<u8>>,
}

#[derive(Clone)]
pub(crate) struct Delivery {
    pub config: DeliveryConfig,
    pub payload: PushPayload,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Delivered,
    Transient { retry_after: Option<Duration> },
    Rejected,
    Unconfigured,
    Mismatch,
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
    /// Construct exactly the configured channels and validate credentials before
    /// starting any dispatcher tasks. Errors name config keys, never values.
    pub fn new(
        config: &crate::config::push::PushConfig,
    ) -> Result<Self, crate::config::ConfigError> {
        let mut senders = Self::default();
        if let Some(config) = &config.apns {
            senders.0[0] = Some(Arc::new(apns::ApnsSender::new(config)?));
        }
        if let Some(config) = &config.fcm {
            senders.0[1] = Some(Arc::new(fcm::FcmSender::new(config)?));
        }
        if let Some(config) = &config.http {
            senders.0[2] = Some(Arc::new(http::HttpSender::new(config)));
        }
        Ok(senders)
    }

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
