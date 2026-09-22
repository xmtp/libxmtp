use thiserror::Error;
use xmtp_common::{ErrorCode, RetryableError, time::Duration};
use xmtp_db::StorageError;

pub use xmtp_db::delivery::DeliveryFilter as LocalDeliveryFilter;

use crate::subscriptions::policy::StreamPolicy;

const LEASE_RENEWAL_DIVISOR: u32 = 3;

/// Bounds local reads and maintains exclusive default-consumer ownership.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LocalDeliveryConfig {
    /// Maximum candidates fetched in one local read.
    pub batch_size: u32,
    /// Maximum candidate bytes, checked before message blobs are loaded.
    pub max_bytes: u64,
    /// Maximum wait before checking cross-process writes and missed local wakes.
    pub poll_interval: Duration,
    /// Time before another process can take over an unrenewed default consumer.
    pub lease_duration: Duration,
    /// Renewal interval; must be shorter than the lease duration.
    pub renew_interval: Duration,
}

impl Default for LocalDeliveryConfig {
    fn default() -> Self {
        Self::from(&StreamPolicy::default())
    }
}

impl From<&StreamPolicy> for LocalDeliveryConfig {
    fn from(settings: &StreamPolicy) -> Self {
        Self {
            batch_size: settings.max_local_read_rows,
            max_bytes: settings.max_local_read_bytes,
            poll_interval: settings.active_database_poll_interval,
            lease_duration: settings.default_consumer_lease_duration,
            renew_interval: settings.default_consumer_lease_duration / LEASE_RENEWAL_DIVISOR,
        }
    }
}

impl LocalDeliveryConfig {
    pub(super) fn validate(self) -> Result<(), LocalDeliveryError> {
        if self.batch_size == 0
            || self.max_bytes == 0
            || self.poll_interval.is_zero()
            || self.renew_interval.is_zero()
            || self.renew_interval >= self.lease_duration
        {
            return Err(LocalDeliveryError::InvalidConfiguration);
        }
        self.lease_until(0)?;
        Ok(())
    }

    pub(super) fn lease_until(self, now: i64) -> Result<i64, LocalDeliveryError> {
        now.checked_add(self.lease_duration_ns()?)
            .ok_or(LocalDeliveryError::InvalidConfiguration)
    }

    pub(super) fn lease_duration_ns(self) -> Result<i64, LocalDeliveryError> {
        i64::try_from(self.lease_duration.as_nanos())
            .map_err(|_| LocalDeliveryError::InvalidConfiguration)
    }
}

#[derive(Debug, Error, ErrorCode)]
pub enum LocalDeliveryError {
    /// Preserve the first terminal background failure for every pending token.
    #[error(transparent)]
    #[error_code(inherit)]
    SessionFailure(std::sync::Arc<LocalDeliveryError>),
    /// Message content could not be decoded or enriched. Not retryable.
    #[error(transparent)]
    #[error_code(inherit)]
    Enrichment(crate::messages::enrichment::EnrichMessageError),
    /// The retained item has no decoded message. Not retryable.
    #[error("The delivery item has no enriched message")]
    EnrichedMessageUnavailable,
    /// This stream exhausted its network retry budget. Not retryable; a new stream may retry.
    #[error("The message reader exhausted its network recovery budget after {attempts} attempts: {}", source.as_ref().map(|error| error.to_string()).unwrap_or_else(|| "the recovery deadline elapsed without a transport error".into()))]
    NetworkRecoveryExhausted {
        attempts: u32,
        #[source]
        source: Option<std::sync::Arc<crate::subscriptions::incoming::IncomingError>>,
    },
    /// A terminal transport error stopped this stream. Not retryable.
    #[error("The message reader network failed: {0}")]
    NetworkFailure(#[source] std::sync::Arc<crate::subscriptions::incoming::IncomingError>),
    /// Database receipt, lease, cursor, or acknowledgement failure. May be retryable.
    #[error(transparent)]
    #[error_code(inherit)]
    Storage(#[from] StorageError),
    /// The callback failed or its token was dropped before acknowledgement. Not retryable.
    #[error("The delivered message was not acknowledged")]
    AcknowledgementRejected,
    /// A previous acknowledgement write failed. Reopen to retry delivery. Not retryable.
    #[error("Delivery stopped after acknowledgement persistence failed")]
    AcknowledgementFailed,
    /// Scope, filters, or retained content changed before dispatch. Reselect without acknowledgement.
    #[error("The queued message belongs to an old delivery selection")]
    SelectionChanged,
    /// The batch or timing settings cannot maintain a valid consumer lease. Not retryable.
    #[error("The local delivery configuration is invalid")]
    InvalidConfiguration,
    /// This reader has been closed. Not retryable.
    #[error("The local message reader is closed")]
    Closed,
}

impl RetryableError for LocalDeliveryError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::Storage(error) => error.is_retryable(),
            _ => false,
        }
    }
}
