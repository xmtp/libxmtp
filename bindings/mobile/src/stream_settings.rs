use std::time::Duration;
use xmtp_mls::subscriptions::settings::{InvalidStreamSettings, StreamSettings};

/// Optional stream limits. Omitted fields use core defaults. Timer values are milliseconds.
#[derive(uniffi::Record, Debug, Clone, Default)]
pub struct FfiStreamSettings {
    /// Maximum rows in one durable admission batch.
    #[uniffi(default = None)]
    pub max_admission_rows: Option<u32>,
    /// Maximum encoded bytes in one durable admission batch.
    #[uniffi(default = None)]
    pub max_admission_bytes: Option<u64>,
    /// Maximum rows loaded in one incoming processing batch.
    #[uniffi(default = None)]
    pub max_fetched_rows: Option<u32>,
    /// Maximum encoded bytes loaded in one incoming processing batch.
    #[uniffi(default = None)]
    pub max_fetched_bytes: Option<u64>,
    /// Maximum unresolved group rows across topics.
    #[uniffi(default = None)]
    pub group_pending_rows: Option<u64>,
    /// Maximum encoded bytes in unresolved group rows.
    #[uniffi(default = None)]
    pub group_pending_bytes: Option<u64>,
    /// Maximum unresolved Welcome rows across topics.
    #[uniffi(default = None)]
    pub welcome_pending_rows: Option<u64>,
    /// Maximum encoded bytes in unresolved Welcome rows.
    #[uniffi(default = None)]
    pub welcome_pending_bytes: Option<u64>,
    /// Maximum unresolved identity rows across topics.
    #[uniffi(default = None)]
    pub identity_pending_rows: Option<u64>,
    /// Maximum encoded bytes in unresolved identity rows.
    #[uniffi(default = None)]
    pub identity_pending_bytes: Option<u64>,
    /// Maximum unresolved rows for one topic.
    #[uniffi(default = None)]
    pub max_pending_rows_per_topic: Option<u64>,
    /// Maximum encoded bytes in unresolved rows for one topic.
    #[uniffi(default = None)]
    pub max_pending_bytes_per_topic: Option<u64>,
    /// Maximum concurrent dependency requests.
    #[uniffi(default = None)]
    pub max_dependency_requests: Option<u32>,
    /// Maximum candidate rows in one local delivery read.
    #[uniffi(default = None)]
    pub max_local_read_rows: Option<u32>,
    /// Maximum message bytes in one local delivery read.
    #[uniffi(default = None)]
    pub max_local_read_bytes: Option<u64>,
    /// Time between receiver fallback checks, in milliseconds.
    #[uniffi(default = None)]
    pub receiver_fallback_interval_ms: Option<u64>,
    /// Time between active local database checks, in milliseconds.
    #[uniffi(default = None)]
    pub active_database_poll_interval_ms: Option<u64>,
    /// Default consumer ownership lease, in milliseconds. Must exceed the poll interval.
    #[uniffi(default = None)]
    pub default_consumer_lease_duration_ms: Option<u64>,
    /// Identity dependency deadline, in milliseconds. Must exceed the backend statement timeout.
    #[uniffi(default = None)]
    pub identity_reference_wait_ms: Option<u64>,
    /// Fixed processing-barrier deadline, in milliseconds.
    #[uniffi(default = None)]
    pub barrier_timeout_ms: Option<u64>,
}

impl TryFrom<FfiStreamSettings> for StreamSettings {
    type Error = InvalidStreamSettings;

