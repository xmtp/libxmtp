use std::time::Duration;

/// Bound the complete notification operation, including auth and transport retries.
pub const NOTIFICATION_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Maximum total adds and removes in one notification request.
pub const NOTIFICATION_BATCH_TOPICS: usize = 1000;
/// Most HMAC keys one subscription may carry: the previous, current, and next epoch.
pub const MAX_SUBSCRIPTION_HMAC_KEYS: usize = 3;
/// Most HMAC keys one notification request may carry across its adds.
pub const NOTIFICATION_BATCH_HMAC_KEYS: usize =
    NOTIFICATION_BATCH_TOPICS * MAX_SUBSCRIPTION_HMAC_KEYS;
