/// Group and commit-log topic identifier length, in bytes.
pub const BACKEND_GROUP_ID_BYTES: usize = 16;
/// Welcome, key-package, and identity topic identifier length, in bytes.
pub const BACKEND_INSTALLATION_ID_BYTES: usize = 32;

/// Default age for ordinary group messages, in seconds.
pub const BACKEND_DEFAULT_GROUP_MESSAGE_SECONDS: u64 = 7_776_000;
/// Default age for welcome messages, in seconds.
pub const BACKEND_DEFAULT_WELCOME_SECONDS: u64 = 7_776_000;
/// Default age for key packages, in seconds.
pub const BACKEND_DEFAULT_KEY_PACKAGE_SECONDS: u64 = 7_776_000;

/// Default per-topic query limit.
pub const BACKEND_DEFAULT_QUERY_LIMIT: usize = 100;
/// Default maximum query limit.
pub const BACKEND_DEFAULT_MAX_QUERY_LIMIT: usize = 1_000;
/// Default maximum number of query topics.
pub const BACKEND_DEFAULT_MAX_QUERY_TOPICS: usize = 1_000;
/// Default maximum number of metadata-only newest topics.
pub const BACKEND_DEFAULT_MAX_NEWEST_METADATA_TOPICS: usize = 1_000;
/// Default maximum number of full newest topics.
pub const BACKEND_DEFAULT_MAX_NEWEST_FULL_TOPICS: usize = 100;
/// Default maximum number of topics in one publish.
pub const BACKEND_DEFAULT_MAX_PUBLISH_TOPICS: usize = 1_000;
/// Default maximum encoded envelope size, in bytes.
pub const BACKEND_DEFAULT_MAX_ENVELOPE_BYTES: usize = 1_048_576;
/// Default maximum encoded request size, in bytes.
pub const BACKEND_DEFAULT_MAX_REQUEST_BYTES: usize = 26_214_400;
/// Default maximum encoded response size, in bytes.
pub const BACKEND_DEFAULT_MAX_RESPONSE_BYTES: usize = 26_214_400;
/// Default maximum topics added by one stream update.
pub const BACKEND_DEFAULT_MAX_UPDATE_ADDS: usize = 100_000;
/// Default maximum topics removed by one stream update.
pub const BACKEND_DEFAULT_MAX_UPDATE_REMOVES: usize = 100_000;
/// Default maximum registered topics per stream.
pub const BACKEND_DEFAULT_MAX_STREAM_TOPICS: usize = 100_000;
/// Default maximum topics in a static subscription.
pub const BACKEND_DEFAULT_MAX_STATIC_TOPICS: usize = 10_000;
/// Default maximum identifiers in one inbox lookup.
pub const BACKEND_DEFAULT_MAX_LOOKUP_IDENTIFIERS: usize = 250;
/// Default maximum smart-contract-wallet signatures per request.
pub const BACKEND_DEFAULT_MAX_SCW_SIGNATURES: usize = 100;
/// Default maximum identity entries per inbox.
pub const BACKEND_DEFAULT_MAX_IDENTITY_ENTRIES: usize = 256;
/// Default maximum HTTP/2 streams per connection.
pub const BACKEND_DEFAULT_MAX_HTTP2_STREAMS: usize = 100;
/// Default stream update rate, in frames per second.
pub const BACKEND_DEFAULT_MAX_UPDATE_FRAMES_PER_SECOND: u32 = 10;
/// Default stream update token-bucket burst.
pub const BACKEND_DEFAULT_MAX_UPDATE_BURST: u32 = 100;
/// Default client ping rate, in frames per second.
pub const BACKEND_DEFAULT_MAX_PING_FRAMES_PER_SECOND: u32 = 10;
/// Default client ping token-bucket burst.
pub const BACKEND_DEFAULT_MAX_PING_BURST: u32 = 100;

/// Default interval between stream keepalive frames, in milliseconds.
pub const BACKEND_DEFAULT_KEEPALIVE_INTERVAL_MS: u64 = 30_000;
