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
const ENVELOPE_METADATA_AND_FRAMING_BYTES: usize =
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

fn resolve_url(value: &str, field: &'static str) -> Result<String, ConfigError> {
    resolve_env(value).map_err(|error| match error {
        EnvironmentError::Missing { name } => ConfigError::Environment { name },
        EnvironmentError::EmptyName => invalid(field, "environment variable name is empty"),
    })
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
    fn validate(&self) -> Result<(), ConfigError> {
        if self.listen.parse::<SocketAddr>().is_err() {
            return Err(invalid("server.listen", "must be a socket address"));
        }
        positive(self.max_drain_duration_ms, "server.max_drain_duration_ms")
    }
}

impl DatabaseConfig {
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
    fn validate(&self) -> Result<(), ConfigError> {
        positive(self.poll_interval_ms, "streams.poll_interval_ms")?;
        positive(self.max_gap_ranges, "streams.max_gap_ranges")?;
        positive(self.keepalive_interval_ms, "streams.keepalive_interval_ms")?;
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
    pub listen: String,
    #[schemars(range(min = 1))]
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
    pub url: String,
    #[serde(default)]
    pub replica_url: Option<String>,
    #[serde(default = "default_max_connections")]
    #[schemars(range(min = 1))]
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
    #[schemars(range(min = 1))]
    pub poll_interval_ms: u64,
    #[schemars(range(min = 1))]
    pub max_gap_ranges: usize,
    #[schemars(range(min = 1))]
    pub keepalive_interval_ms: u64,
    #[schemars(range(min = 1))]
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
    #[schemars(range(min = 1))]
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
    #[schemars(range(min = 1))]
    pub max_query_topics: usize,
    #[schemars(range(min = 1))]
    pub default_query_limit: usize,
    #[schemars(range(min = 1))]
    pub max_query_limit: usize,
    #[schemars(range(min = 1))]
    pub max_newest_metadata_topics: usize,
    #[schemars(range(min = 1))]
    pub max_newest_full_topics: usize,
    #[schemars(range(min = 1))]
    pub max_publish_topics: usize,
    #[schemars(range(min = 1))]
    pub max_envelope_bytes: usize,
    #[schemars(range(min = 1))]
    pub max_request_bytes: usize,
    #[schemars(range(min = 1))]
    pub max_response_bytes: usize,
    #[schemars(range(min = 1))]
    pub max_update_adds: usize,
    #[schemars(range(min = 1))]
    pub max_update_removes: usize,
    #[schemars(range(min = 1))]
    pub max_stream_topics: usize,
    #[schemars(range(min = 1))]
    pub max_static_topics: usize,
    #[schemars(range(min = 1))]
    pub max_lookup_identifiers: usize,
    #[schemars(range(min = 1))]
    pub max_scw_signatures: usize,
    #[schemars(range(min = 1))]
    pub max_identity_entries: usize,
    #[schemars(range(min = 1))]
    pub max_http2_streams: usize,
    #[schemars(range(min = 1))]
    pub max_update_frames_per_second: u32,
    #[schemars(range(min = 1))]
    pub max_update_burst: u32,
    #[schemars(range(min = 1))]
    pub max_ping_frames_per_second: u32,
    #[schemars(range(min = 1))]
    pub max_ping_burst: u32,
}

impl LimitsConfig {
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
mod tests {
    use super::*;

    const MINIMAL: &str = "[database]\nurl = 'postgres://localhost/xmtp'\n";
    const RESPONSE_TEST_ENVELOPE_BYTES: usize = 1_000_000;
    const RESPONSE_TEST_REQUEST_BYTES: usize = 2_000_000;

