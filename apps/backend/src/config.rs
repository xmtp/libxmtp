//! Backend configuration loaded from a TOML file.

use std::{collections::BTreeMap, fs, net::SocketAddr, path::Path};

use schemars::{JsonSchema, Schema};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use xmtp_common::NS_IN_SEC;
use xmtp_configuration::{
    BACKEND_DEFAULT_GROUP_MESSAGE_SECONDS, BACKEND_DEFAULT_KEY_PACKAGE_SECONDS,
    BACKEND_DEFAULT_MAX_ENVELOPE_BYTES, BACKEND_DEFAULT_MAX_HTTP2_STREAMS,
    BACKEND_DEFAULT_MAX_IDENTITY_ENTRIES, BACKEND_DEFAULT_MAX_LOOKUP_IDENTIFIERS,
    BACKEND_DEFAULT_MAX_NEWEST_FULL_TOPICS, BACKEND_DEFAULT_MAX_NEWEST_METADATA_TOPICS,
    BACKEND_DEFAULT_MAX_PING_BURST, BACKEND_DEFAULT_MAX_PING_FRAMES_PER_SECOND,
    BACKEND_DEFAULT_MAX_PUBLISH_TOPICS, BACKEND_DEFAULT_MAX_QUERY_LIMIT,
    BACKEND_DEFAULT_MAX_QUERY_TOPICS, BACKEND_DEFAULT_MAX_REQUEST_BYTES,
    BACKEND_DEFAULT_MAX_RESPONSE_BYTES, BACKEND_DEFAULT_MAX_SCW_SIGNATURES,
    BACKEND_DEFAULT_MAX_STATIC_TOPICS, BACKEND_DEFAULT_MAX_STREAM_TOPICS,
    BACKEND_DEFAULT_MAX_UPDATE_ADDS, BACKEND_DEFAULT_MAX_UPDATE_BURST,
    BACKEND_DEFAULT_MAX_UPDATE_FRAMES_PER_SECOND, BACKEND_DEFAULT_MAX_UPDATE_REMOVES,
    BACKEND_DEFAULT_QUERY_LIMIT, BACKEND_DEFAULT_WELCOME_SECONDS,
};

mod schema;

const SCHEMA_ID: &str =
    "https://raw.githubusercontent.com/xmtp/libxmtp/self-hosted/docs/schemas/backend-v1.json";

// These values are server implementation details. They are not configuration keys.
pub(crate) const DELIVERY_FRAME_BYTES: usize = 2 * 1024 * 1024;
pub(crate) const FETCH_BUFFER_BYTES: usize = 64 * 1024 * 1024;
pub(crate) const OUTBOUND_QUEUE_BYTES: usize = 16 * 1024 * 1024;
// Maximum metadata with a 128-byte stored topic and ten-byte scalar varints.
const MAX_METADATA_BYTES: usize = 207;
const MAX_NESTED_FIELD_OVERHEAD: usize = 11;
const GRPC_HEADER_BYTES: usize = 5;
pub(crate) const ENVELOPE_METADATA_AND_FRAMING_BYTES: usize =
    MAX_METADATA_BYTES + 4 * MAX_NESTED_FIELD_OVERHEAD + GRPC_HEADER_BYTES;
const MAX_RETENTION_SECONDS: u64 = i64::MAX as u64 / NS_IN_SEC as u64;
const MAX_DATABASE_TIMEOUT_MS: u64 = i32::MAX as u64;

const DEFAULT_LISTEN: &str = "0.0.0.0:5050";
const DEFAULT_DRAIN_DURATION_MS: u64 = 10_000;
const DEFAULT_MAX_CONNECTIONS: u32 = 20;
const DEFAULT_STATEMENT_TIMEOUT_MS: u64 = 5_000;
const DEFAULT_PUBLISH_DURATION_MS: u64 = 10_000;
const DEFAULT_BARRIER_WAIT_MS: u64 = 1_000;
const DEFAULT_POLL_INTERVAL_MS: u64 = 100;
const DEFAULT_MAX_GAP_RANGES: usize = 10_000;
const DEFAULT_KEEPALIVE_INTERVAL_MS: u64 = 30_000;
const DEFAULT_MAX_PONG_WAIT_MS: u64 = 90_000;
const DEFAULT_MAX_SCW_CACHE_ENTRIES: usize = 10_000;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("could not read configuration file")]
    Read(#[source] std::io::Error),
    #[error("configuration is not valid TOML")]
    Parse,
    #[error("configuration is invalid: {field} ({reason})")]
    Invalid {
        field: &'static str,
        reason: &'static str,
    },
    #[error("environment variable {name} is not available")]
    Environment { name: String },
}

