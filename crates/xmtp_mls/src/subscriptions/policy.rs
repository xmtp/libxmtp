//! Client limits for incoming work and local message readers.

use std::time::Duration;
use xmtp_configuration::*;
use xmtp_db::incoming_envelope::{IncomingLimits, NetworkEntityKind, PendingBudget};

/// Bounds apply to encoded items and row metadata, not to the whole database.
#[derive(Debug, Clone)]
pub(crate) struct StreamPolicy {
    /// Maximum rows committed in one durable admission transaction.
    pub(crate) max_admission_rows: u32,
    /// Maximum encoded bytes committed in one admission transaction.
    pub(crate) max_admission_bytes: u64,
    /// Maximum rows accepted from one network fetch, before admission chunks.
    pub(crate) max_fetched_rows: u32,
    /// Maximum bytes accepted from one network fetch; independent of per-kind budgets.
    pub(crate) max_fetched_bytes: u64,
    /// Total retained group-envelope budget across all group topics.
    pub(crate) group_pending: PendingBudget,
    /// Total retained Welcome budget, separate from group and identity budgets.
    pub(crate) welcome_pending: PendingBudget,
    /// Total retained identity-envelope budget across identity topics.
    pub(crate) identity_pending: PendingBudget,
    /// Per-topic row limit, applied alongside the total kind budget.
    pub(crate) max_pending_rows_per_topic: u64,
    /// Per-topic byte limit; rejected chunks do not advance F.
    pub(crate) max_pending_bytes_per_topic: u64,
    /// Maximum concurrent dependency and fixed-target requests.
    pub(crate) max_dependency_requests: usize,
    /// Maximum stored candidates loaded by one local-delivery read.
    pub(crate) max_local_read_rows: u32,
    /// Maximum candidate bytes checked before loading message blobs.
    pub(crate) max_local_read_bytes: u64,
    /// Bound on stream-first receipt waits. Explicit sync queries without this wait.
    pub(crate) receiver_fallback_interval: Duration,
    /// Poll interval for cross-process progress and missed local wake events.
    pub(crate) active_database_poll_interval: Duration,
    /// Default-reader ownership expires after this interval without renewal.
    pub(crate) default_consumer_lease_duration: Duration,
    /// Healthy-primary wait before an absent exact identity reference is rejected.
    pub(crate) identity_reference_wait: Duration,
    /// One budget for target capture, receipt, processing, and required send follow-up work.
    pub(crate) barrier_timeout: Duration,
    /// Interval between full rescans of blocked Welcome rows.
    pub(crate) blocked_welcome_rescan_interval: Duration,
    /// First delay before retrying a source that failed with a permanent error.
    pub(crate) permanent_retry_initial: Duration,
    /// Longest delay between retries of a repeatedly failing source.
    pub(crate) permanent_retry_max: Duration,
}

impl Default for StreamPolicy {
    fn default() -> Self {
        Self {
            max_admission_rows: STREAM_BATCH_ROWS,
            max_admission_bytes: STREAM_BATCH_BYTES,
            max_fetched_rows: STREAM_BATCH_ROWS,
            max_fetched_bytes: STREAM_BATCH_BYTES,
            group_pending: PendingBudget {
                rows: STREAM_GROUP_ROWS,
                bytes: STREAM_GROUP_BYTES,
            },
            welcome_pending: PendingBudget {
                rows: STREAM_WELCOME_ROWS,
                bytes: STREAM_WELCOME_BYTES,
            },
            identity_pending: PendingBudget {
                rows: STREAM_IDENTITY_ROWS,
                bytes: STREAM_IDENTITY_BYTES,
            },
            max_pending_rows_per_topic: STREAM_TOPIC_ROWS,
            max_pending_bytes_per_topic: STREAM_TOPIC_BYTES,
            max_dependency_requests: IDENTITY_DEPENDENCY_CONCURRENCY,
            max_local_read_rows: STREAM_LOCAL_READ_ROWS,
            max_local_read_bytes: STREAM_LOCAL_READ_BYTES,
            receiver_fallback_interval: RECEIVER_FALLBACK_INTERVAL,
            active_database_poll_interval: ACTIVE_DATABASE_POLL_INTERVAL,
            default_consumer_lease_duration: DEFAULT_CONSUMER_LEASE_DURATION,
            identity_reference_wait: IDENTITY_REFERENCE_WAIT,
            barrier_timeout: STREAM_BARRIER_TIMEOUT,
            blocked_welcome_rescan_interval: STREAM_BLOCKED_WELCOME_RESCAN_INTERVAL,
            permanent_retry_initial: STREAM_PERMANENT_RETRY_INITIAL,
            permanent_retry_max: STREAM_PERMANENT_RETRY_MAX,
        }
    }
}