    #[xmtp_common::test(unwrap_try = true)]
    fn minimal_configuration_uses_defaults() {
        let config: Config = toml::from_str(MINIMAL)?;
        config.validate()?;

        assert_eq!(config.server.listen, DEFAULT_LISTEN);
        assert_eq!(
            config.limits.max_request_bytes,
            BACKEND_DEFAULT_MAX_REQUEST_BYTES
        );
        assert!(config.chains.is_empty());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn unknown_keys_are_rejected() {
        let error = toml::from_str::<Config>(
            "[database]\nurl = 'postgres://localhost/xmtp'\nextra = true\n",
        )
        .expect_err("unknown keys must fail");
        assert!(error.to_string().contains("unknown field"));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn invalid_relationship_is_rejected() {
        let mut config: Config = toml::from_str(MINIMAL)?;
        config.limits.max_envelope_bytes = DELIVERY_FRAME_BYTES;
        config.limits.max_request_bytes = DELIVERY_FRAME_BYTES;
        config.limits.max_response_bytes = DELIVERY_FRAME_BYTES;
        assert!(config.validate().is_err());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn framing_bound_covers_maximum_metadata_and_nested_messages() {
        use crate::api;
        use prost::Message;

        const STORED_TOPIC_BYTES: usize = 128;
        const HASH_BYTES: usize = 32;
        let meta = api::EnvelopeMeta {
            cursor: Some(api::Cursor {
                sequence_id: u64::MAX,
            }),
            server_ns: u64::MAX,
            message_hash: Some(api::MessageHash {
                hash: Some(api::message_hash::Hash::Sha256(vec![0; HASH_BYTES])),
            }),
            topic: Some(api::Topic {
                topic: vec![0; STORED_TOPIC_BYTES],
            }),
            expiry_ns: u64::MAX,
            is_commit_or_proposal: true,
        };
        assert_eq!(meta.encoded_len(), MAX_METADATA_BYTES);
        for size in [
            0,
            127,
            128,
            16_383,
            16_384,
            BACKEND_DEFAULT_MAX_ENVELOPE_BYTES,
        ] {
            let envelope = api::ClientEnvelope {
                payload: Some(api::client_envelope::Payload::GroupMessage(
                    api::GroupMessage {
                        data: vec![0; size],
                        ..Default::default()
                    },
                )),
            };
            let bound = envelope.encoded_len() + ENVELOPE_METADATA_AND_FRAMING_BYTES;
            let response = api::SubscribeResponse {
                response: Some(api::subscribe_response::Response::Messages(
                    api::subscribe_response::Messages {
                        envelopes: vec![api::ServerEnvelope {
                            meta: Some(meta.clone()),
                            envelope: Some(envelope),
                        }],
                    },
                )),
            };
            assert!(response.encoded_len() + GRPC_HEADER_BYTES <= bound);
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn every_cross_field_relationship_is_rejected() {
        let mut config: Config = toml::from_str(MINIMAL)?;

        config.publishing.max_publish_duration_ms = config.database.max_statement_timeout_ms;
        assert!(config.validate().is_err());
        config = toml::from_str(MINIMAL)?;
        config.streams.max_pong_wait_ms = config.streams.keepalive_interval_ms;
        assert!(config.validate().is_err());
        config = toml::from_str(MINIMAL)?;
        config.limits.max_query_limit = config.limits.default_query_limit - 1;
        assert!(config.validate().is_err());
        config = toml::from_str(MINIMAL)?;
        config.limits.max_envelope_bytes = config.limits.max_request_bytes + 1;
        assert!(config.validate().is_err());
        config = toml::from_str(MINIMAL)?;
        config.limits.max_envelope_bytes = RESPONSE_TEST_ENVELOPE_BYTES;
        config.limits.max_request_bytes = RESPONSE_TEST_REQUEST_BYTES;
        config.limits.max_response_bytes = RESPONSE_TEST_ENVELOPE_BYTES;
        assert!(config.validate().is_err());
        config = toml::from_str(MINIMAL)?;
        config.limits.max_update_adds = config.limits.max_stream_topics + 1;
        assert!(config.validate().is_err());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn missing_and_empty_environment_references_fail_without_values() {
        let missing = write_config(
            "missing",
            "[database]\nurl = 'env:XMTP_BACKEND_CONFIG_MISSING_9F31'\n",
        );
        let missing_error = Config::load(&missing).expect_err("missing env must fail");
        assert!(
            missing_error
                .to_string()
                .contains("XMTP_BACKEND_CONFIG_MISSING_9F31")
        );
        std::fs::remove_file(missing)?;

        let empty = write_config("empty", "[database]\nurl = 'env:'\n");
        let empty_error = Config::load(&empty).expect_err("empty env must fail");
        assert!(!empty_error.to_string().contains("env:"));
        std::fs::remove_file(empty)?;
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn debug_output_redacts_urls() {
        let config: Config =
            toml::from_str("[database]\nurl = 'postgres://user:password@localhost/xmtp'\n")?;
        let debug = format!("{config:?}");
        assert!(!debug.contains("password"));
        assert!(debug.contains("<redacted>"));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn generated_schema_matches_published_scalar_surface() {
        let generated = serde_json::to_value(Config::schema())?;
        let published: serde_json::Value =
            serde_json::from_str(include_str!("../../../docs/schemas/backend-v1.json"))?;
        assert_eq!(generated, published);
        assert_eq!(generated["$id"], SCHEMA_ID);
        for section in [
            "server",
            "database",
            "publishing",
            "streams",
            "retention",
            "validation",
            "limits",
        ] {
            assert!(generated["properties"][section].is_object());
            assert!(published["properties"][section].is_object());
        }
        for field in [
            "max_query_topics",
            "default_query_limit",
            "max_query_limit",
            "max_newest_metadata_topics",
            "max_newest_full_topics",
            "max_publish_topics",
            "max_envelope_bytes",
            "max_request_bytes",
            "max_response_bytes",
            "max_update_adds",
            "max_update_removes",
            "max_stream_topics",
            "max_static_topics",
            "max_lookup_identifiers",
            "max_scw_signatures",
            "max_identity_entries",
            "max_http2_streams",
            "max_update_frames_per_second",
            "max_update_burst",
            "max_ping_frames_per_second",
            "max_ping_burst",
        ] {
            assert!(schema_property(&generated, "limits", field).is_object());
            assert_eq!(schema_property(&published, "limits", field)["minimum"], 1);
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn environment_urls_resolve_once() {
        let path = std::env::var("PATH")?;
        assert_eq!(resolve_env("env:PATH")?, path);
    }

    fn write_config(name: &str, source: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "xmtp-backend-config-{}-{name}.toml",
            std::process::id()
        ));
        std::fs::write(&path, source).expect("write test config");
        path
    }

    fn schema_property<'a>(
        schema: &'a serde_json::Value,
        section: &str,
        field: &str,
    ) -> &'a serde_json::Value {
        let section_schema = &schema["properties"][section];
        let section_schema = section_schema["$ref"]
            .as_str()
            .and_then(|reference| reference.strip_prefix('#'))
            .and_then(|reference| schema.pointer(reference))
            .unwrap_or(section_schema);
        &section_schema["properties"][field]
    }
}