#[derive(Debug, Error)]
enum EnvironmentError {
    #[error("environment variable {name} is not available")]
    Missing { name: String },
    #[error("environment variable name is empty")]
    EmptyName,
}

/// Resolve one environment reference. Do not expand references in the resolved value.
fn resolve_env(value: &str) -> Result<String, EnvironmentError> {
    let Some(name) = value.strip_prefix("env:") else {
        return Ok(value.to_owned());
    };
    if name.is_empty() {
        return Err(EnvironmentError::EmptyName);
    }
    std::env::var(name).map_err(|_| EnvironmentError::Missing {
        name: name.to_owned(),
    })
}

/// Complete backend configuration.
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    #[serde(default)]
    pub publishing: PublishingConfig,
    #[serde(default)]
    pub streams: StreamsConfig,
    #[serde(default)]
    pub retention: RetentionConfig,
    #[serde(default)]
    #[schemars(schema_with = "schema::chains")]
    pub chains: BTreeMap<String, String>,
    #[serde(default)]
    pub validation: ValidationConfig,
    #[serde(default)]
    pub limits: LimitsConfig,
}

impl Config {
    /// Load, resolve environment references, and validate one TOML file.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let contents = fs::read_to_string(path).map_err(ConfigError::Read)?;
        let mut config: Self = toml::from_str(&contents).map_err(|_| ConfigError::Parse)?;
        config.resolve_environment()?;
        config.validate()?;
        Ok(config)
    }

    /// Resolve each supported string before validation without exposing secret values.
    fn resolve_environment(&mut self) -> Result<(), ConfigError> {
        self.server.listen = resolve_url(&self.server.listen, "server.listen")?;
        self.database.url = resolve_url(&self.database.url, "database.url")?;
        self.database.replica_url = self
            .database
            .replica_url
            .as_deref()
            .map(|url| resolve_url(url, "database.replica_url"))
            .transpose()?;
        for url in self.chains.values_mut() {
            *url = resolve_url(url, "chains")?;
        }
        Ok(())
    }

    /// Validate scalar values and relationships between values.
    pub fn validate(&self) -> Result<(), ConfigError> {
        for (field, milliseconds) in [
            (
                "server.max_drain_duration_ms",
                self.server.max_drain_duration_ms,
            ),
            (
                "database.max_statement_timeout_ms",
                self.database.max_statement_timeout_ms,
            ),
            (
                "publishing.max_publish_duration_ms",
                self.publishing.max_publish_duration_ms,
            ),
            (
                "publishing.max_barrier_wait_ms",
                self.publishing.max_barrier_wait_ms,
            ),
            ("streams.poll_interval_ms", self.streams.poll_interval_ms),
            (
                "streams.keepalive_interval_ms",
                self.streams.keepalive_interval_ms,
            ),
            ("streams.max_pong_wait_ms", self.streams.max_pong_wait_ms),
        ] {
            validate_deadline(milliseconds, field)?;
        }
        self.server.validate()?;
        self.database.validate()?;
        self.publishing
            .validate(self.database.max_statement_timeout_ms)?;
        self.streams.validate()?;
        self.retention.validate()?;
        self.validation.validate()?;
        self.validate_chains()?;
        self.limits.validate()?;
        Ok(())
    }

    /// Require chain keys that the SCW verifier can route and HTTP(S) RPC URLs.
    fn validate_chains(&self) -> Result<(), ConfigError> {
        for (chain, url) in &self.chains {
            if xmtp_id::associations::AccountId::new(chain.clone(), "0x0".to_owned())
                .get_chain_id_u64()
                .is_err()
            {
                return Err(invalid("chains", "keys must be EIP-155 CAIP-2 chain ids"));
            }
            non_empty_url(url, "chains", UrlKind::Http)?;
        }
        Ok(())
    }

    /// Return the JSON schema published for Taplo configuration files.
    pub fn schema() -> Schema {
        let mut schema = schemars::schema_for!(Self);
        schema.ensure_object().insert(
            "$id".to_owned(),
            serde_json::Value::String(SCHEMA_ID.to_owned()),
        );
        schema
    }
}

