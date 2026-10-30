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
/// How often a long-lived client rechecks Welcomes it could not process, so
/// their retention deadline is reached without restarting the client.
pub const STREAM_BLOCKED_WELCOME_RESCAN_INTERVAL: Duration = Duration::from_secs(3600);
/// First delay before retrying a receiver that failed with a permanent error.
/// A permanent classification describes this response, not every later one, so
/// the retry never stops. It only slows down.
pub const STREAM_PERMANENT_RETRY_INITIAL: Duration = Duration::from_secs(5);
/// Longest delay between retries of a receiver that keeps failing permanently.
pub const STREAM_PERMANENT_RETRY_MAX: Duration = Duration::from_secs(300);
/// Conversations returned by one app conversation listing that names no limit.
pub const CONVERSATION_LIST_DEFAULT_LIMIT: u32 = 100;
/// Messages returned by one `messages()` call that names no limit.
pub const MESSAGE_LIST_DEFAULT_LIMIT: u32 = 100;
/// Largest limit an app may name for one conversation list, message list or reaction page.
pub const LOCAL_PAGE_MAX_LIMIT: u32 = 1000;
/// Reaction messages in a message's reaction summary, and in one reaction page
/// that names no limit.
pub const MESSAGE_REACTION_LIMIT: u32 = 50;
/// Extra refill delay for client Update pacing to absorb variation in arrival time.
pub const STREAM_UPDATE_PACING_MARGIN: Duration = Duration::from_millis(500);
