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

/// Largest complete UpdateSubscriptions protobuf, excluding the gRPC header.
/// Two 32-byte credentials, full adds with 33-byte topics, maximum nonnegative
/// epoch bases, include_commits, and 42-byte keys; includes all field wrappers.
pub const NOTIFICATION_BATCH_MAX_BYTES: usize = 2 * (1 + 1 + 32)
    + NOTIFICATION_BATCH_TOPICS * (3 + 35 + 10 + 2)
    + NOTIFICATION_BATCH_HMAC_KEYS * (1 + 1 + 42);
