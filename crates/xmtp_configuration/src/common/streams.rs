//! Default client bounds for durable receipt and local delivery.

use std::time::Duration;

/// Maximum rows in one admission transaction or fetched page.
pub const STREAM_BATCH_ROWS: u32 = 128;
/// Maximum encoded bytes in one admission transaction or fetched page.
pub const STREAM_BATCH_BYTES: u64 = 32 * 1024 * 1024;
/// Maximum pending rows for one topic.
pub const STREAM_TOPIC_ROWS: u64 = 1024;
/// Maximum pending encoded bytes for one topic.
pub const STREAM_TOPIC_BYTES: u64 = 64 * 1024 * 1024;
/// Maximum pending group rows across all group topics.
pub const STREAM_GROUP_ROWS: u64 = 8192;
/// Maximum pending group bytes across all group topics.
pub const STREAM_GROUP_BYTES: u64 = 128 * 1024 * 1024;
/// Reserved pending Welcome rows, separate from group and identity capacity.
pub const STREAM_WELCOME_ROWS: u64 = 1024;
/// Reserved pending Welcome bytes.
pub const STREAM_WELCOME_BYTES: u64 = 64 * 1024 * 1024;
/// Reserved pending identity rows, separate from group and Welcome capacity.
pub const STREAM_IDENTITY_ROWS: u64 = 4096;
/// Reserved pending identity bytes.
pub const STREAM_IDENTITY_BYTES: u64 = 32 * 1024 * 1024;
/// Maximum rows returned by one local message read.
pub const STREAM_LOCAL_READ_ROWS: u32 = 128;
/// Maximum decoded bytes returned by one local message read.
pub const STREAM_LOCAL_READ_BYTES: u64 = 16 * 1024 * 1024;
/// Maximum delay before a receiver with no progress falls back to Query.
pub const RECEIVER_FALLBACK_INTERVAL: Duration = Duration::from_secs(1);
/// Maximum delay between fresh database checks while work is active.
pub const ACTIVE_DATABASE_POLL_INTERVAL: Duration = Duration::from_millis(250);
/// Lease duration for the default app message consumer.
pub const DEFAULT_CONSUMER_LEASE_DURATION: Duration = Duration::from_secs(30);
/// Maximum duration of one bounded catch-up operation.
pub const STREAM_BARRIER_TIMEOUT: Duration = Duration::from_secs(60);