impl std::fmt::Debug for Config {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Config")
            .field("server", &self.server)
            .field("database", &self.database)
            .field("publishing", &self.publishing)
            .field("streams", &self.streams)
            .field("retention", &self.retention)
            .field(
                "chains",
                &format_args!("{} configured chain(s)", self.chains.len()),
            )
            .field("validation", &self.validation)
            .field("limits", &self.limits)
            .finish()
    }
}

/// Attach the configuration field to an environment error without including its value.
fn resolve_url(value: &str, field: &'static str) -> Result<String, ConfigError> {
    resolve_env(value).map_err(|error| match error {
        EnvironmentError::Missing { name } => ConfigError::Environment { name },
        EnvironmentError::EmptyName => invalid(field, "environment variable name is empty"),
    })
}

/// Timer ranges depend on the host's monotonic clock, not an arbitrary duration cap.
fn validate_deadline(milliseconds: u64, field: &'static str) -> Result<(), ConfigError> {
    if xmtp_common::time::Instant::now()
        .checked_add(std::time::Duration::from_millis(milliseconds))
        .is_none()
    {
        return Err(invalid(
            field,
            "deadline cannot be represented on this host",
        ));
    }
    Ok(())
}

fn invalid(field: &'static str, reason: &'static str) -> ConfigError {
    ConfigError::Invalid { field, reason }
}