impl StreamPolicy {
    pub(crate) fn incoming_limits(&self, kind: NetworkEntityKind) -> IncomingLimits {
        IncomingLimits {
            batch: PendingBudget {
                rows: u64::from(self.max_admission_rows),
                bytes: self.max_admission_bytes,
            },
            topic: PendingBudget {
                rows: self.max_pending_rows_per_topic,
                bytes: self.max_pending_bytes_per_topic,
            },
            kind: match kind {
                NetworkEntityKind::Group => self.group_pending,
                NetworkEntityKind::Welcome => self.welcome_pending,
                NetworkEntityKind::Identity => self.identity_pending,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl StreamPolicy {
        fn validate(&self) -> Result<(), &'static str> {
            let positive = [
                ("max_admission_rows", u64::from(self.max_admission_rows)),
                ("max_admission_bytes", self.max_admission_bytes),
                ("max_fetched_rows", u64::from(self.max_fetched_rows)),
                ("max_fetched_bytes", self.max_fetched_bytes),
                ("group_pending.rows", self.group_pending.rows),
                ("group_pending.bytes", self.group_pending.bytes),
                ("welcome_pending.rows", self.welcome_pending.rows),
                ("welcome_pending.bytes", self.welcome_pending.bytes),
                ("identity_pending.rows", self.identity_pending.rows),
                ("identity_pending.bytes", self.identity_pending.bytes),
                (
                    "max_pending_rows_per_topic",
                    self.max_pending_rows_per_topic,
                ),
                (
                    "max_pending_bytes_per_topic",
                    self.max_pending_bytes_per_topic,
                ),
                (
                    "max_dependency_requests",
                    self.max_dependency_requests as u64,
                ),
                ("max_local_read_rows", u64::from(self.max_local_read_rows)),
                ("max_local_read_bytes", self.max_local_read_bytes),
            ];
            for (name, value) in positive {
                if value == 0 || value > i64::MAX as u64 {
                    return Err(name);
                }
            }
            for (name, value) in [
                (
                    "receiver_fallback_interval",
                    self.receiver_fallback_interval,
                ),
                (
                    "active_database_poll_interval",
                    self.active_database_poll_interval,
                ),
                (
                    "default_consumer_lease_duration",
                    self.default_consumer_lease_duration,
                ),
                ("identity_reference_wait", self.identity_reference_wait),
                ("barrier_timeout", self.barrier_timeout),
                (
                    "blocked_welcome_rescan_interval",
                    self.blocked_welcome_rescan_interval,
                ),
                ("permanent_retry_initial", self.permanent_retry_initial),
                ("permanent_retry_max", self.permanent_retry_max),
            ] {
                // The same timer values must work on native and browser clients.
                if value.is_zero() || value.as_millis() > i32::MAX as u128 {
                    return Err(name);
                }
            }
            if self.active_database_poll_interval >= self.default_consumer_lease_duration {
                return Err("default_consumer_lease_duration");
            }
            if self.permanent_retry_initial > self.permanent_retry_max {
                return Err("permanent_retry_max");
            }
            if self.identity_reference_wait
                <= Duration::from_millis(BACKEND_DEFAULT_STATEMENT_TIMEOUT_MS)
            {
                return Err("identity_reference_wait");
            }
            Ok(())
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn internal_limits_fit_storage_and_timer_ranges() {
        StreamPolicy::default().validate()?;
    }
}