    /// Fill omitted values, then apply the same range and relationship checks as core.
    fn try_from(value: FfiStreamSettings) -> Result<Self, Self::Error> {
        let mut settings = Self::default();
        if let Some(value) = value.max_admission_rows {
            settings.max_admission_rows = value;
        }
        if let Some(value) = value.max_admission_bytes {
            settings.max_admission_bytes = value;
        }
        if let Some(value) = value.max_fetched_rows {
            settings.max_fetched_rows = value;
        }
        if let Some(value) = value.max_fetched_bytes {
            settings.max_fetched_bytes = value;
        }
        if let Some(value) = value.group_pending_rows {
            settings.group_pending.rows = value;
        }
        if let Some(value) = value.group_pending_bytes {
            settings.group_pending.bytes = value;
        }
        if let Some(value) = value.welcome_pending_rows {
            settings.welcome_pending.rows = value;
        }
        if let Some(value) = value.welcome_pending_bytes {
            settings.welcome_pending.bytes = value;
        }
        if let Some(value) = value.identity_pending_rows {
            settings.identity_pending.rows = value;
        }
        if let Some(value) = value.identity_pending_bytes {
            settings.identity_pending.bytes = value;
        }
        if let Some(value) = value.max_pending_rows_per_topic {
            settings.max_pending_rows_per_topic = value;
        }
        if let Some(value) = value.max_pending_bytes_per_topic {
            settings.max_pending_bytes_per_topic = value;
        }
        if let Some(value) = value.max_dependency_requests {
            settings.max_dependency_requests = value
                .try_into()
                .map_err(|_| InvalidStreamSettings("max_dependency_requests"))?;
        }
        if let Some(value) = value.max_local_read_rows {
            settings.max_local_read_rows = value;
        }
        if let Some(value) = value.max_local_read_bytes {
            settings.max_local_read_bytes = value;
        }
        if let Some(value) = value.receiver_fallback_interval_ms {
            settings.receiver_fallback_interval = Duration::from_millis(value);
        }
        if let Some(value) = value.active_database_poll_interval_ms {
            settings.active_database_poll_interval = Duration::from_millis(value);
        }
        if let Some(value) = value.default_consumer_lease_duration_ms {
            settings.default_consumer_lease_duration = Duration::from_millis(value);
        }
        if let Some(value) = value.identity_reference_wait_ms {
            settings.identity_reference_wait = Duration::from_millis(value);
        }
        if let Some(value) = value.barrier_timeout_ms {
            settings.barrier_timeout = Duration::from_millis(value);
        }
        settings.validate()?;
        Ok(settings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[xmtp_common::test(unwrap_try = true)]
    fn partial_stream_settings_keep_core_defaults() {
        let defaults = StreamSettings::default();
        let settings = StreamSettings::try_from(FfiStreamSettings {
            max_admission_rows: Some(5),
            group_pending_bytes: Some(1234),
            receiver_fallback_interval_ms: Some(321),
            ..Default::default()
        })?;
        assert_eq!(settings.max_admission_rows, 5);
        assert_eq!(settings.group_pending.bytes, 1234);
        assert_eq!(
            settings.receiver_fallback_interval,
            Duration::from_millis(321)
        );
        assert_eq!(settings.max_admission_bytes, defaults.max_admission_bytes);
        assert_eq!(settings.group_pending.rows, defaults.group_pending.rows);
        assert_eq!(settings.max_fetched_rows, defaults.max_fetched_rows);
        assert_eq!(settings.barrier_timeout, defaults.barrier_timeout);
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn stream_settings_use_core_range_and_relationship_validation() {
        for (settings, name) in [
            (
                FfiStreamSettings {
                    max_admission_rows: Some(0),
                    ..Default::default()
                },
                "max_admission_rows",
            ),
            (
                FfiStreamSettings {
                    max_local_read_bytes: Some(u64::MAX),
                    ..Default::default()
                },
                "max_local_read_bytes",
            ),
            (
                FfiStreamSettings {
                    receiver_fallback_interval_ms: Some(i32::MAX as u64 + 1),
                    ..Default::default()
                },
                "receiver_fallback_interval",
            ),
            (
                FfiStreamSettings {
                    active_database_poll_interval_ms: Some(2),
                    default_consumer_lease_duration_ms: Some(1),
                    ..Default::default()
                },
                "default_consumer_lease_duration",
            ),
            (
                FfiStreamSettings {
                    identity_reference_wait_ms: Some(1),
                    ..Default::default()
                },
                "identity_reference_wait",
            ),
        ] {
            assert!(matches!(
                StreamSettings::try_from(settings),
                Err(InvalidStreamSettings(actual)) if actual == name
            ));
        }
    }
}
