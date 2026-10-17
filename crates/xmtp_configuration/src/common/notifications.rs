use std::time::Duration;

/// Bound the complete notification operation, including auth and transport retries.
pub const NOTIFICATION_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Maximum total adds and removes in one notification request.
pub const NOTIFICATION_BATCH_TOPICS: usize = 1000;