fn positive<T>(value: T, field: &'static str) -> Result<(), ConfigError>
where
    T: PartialEq + PartialOrd + Default,
{
    if value <= T::default() {
        Err(invalid(field, "must be greater than zero"))
    } else {
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum UrlKind {
    Postgres,
    Http,
}

/// Check a resolved URL and its allowed scheme without including the URL in errors.
fn non_empty_url(value: &str, field: &'static str, kind: UrlKind) -> Result<(), ConfigError> {
    let valid_scheme = match kind {
        UrlKind::Postgres => value.starts_with("postgres://") || value.starts_with("postgresql://"),
        UrlKind::Http => value.starts_with("http://") || value.starts_with("https://"),
    };
    if value.trim().is_empty() || !valid_scheme || url::Url::parse(value).is_err() {
        Err(invalid(field, "must be a non-empty URL"))
    } else {
        Ok(())
    }
}

impl ServerConfig {
    /// Require a numeric bind address and a positive shutdown budget.
    fn validate(&self) -> Result<(), ConfigError> {
        if self.listen.parse::<SocketAddr>().is_err() {
            return Err(invalid("server.listen", "must be a socket address"));
        }
        positive(self.max_drain_duration_ms, "server.max_drain_duration_ms")
    }
}

impl DatabaseConfig {
    /// Check both pool URLs and the range accepted by Postgres statement timeouts.
    fn validate(&self) -> Result<(), ConfigError> {
        non_empty_url(&self.url, "database.url", UrlKind::Postgres)?;
        if self.max_statement_timeout_ms > MAX_DATABASE_TIMEOUT_MS {
            return Err(invalid(
                "database.max_statement_timeout_ms",
                "exceeds the Postgres timeout range",
            ));
        }
        if let Some(url) = &self.replica_url {
            non_empty_url(url, "database.replica_url", UrlKind::Postgres)?;
        }
        positive(self.max_connections, "database.max_connections")?;
        positive(
            self.max_statement_timeout_ms,
            "database.max_statement_timeout_ms",
        )
    }
}

impl PublishingConfig {
    /// Keep transaction and barrier timeouts in the Postgres range.
    /// The transaction budget must exceed a single statement budget.
    fn validate(&self, statement_timeout_ms: u64) -> Result<(), ConfigError> {
        if self.max_publish_duration_ms > MAX_DATABASE_TIMEOUT_MS
            || self.max_barrier_wait_ms > MAX_DATABASE_TIMEOUT_MS
        {
            return Err(invalid("publishing", "exceeds the Postgres timeout range"));
        }
        if self.max_publish_duration_ms <= statement_timeout_ms {
            return Err(invalid(
                "publishing.max_publish_duration_ms",
                "must be greater than database.max_statement_timeout_ms",
            ));
        }
        positive(
            self.max_publish_duration_ms,
            "publishing.max_publish_duration_ms",
        )?;
        positive(self.max_barrier_wait_ms, "publishing.max_barrier_wait_ms")
    }
}

impl StreamsConfig {
    /// Check stream capacities and timing, including the advertised wire interval.
    fn validate(&self) -> Result<(), ConfigError> {
        positive(self.poll_interval_ms, "streams.poll_interval_ms")?;
        positive(self.max_gap_ranges, "streams.max_gap_ranges")?;
        positive(self.keepalive_interval_ms, "streams.keepalive_interval_ms")?;
        if self.keepalive_interval_ms > u32::MAX as u64 {
            return Err(invalid(
                "streams.keepalive_interval_ms",
                "must fit the Started keepalive interval type",
            ));
        }
        if self.max_pong_wait_ms <= self.keepalive_interval_ms {
            return Err(invalid(
                "streams.max_pong_wait_ms",
                "must be greater than streams.keepalive_interval_ms",
            ));
        }
        positive(self.max_pong_wait_ms, "streams.max_pong_wait_ms")
    }
}

impl RetentionConfig {
    /// Check finite expiry against the primary database's startup clock. The
    /// insert still checks arithmetic after later clock changes or time advances.
    pub(crate) fn validate_at(&self, database_ns: i64) -> Result<(), ConfigError> {
        for (field, seconds) in [
            (
                "retention.group_message_seconds",
                self.group_message_seconds,
            ),
            ("retention.welcome_seconds", self.welcome_seconds),
            ("retention.key_package_seconds", self.key_package_seconds),
        ] {
            let expiry = i64::try_from(seconds)
                .ok()
                .and_then(|seconds| seconds.checked_mul(NS_IN_SEC))
                .and_then(|duration| database_ns.checked_add(duration));
            if database_ns < 0 || expiry.is_none() {
                return Err(invalid(
                    field,
                    "expiry cannot be represented at the database clock",
                ));
            }
        }
        Ok(())
    }

    /// Keep positive retention periods representable in nanosecond expiry arithmetic.
    fn validate(&self) -> Result<(), ConfigError> {
        positive(
            self.group_message_seconds,
            "retention.group_message_seconds",
        )?;
        positive(self.welcome_seconds, "retention.welcome_seconds")?;
        positive(self.key_package_seconds, "retention.key_package_seconds")?;
        if self.group_message_seconds > MAX_RETENTION_SECONDS
            || self.welcome_seconds > MAX_RETENTION_SECONDS
            || self.key_package_seconds > MAX_RETENTION_SECONDS
        {
            return Err(invalid(
                "retention",
                "duration is too large for nanosecond expiry arithmetic",
            ));
        }
        Ok(())
    }
}

impl ValidationConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        positive(
            self.max_scw_cache_entries,
            "validation.max_scw_cache_entries",
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct ServerConfig {
    #[schemars(schema_with = "schema::socket_address")]
    pub listen: String,
    #[schemars(schema_with = "schema::positive_integer::<{ u64::MAX }>")]
    pub max_drain_duration_ms: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen: DEFAULT_LISTEN.to_owned(),
            max_drain_duration_ms: DEFAULT_DRAIN_DURATION_MS,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DatabaseConfig {
    #[schemars(schema_with = "schema::postgres_url")]
    pub url: String,
    #[serde(default)]
    #[schemars(schema_with = "schema::optional_postgres_url")]
    pub replica_url: Option<String>,
    #[serde(default = "default_max_connections")]
    #[schemars(schema_with = "schema::positive_integer::<{ u32::MAX as u64 }>")]
    pub max_connections: u32,
    #[serde(default = "default_statement_timeout_ms")]
    #[schemars(range(min = 1, max = MAX_DATABASE_TIMEOUT_MS))]
    pub max_statement_timeout_ms: u64,
}

fn default_max_connections() -> u32 {
    DEFAULT_MAX_CONNECTIONS
}
fn default_statement_timeout_ms() -> u64 {
    DEFAULT_STATEMENT_TIMEOUT_MS
}

impl std::fmt::Debug for DatabaseConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DatabaseConfig")
            .field("url", &"<redacted>")
            .field(
                "replica_url",
                &self.replica_url.as_ref().map(|_| "<redacted>"),
            )
            .field("max_connections", &self.max_connections)
            .field("max_statement_timeout_ms", &self.max_statement_timeout_ms)
            .finish()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct PublishingConfig {
    #[schemars(range(min = 1, max = MAX_DATABASE_TIMEOUT_MS))]
    pub max_publish_duration_ms: u64,
    #[schemars(range(min = 1, max = MAX_DATABASE_TIMEOUT_MS))]
    pub max_barrier_wait_ms: u64,
}

impl Default for PublishingConfig {
    fn default() -> Self {
        Self {
            max_publish_duration_ms: DEFAULT_PUBLISH_DURATION_MS,
            max_barrier_wait_ms: DEFAULT_BARRIER_WAIT_MS,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct StreamsConfig {
    #[schemars(schema_with = "schema::positive_integer::<{ u64::MAX }>")]
    pub poll_interval_ms: u64,
    #[schemars(schema_with = "schema::positive_integer::<{ usize::MAX as u64 }>")]
    pub max_gap_ranges: usize,
    #[schemars(schema_with = "schema::positive_integer::<{ u32::MAX as u64 }>")]
    pub keepalive_interval_ms: u64,
    #[schemars(schema_with = "schema::positive_integer::<{ u64::MAX }>")]
    pub max_pong_wait_ms: u64,
}

impl Default for StreamsConfig {
    fn default() -> Self {
        Self {
            poll_interval_ms: DEFAULT_POLL_INTERVAL_MS,
            max_gap_ranges: DEFAULT_MAX_GAP_RANGES,
            keepalive_interval_ms: DEFAULT_KEEPALIVE_INTERVAL_MS,
            max_pong_wait_ms: DEFAULT_MAX_PONG_WAIT_MS,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct RetentionConfig {
    #[schemars(range(min = 1, max = MAX_RETENTION_SECONDS))]
    pub group_message_seconds: u64,
    #[schemars(range(min = 1, max = MAX_RETENTION_SECONDS))]
    pub welcome_seconds: u64,
    #[schemars(range(min = 1, max = MAX_RETENTION_SECONDS))]
    pub key_package_seconds: u64,
}

impl Default for RetentionConfig {
    fn default() -> Self {
        Self {
            group_message_seconds: BACKEND_DEFAULT_GROUP_MESSAGE_SECONDS,
            welcome_seconds: BACKEND_DEFAULT_WELCOME_SECONDS,
            key_package_seconds: BACKEND_DEFAULT_KEY_PACKAGE_SECONDS,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct ValidationConfig {
    #[schemars(schema_with = "schema::positive_integer::<{ usize::MAX as u64 }>")]
    pub max_scw_cache_entries: usize,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            max_scw_cache_entries: DEFAULT_MAX_SCW_CACHE_ENTRIES,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct LimitsConfig {
    #[schemars(schema_with = "schema::positive_integer::<{ usize::MAX as u64 }>")]
    pub max_query_topics: usize,
    #[schemars(schema_with = "schema::positive_integer::<{ i64::MAX as u64 - 1 }>")]
    pub default_query_limit: usize,
    #[schemars(schema_with = "schema::positive_integer::<{ i64::MAX as u64 - 1 }>")]
    pub max_query_limit: usize,
    #[schemars(schema_with = "schema::positive_integer::<{ usize::MAX as u64 }>")]
    pub max_newest_metadata_topics: usize,
    #[schemars(schema_with = "schema::positive_integer::<{ usize::MAX as u64 }>")]
    pub max_newest_full_topics: usize,
    #[schemars(schema_with = "schema::positive_integer::<{ usize::MAX as u64 }>")]
    pub max_publish_topics: usize,
    #[schemars(schema_with = "schema::positive_integer::<{ usize::MAX as u64 }>")]
    pub max_envelope_bytes: usize,
    #[schemars(schema_with = "schema::positive_integer::<{ usize::MAX as u64 }>")]
    pub max_request_bytes: usize,
    #[schemars(schema_with = "schema::positive_integer::<{ usize::MAX as u64 }>")]
    pub max_response_bytes: usize,
    #[schemars(schema_with = "schema::positive_integer::<{ usize::MAX as u64 }>")]
    pub max_update_adds: usize,
    #[schemars(schema_with = "schema::positive_integer::<{ usize::MAX as u64 }>")]
    pub max_update_removes: usize,
    #[schemars(schema_with = "schema::positive_integer::<{ usize::MAX as u64 }>")]
    pub max_stream_topics: usize,
    #[schemars(schema_with = "schema::positive_integer::<{ usize::MAX as u64 }>")]
    pub max_static_topics: usize,
    #[schemars(schema_with = "schema::positive_integer::<{ usize::MAX as u64 }>")]
    pub max_lookup_identifiers: usize,
    #[schemars(schema_with = "schema::positive_integer::<{ usize::MAX as u64 }>")]
    pub max_scw_signatures: usize,
    #[schemars(schema_with = "schema::positive_integer::<{ usize::MAX as u64 }>")]
    pub max_identity_entries: usize,
    #[schemars(schema_with = "schema::positive_integer::<{ u32::MAX as u64 }>")]
    pub max_http2_streams: usize,
    #[schemars(schema_with = "schema::positive_integer::<{ u32::MAX as u64 }>")]
    pub max_update_frames_per_second: u32,
    #[schemars(schema_with = "schema::positive_integer::<{ u32::MAX as u64 }>")]
    pub max_update_burst: u32,
    #[schemars(schema_with = "schema::positive_integer::<{ u32::MAX as u64 }>")]
    pub max_ping_frames_per_second: u32,
    #[schemars(schema_with = "schema::positive_integer::<{ u32::MAX as u64 }>")]
    pub max_ping_burst: u32,
}

impl LimitsConfig {
    /// Check positive limits, downstream integer ranges, and related capacities.
    fn validate(&self) -> Result<(), ConfigError> {
        positive(self.max_query_topics, "limits.max_query_topics")?;
        positive(self.default_query_limit, "limits.default_query_limit")?;
        positive(self.max_query_limit, "limits.max_query_limit")?;
        if self.max_query_limit < self.default_query_limit {
            return Err(invalid(
                "limits.max_query_limit",
                "must be at least limits.default_query_limit",
            ));
        }
        if self.max_query_limit >= i64::MAX as usize {
            return Err(invalid(
                "limits.max_query_limit",
                "must fit the database limit type",
            ));
        }
        positive(
            self.max_newest_metadata_topics,
            "limits.max_newest_metadata_topics",
        )?;
        positive(self.max_newest_full_topics, "limits.max_newest_full_topics")?;
        positive(self.max_publish_topics, "limits.max_publish_topics")?;
        positive(self.max_envelope_bytes, "limits.max_envelope_bytes")?;
        positive(self.max_request_bytes, "limits.max_request_bytes")?;
        positive(self.max_response_bytes, "limits.max_response_bytes")?;
        self.validate_envelope_fit()?;
        positive(self.max_update_adds, "limits.max_update_adds")?;
        positive(self.max_update_removes, "limits.max_update_removes")?;
        positive(self.max_stream_topics, "limits.max_stream_topics")?;
        if self.max_update_adds > self.max_stream_topics {
            return Err(invalid(
                "limits.max_update_adds",
                "must not exceed limits.max_stream_topics",
            ));
        }
        positive(self.max_static_topics, "limits.max_static_topics")?;
        positive(self.max_lookup_identifiers, "limits.max_lookup_identifiers")?;
        positive(self.max_scw_signatures, "limits.max_scw_signatures")?;
        positive(self.max_identity_entries, "limits.max_identity_entries")?;
        positive(self.max_http2_streams, "limits.max_http2_streams")?;
        if self.max_http2_streams > u32::MAX as usize {
            return Err(invalid(
                "limits.max_http2_streams",
                "must fit the HTTP/2 stream limit type",
            ));
        }
        positive(
            self.max_update_frames_per_second,
            "limits.max_update_frames_per_second",
        )?;
        positive(self.max_update_burst, "limits.max_update_burst")?;
        positive(
            self.max_ping_frames_per_second,
            "limits.max_ping_frames_per_second",
        )?;
        positive(self.max_ping_burst, "limits.max_ping_burst")
    }

    /// Reserve worst-case metadata and framing in each request and delivery budget.
    /// Saturating arithmetic ensures oversized configured values cannot wrap to fit.
    fn validate_envelope_fit(&self) -> Result<(), ConfigError> {
        let request_bytes = self.max_envelope_bytes.saturating_add(
            1 + prost::encoding::encoded_len_varint(self.max_envelope_bytes as u64),
        );
        // This covers the largest legal topic, hash, cursor, metadata, protobuf length prefixes,
        // nested server-envelope tags, stream message tags, and the gRPC five-byte frame header.
        let bounded_payload = self
            .max_envelope_bytes
            .saturating_add(ENVELOPE_METADATA_AND_FRAMING_BYTES);
        if request_bytes > self.max_request_bytes
            || bounded_payload > DELIVERY_FRAME_BYTES
            || bounded_payload > self.max_response_bytes
            || bounded_payload > FETCH_BUFFER_BYTES
            || bounded_payload > OUTBOUND_QUEUE_BYTES
        {
            return Err(invalid(
                "limits.max_envelope_bytes",
                "envelope plus framing must fit all transport budgets",
            ));
        }
        Ok(())
    }
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            max_query_topics: BACKEND_DEFAULT_MAX_QUERY_TOPICS,
            default_query_limit: BACKEND_DEFAULT_QUERY_LIMIT,
            max_query_limit: BACKEND_DEFAULT_MAX_QUERY_LIMIT,
            max_newest_metadata_topics: BACKEND_DEFAULT_MAX_NEWEST_METADATA_TOPICS,
            max_newest_full_topics: BACKEND_DEFAULT_MAX_NEWEST_FULL_TOPICS,
            max_publish_topics: BACKEND_DEFAULT_MAX_PUBLISH_TOPICS,
            max_envelope_bytes: BACKEND_DEFAULT_MAX_ENVELOPE_BYTES,
            max_request_bytes: BACKEND_DEFAULT_MAX_REQUEST_BYTES,
            max_response_bytes: BACKEND_DEFAULT_MAX_RESPONSE_BYTES,
            max_update_adds: BACKEND_DEFAULT_MAX_UPDATE_ADDS,
            max_update_removes: BACKEND_DEFAULT_MAX_UPDATE_REMOVES,
            max_stream_topics: BACKEND_DEFAULT_MAX_STREAM_TOPICS,
            max_static_topics: BACKEND_DEFAULT_MAX_STATIC_TOPICS,
            max_lookup_identifiers: BACKEND_DEFAULT_MAX_LOOKUP_IDENTIFIERS,
            max_scw_signatures: BACKEND_DEFAULT_MAX_SCW_SIGNATURES,
            max_identity_entries: BACKEND_DEFAULT_MAX_IDENTITY_ENTRIES,
            max_http2_streams: BACKEND_DEFAULT_MAX_HTTP2_STREAMS,
            max_update_frames_per_second: BACKEND_DEFAULT_MAX_UPDATE_FRAMES_PER_SECOND,
            max_update_burst: BACKEND_DEFAULT_MAX_UPDATE_BURST,
            max_ping_frames_per_second: BACKEND_DEFAULT_MAX_PING_FRAMES_PER_SECOND,
            max_ping_burst: BACKEND_DEFAULT_MAX_PING_BURST,
        }
    }
}

#[cfg(test)]
mod tests;
