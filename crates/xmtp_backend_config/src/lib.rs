//! Backend configuration loaded from a TOML file.

use std::{
    collections::BTreeMap,
    fs,
    net::{IpAddr, SocketAddr},
    num::NonZeroUsize,
    path::Path,
};

use schemars::{JsonSchema, Schema};
use serde::{Deserialize, Serialize};
use thiserror::Error;
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
    BACKEND_DEFAULT_QUERY_LIMIT, BACKEND_DEFAULT_WELCOME_SECONDS, MAX_SCW_SIGNATURES_PER_REQUEST,
    MAX_SERVER_IDENTIFIER_BYTES, MAX_TRANSPORT_BYTES, NOTIFICATION_BATCH_MAX_BYTES,
    parse_eip155_chain_id, validate_server_identifier,
};

pub mod application_components;
pub mod attachments;
pub mod auth;
pub mod handles;
pub mod push;
pub mod rate_limits;
mod schema;
mod telemetry;
mod validated;
pub use rate_limits::{AuthType, CallerKey, CallerLimits, Category, LimitConfig, RateLimitConfig};
pub use telemetry::{IDENTIFIER_ATTRIBUTE, TelemetryConfig};
pub use validated::{Validated, ValidatedConfig};

const SCHEMA_ID: &str =
    "https://raw.githubusercontent.com/xmtp/libxmtp/main/docs/schemas/backend-v1.json";

// These values are server implementation details. They are not configuration keys.
/// The largest frame one stream delivery writes.
pub const DELIVERY_FRAME_BYTES: usize = 2 * 1024 * 1024;
/// Maximum metadata with a 128-byte stored topic and ten-byte scalar varints.
pub const MAX_METADATA_BYTES: usize = 207;
const MAX_NESTED_FIELD_OVERHEAD: usize = 11;
/// The gRPC message header: a compression flag and a four-byte length.
pub const GRPC_HEADER_BYTES: usize = 5;
/// The bytes one delivered envelope may need beyond its own encoding.
pub const ENVELOPE_METADATA_AND_FRAMING_BYTES: usize =
    MAX_METADATA_BYTES + 4 * MAX_NESTED_FIELD_OVERHEAD + GRPC_HEADER_BYTES;
const NS_IN_SEC: i64 = 1_000_000_000;
const MAX_RETENTION_SECONDS: u64 = i64::MAX as u64 / NS_IN_SEC as u64;
const MAX_DATABASE_TIMEOUT_MS: u64 = i32::MAX as u64;
/// Advisory MLS group shapes are published as `uint32` and bounded here.
pub const MAX_MLS_LIMIT: u32 = u16::MAX as u32;
/// The most installations per inbox a deployment may publish: one identity
/// update's actions.
pub const MAX_INSTALLATIONS_LIMIT: u32 = xmtp_configuration::MAX_IDENTITY_UPDATE_ACTIONS as u32;

const DEFAULT_LISTEN: &str = "0.0.0.0:5050";
const DEFAULT_DRAIN_DURATION_MS: u64 = 10_000;
const DEFAULT_MAX_CONNECTIONS: u32 = 20;
const DEFAULT_PUBLISH_DURATION_MS: u64 = 10_000;
const DEFAULT_BARRIER_WAIT_MS: u64 = 1_000;
const DEFAULT_POLL_INTERVAL_MS: u64 = 100;
const DEFAULT_MAX_GAP_RANGES: usize = 10_000;
const DEFAULT_KEEPALIVE_INTERVAL_MS: u64 =
    xmtp_configuration::BACKEND_DEFAULT_KEEPALIVE_INTERVAL_MS;
const DEFAULT_MAX_PONG_WAIT_MS: u64 = 90_000;
const DEFAULT_MAX_SCW_CACHE_ENTRIES: usize = 10_000;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("configuration is invalid: {field} ({reason})")]
    Auth { field: String, reason: &'static str },
    #[error("could not read configuration file")]
    Read(#[source] std::io::Error),
    #[error("configuration is not valid TOML")]
    Parse,
    #[error("configuration is invalid: {field} ({reason})")]
    Invalid { field: String, reason: &'static str },
    #[error("configuration is invalid: {field} (environment variable {name} is not available)")]
    Environment { field: String, name: String },
    #[error("configuration is invalid: {field} ({reason})")]
    ApplicationComponent {
        field: String,
        reason: xmtp_configuration::ApplicationComponentError,
    },
    #[error("configuration is invalid: {field} (name must not equal a well-known component name)")]
    WellKnownComponentName { field: String },
}

impl ConfigError {
    /// The key the refusal names, as a TOML dotted key. A duplicate API key
    /// value names both keys, in sorted order and joined by `/`:
    /// `auth.api_keys.<a>/<b>`. `Read` and `Parse` name none.
    pub fn field(&self) -> Option<&str> {
        match self {
            Self::Read(_) | Self::Parse => None,
            Self::Auth { field, .. }
            | Self::Invalid { field, .. }
            | Self::Environment { field, .. }
            | Self::ApplicationComponent { field, .. }
            | Self::WellKnownComponentName { field } => Some(field),
        }
    }
}

#[derive(Debug, Error)]
enum EnvironmentError {
    #[error("environment variable {name} is not available")]
    Missing { name: String },
    #[error("environment variable name is empty")]
    EmptyName,
}

/// One step below a key: a key name, or the position of an array entry.
#[derive(Clone, Copy)]
pub(crate) enum Step<'a> {
    Key(&'a str),
    Entry(usize),
}

/// The key `parent` then `step`, written as a TOML dotted key. A key name
/// that is not a bare key (`A-Za-z0-9_-`) is quoted, and an array entry is
/// `[N]`: `chains."eip155:1"`, `auth.keys[0]`. Every refusal that names a key
/// from the document builds it here.
pub(crate) fn key_path(parent: &str, step: Step<'_>) -> String {
    use std::fmt::Write;
    let mut path = parent.to_owned();
    match step {
        Step::Entry(index) => {
            let _ = write!(path, "[{index}]");
        }
        Step::Key(name) => {
            if !path.is_empty() {
                path.push('.');
            }
            let bare = !name.is_empty()
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'));
            if bare {
                path.push_str(name);
            } else {
                path.push('"');
                for character in name.chars() {
                    match character {
                        '"' => path.push_str("\\\""),
                        '\\' => path.push_str("\\\\"),
                        character if character.is_control() => {
                            let _ = write!(path, "\\u{:04X}", u32::from(character));
                        }
                        character => path.push(character),
                    }
                }
                path.push('"');
            }
        }
    }
    path
}

/// Name the key of a typed-decoding error, with a fixed reason chosen by how
/// its message starts. The message is never shown, because it can hold the
/// value.
fn decode_error(error: &serde_path_to_error::Error<toml::de::Error>) -> ConfigError {
    use serde_path_to_error::Segment;
    let mut field = String::new();
    for segment in error.path() {
        let step = match segment {
            Segment::Seq { index } => Step::Entry(*index),
            Segment::Map { key } | Segment::Enum { variant: key } => Step::Key(key),
            // A key that is not a string. A TOML document has none.
            Segment::Unknown => break,
        };
        field = key_path(&field, step);
    }
    let message = error.inner().message();
    let named = |prefix: &str| {
        message
            .strip_prefix(prefix)
            .and_then(|rest| rest.split('`').next())
    };
    let reason = if let Some(name) = named("unknown field `") {
        // The path already ends at a key that the derived decoder refuses, and
        // the message names that whole key, which may itself hold a backtick.
        // The path ends at the table when a whole-table check refuses the
        // key, as a credential kind does for a key it does not take; those
        // names have no backtick.
        let at_key = matches!(
            error.path().iter().next_back(),
            Some(Segment::Map { key }) if message
                .strip_prefix(&format!("unknown field `{key}`"))
                .is_some_and(|rest| rest.is_empty() || rest.starts_with(','))
        );
        if !at_key {
            field = key_path(&field, Step::Key(name));
        }
        "is not a configuration key"
    } else if let Some(name) = named("missing field `") {
        field = key_path(&field, Step::Key(name));
        "is required"
    } else if message.starts_with("unknown variant") {
        "is not one of the allowed values"
    } else {
        "has the wrong type or value"
    };
    ConfigError::Invalid { field, reason }
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attachments: Option<attachments::AttachmentsConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<auth::AuthConfig>,
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub telemetry: TelemetryConfig,
    pub database: DatabaseConfig,
    #[serde(default)]
    pub publishing: PublishingConfig,
    #[serde(default)]
    pub streams: StreamsConfig,
    #[serde(default)]
    pub retention: RetentionConfig,
    #[serde(default)]
    pub push: push::PushConfig,
    #[serde(default)]
    #[schemars(schema_with = "schema::chains")]
    pub chains: BTreeMap<String, String>,
    #[serde(default)]
    pub validation: ValidationConfig,
    #[serde(default)]
    pub limits: LimitsConfig,
    #[serde(default)]
    pub rate_limits: rate_limits::RateLimitsSection,
    #[serde(default)]
    pub mls: MlsConfig,
    #[serde(default)]
    pub handles: handles::HandlesConfig,
    /// Group metadata fields clients register in the conversations they
    /// create. Published sorted by `component_id`.
    #[serde(default)]
    pub application_components: Vec<application_components::ApplicationComponentConfig>,
}

impl Config {
    /// Read one TOML file and resolve its environment references. Nothing is
    /// checked until `validate`.
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let contents = fs::read_to_string(path).map_err(ConfigError::Read)?;
        Self::from_toml_str(&contents)
    }

    /// Parse one TOML document and resolve its environment references.
    /// Nothing is checked until `validate`. Errors omit document contents.
    /// This method cannot return `ConfigError::Read`.
    pub fn from_toml_str(contents: &str) -> Result<Self, ConfigError> {
        let mut value: toml::Value = toml::from_str(contents).map_err(|_| ConfigError::Parse)?;
        resolve_environment(&mut value, "")?;
        serde_path_to_error::deserialize(value).map_err(|error| decode_error(&error))
    }

    /// Check every rule the document and the environment decide: scalar
    /// values and the relationships between values. A `ValidatedConfig`
    /// comes only from here.
    pub fn validate(self) -> Result<ValidatedConfig, ConfigError> {
        let (scw_cache_entries, rate_limits) = self.check()?;
        Ok(ValidatedConfig::new(self, scw_cache_entries, rate_limits))
    }

    /// Every document check. Returns the checked values `ValidatedConfig`
    /// keeps.
    fn check(&self) -> Result<(NonZeroUsize, RateLimitConfig), ConfigError> {
        if let Some(attachments) = &self.attachments {
            attachments.validate()?;
        }
        if let Some(auth) = &self.auth {
            auth.validate()?;
        }
        for (field, milliseconds) in [
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
            (
                "streams.keepalive_interval_ms",
                self.streams.keepalive_interval_ms,
            ),
        ] {
            validate_deadline(milliseconds, field)?;
        }
        self.server.validate()?;
        self.telemetry.validate()?;
        self.database.validate()?;
        self.publishing
            .validate(self.database.max_statement_timeout_ms)?;
        self.streams.validate()?;
        self.retention.validate()?;
        self.push.validate()?;
        let scw_cache_entries = self.validation.validate()?;
        self.validate_chains()?;
        self.limits.validate()?;
        let rate_limits = self.rate_limits.validate()?;
        self.mls.validate()?;
        self.handles.validate()?;
        application_components::validate(&self.application_components)?;
        Ok((scw_cache_entries, rate_limits))
    }

    /// Require chain keys that the SCW verifier can route and HTTP(S) RPC URLs.
    fn validate_chains(&self) -> Result<(), ConfigError> {
        for (chain, url) in &self.chains {
            let field = key_path("chains", Step::Key(chain));
            if parse_eip155_chain_id(chain).is_err() {
                return Err(invalid(field, "keys must be EIP-155 CAIP-2 chain ids"));
            }
            non_empty_url(url, &field, UrlKind::Http)?;
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
            .field("auth", &self.auth)
            .field("attachments", &self.attachments)
            .field("server", &self.server)
            .field("database", &self.database)
            .field("publishing", &self.publishing)
            .field("streams", &self.streams)
            .field("retention", &self.retention)
            .field("push", &self.push)
            .field(
                "chains",
                &format_args!("{} configured chain(s)", self.chains.len()),
            )
            .field("validation", &self.validation)
            .field("limits", &self.limits)
            .field("rate_limits", &self.rate_limits)
            .field("mls", &self.mls)
            .field("handles", &self.handles)
            .field("application_components", &self.application_components)
            .finish()
    }
}

/// Resolve string references once, before typed decoding, including string
/// enums, and refuse a date or a time. An error names the key at `path` and
/// the variable, never a value. Resolved values are not recursively expanded.
fn resolve_environment(value: &mut toml::Value, path: &str) -> Result<(), ConfigError> {
    match value {
        toml::Value::String(text) => {
            *text = resolve_env(text).map_err(|error| match error {
                EnvironmentError::Missing { name } => ConfigError::Environment {
                    field: path.to_owned(),
                    name,
                },
                EnvironmentError::EmptyName => invalid(path, "environment variable name is empty"),
            })?;
        }
        toml::Value::Array(values) => {
            for (index, value) in values.iter_mut().enumerate() {
                resolve_environment(value, &key_path(path, Step::Entry(index)))?;
            }
        }
        toml::Value::Table(values) => {
            for (name, value) in values.iter_mut() {
                resolve_environment(value, &key_path(path, Step::Key(name)))?;
            }
        }
        // No key takes a date or a time, and the decoder would read one as
        // text, so a string key would accept it.
        toml::Value::Datetime(_) => return Err(invalid(path, "has the wrong type or value")),
        _ => {}
    }
    Ok(())
}

/// A timer without a fixed maximum must still fit the host's monotonic clock.
fn validate_deadline(milliseconds: u64, field: &str) -> Result<(), ConfigError> {
    if std::time::Instant::now()
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

fn invalid(field: impl Into<String>, reason: &'static str) -> ConfigError {
    ConfigError::Invalid {
        field: field.into(),
        reason,
    }
}

/// The maximum of a published count limit and of a timer with a fixed bound:
/// the largest unsigned 32-bit integer.
pub(crate) const MAX_U32_KEY: u64 = u32::MAX as u64;

fn at_most_u32(value: u64, field: &str) -> Result<(), ConfigError> {
    if value > MAX_U32_KEY {
        Err(invalid(field, "must not exceed 4294967295"))
    } else {
        Ok(())
    }
}

fn positive<T>(value: T, field: &str) -> Result<(), ConfigError>
where
    T: PartialEq + PartialOrd + Default,
{
    if value <= T::default() {
        Err(invalid(field, "must be greater than zero"))
    } else {
        Ok(())
    }
}

/// The `ip address` standard check: an IPv4 address in dotted decimal with no
/// leading zero, or an IPv6 address without brackets or a zone index.
fn ip_address(value: &str) -> Option<IpAddr> {
    value.parse().ok()
}

/// The `ip network` standard check: an `ip address`, `/`, and a decimal
/// prefix length that fits the address, with every bit after the prefix 0.
fn ip_network(value: &str) -> Option<ipnet::IpNet> {
    let (address, prefix) = value.split_once('/')?;
    // `u8::from_str` also takes a leading `+`.
    if prefix.is_empty() || !prefix.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let address = ip_address(address)?;
    let network = ipnet::IpNet::new(address, prefix.parse().ok()?).ok()?;
    (network.network() == address).then_some(network)
}

#[derive(Clone, Copy)]
enum UrlKind {
    Postgres,
    Http,
}

/// Check a resolved URL and its allowed scheme without including the URL in errors.
fn non_empty_url(value: &str, field: &str, kind: UrlKind) -> Result<(), ConfigError> {
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
    /// Require a numeric bind address and a positive shutdown budget, plus the
    /// deployment identity clients bind to.
    // implements: CONF-002, CONF-067
    fn validate(&self) -> Result<(), ConfigError> {
        let Some(identifier) = self.identifier.as_deref() else {
            return Err(invalid(
                "server.identifier",
                "must be set to a stable name for this deployment",
            ));
        };
        if validate_server_identifier(identifier).is_err() {
            return Err(invalid(
                "server.identifier",
                "must be 1 to 256 bytes with no whitespace or control characters",
            ));
        }
        if let Some(version) = self.min_libxmtp_version.as_deref()
            && semver::Version::parse(version).is_err()
        {
            return Err(invalid(
                "server.min_libxmtp_version",
                "must be a semantic version",
            ));
        }
        if self.listen.parse::<SocketAddr>().is_err() {
            return Err(invalid("server.listen", "must be a socket address"));
        }
        positive(self.max_drain_duration_ms, "server.max_drain_duration_ms")?;
        at_most_u32(self.max_drain_duration_ms, "server.max_drain_duration_ms")
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
        for (field, milliseconds) in [
            (
                "publishing.max_publish_duration_ms",
                self.max_publish_duration_ms,
            ),
            ("publishing.max_barrier_wait_ms", self.max_barrier_wait_ms),
        ] {
            if milliseconds > MAX_DATABASE_TIMEOUT_MS {
                return Err(invalid(field, "exceeds the Postgres timeout range"));
            }
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
        at_most_u32(self.poll_interval_ms, "streams.poll_interval_ms")?;
        positive(self.max_gap_ranges, "streams.max_gap_ranges")?;
        positive(self.keepalive_interval_ms, "streams.keepalive_interval_ms")?;
        if self.keepalive_interval_ms > u32::MAX as u64 {
            return Err(invalid(
                "streams.keepalive_interval_ms",
                "must fit the Started keepalive interval type",
            ));
        }
        positive(self.max_pong_wait_ms, "streams.max_pong_wait_ms")?;
        at_most_u32(self.max_pong_wait_ms, "streams.max_pong_wait_ms")
    }
}

impl RetentionConfig {
    /// Check finite expiry against the primary database's startup clock. The
    /// insert still checks arithmetic after later clock changes or time advances.
    pub fn validate_at(&self, database_ns: i64) -> Result<(), ConfigError> {
        for (field, seconds) in self.periods() {
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
        for (field, seconds) in self.periods() {
            positive(seconds, field)?;
            if seconds > MAX_RETENTION_SECONDS {
                return Err(invalid(
                    field,
                    "duration is too large for nanosecond expiry arithmetic",
                ));
            }
        }
        Ok(())
    }

    /// Each retention period with its key.
    fn periods(&self) -> [(&'static str, u64); 3] {
        [
            (
                "retention.group_message_seconds",
                self.group_message_seconds,
            ),
            ("retention.welcome_seconds", self.welcome_seconds),
            ("retention.key_package_seconds", self.key_package_seconds),
        ]
    }
}

impl ValidationConfig {
    /// The signature cache capacity, which must be greater than zero.
    fn validate(&self) -> Result<NonZeroUsize, ConfigError> {
        NonZeroUsize::new(self.max_scw_cache_entries).ok_or(invalid(
            "validation.max_scw_cache_entries",
            "must be greater than zero",
        ))
    }
}

/// Advisory group shapes published to clients. The backend enforces none of
/// them; a client applies them before it builds a commit.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct MlsConfig {
    #[schemars(schema_with = "schema::positive_integer::<{ MAX_MLS_LIMIT as u64 }>")]
    pub max_group_members: u32,
    /// At most the number of actions one identity update may carry, so one
    /// update can revoke every installation the ceiling allows.
    #[schemars(schema_with = "schema::positive_integer::<{ MAX_INSTALLATIONS_LIMIT as u64 }>")]
    pub max_installations_per_inbox: u32,
    /// Publish and read the remote commit log. False switches both off for
    /// every client of this deployment.
    pub commit_log_enabled: bool,
}

impl Default for MlsConfig {
    fn default() -> Self {
        Self {
            max_group_members: xmtp_configuration::MAX_GROUP_SIZE as u32,
            max_installations_per_inbox: xmtp_configuration::MAX_INSTALLATIONS_PER_INBOX as u32,
            commit_log_enabled: xmtp_configuration::ENABLE_COMMIT_LOG,
        }
    }
}

impl MlsConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        for (field, value, maximum, too_large) in [
            (
                "mls.max_group_members",
                self.max_group_members,
                MAX_MLS_LIMIT,
                "must not exceed 65535",
            ),
            (
                "mls.max_installations_per_inbox",
                self.max_installations_per_inbox,
                MAX_INSTALLATIONS_LIMIT,
                "must not exceed the actions of one identity update",
            ),
        ] {
            positive(value, field)?;
            if value > maximum {
                return Err(invalid(field, too_large));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct ServerConfig {
    /// Stable operator-chosen name for this deployment, by convention reverse
    /// DNS such as `org.xmtp.dev`. Clients bind to it on first connect, so it
    /// must never change once clients have connected. Required.
    #[schemars(schema_with = "schema::server_identifier")]
    pub identifier: Option<String>,
    /// Oldest libxmtp version this deployment accepts. Absent publishes no
    /// minimum. Prerelease tags are ignored when clients compare.
    #[schemars(schema_with = "schema::optional_semver")]
    pub min_libxmtp_version: Option<String>,
    #[schemars(schema_with = "schema::socket_address")]
    pub listen: String,
    /// Global backend log level. Request summaries and mutations use INFO.
    #[schemars(schema_with = "schema::log_level")]
    pub log_level: LogLevel,
    #[schemars(schema_with = "schema::log_format")]
    pub log_format: LogFormat,
    /// Emit one summary when each gRPC response finishes or is cancelled.
    pub request_logger: bool,
    #[schemars(schema_with = "schema::positive_integer::<{ MAX_U32_KEY }>")]
    pub max_drain_duration_ms: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            identifier: None,
            min_libxmtp_version: None,
            listen: DEFAULT_LISTEN.to_owned(),
            log_level: LogLevel::Info,
            log_format: LogFormat::Text,
            request_logger: true,
            max_drain_duration_ms: DEFAULT_DRAIN_DURATION_MS,
        }
    }
}

/// Configuration and CLI representation of the shared logging levels.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Off,
    Error,
    Warn,
    #[default]
    Info,
    Debug,
    Trace,
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
    xmtp_configuration::BACKEND_DEFAULT_STATEMENT_TIMEOUT_MS
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
    #[schemars(schema_with = "schema::positive_integer::<{ MAX_U32_KEY }>")]
    pub poll_interval_ms: u64,
    #[schemars(schema_with = "schema::positive_integer::<{ usize::MAX as u64 }>")]
    pub max_gap_ranges: usize,
    #[schemars(schema_with = "schema::positive_integer::<{ u32::MAX as u64 }>")]
    pub keepalive_interval_ms: u64,
    #[schemars(schema_with = "schema::positive_integer::<{ MAX_U32_KEY }>")]
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
    /// Maximum stored topics for one push recipient.
    #[schemars(schema_with = "schema::positive_integer::<{ i32::MAX as u64 }>")]
    pub max_push_topics: i32,
    #[schemars(schema_with = "schema::positive_integer::<{ MAX_U32_KEY }>")]
    pub max_query_topics: usize,
    #[schemars(schema_with = "schema::positive_integer::<{ MAX_U32_KEY }>")]
    pub default_query_limit: usize,
    #[schemars(schema_with = "schema::positive_integer::<{ MAX_U32_KEY }>")]
    pub max_query_limit: usize,
    #[schemars(schema_with = "schema::positive_integer::<{ MAX_U32_KEY }>")]
    pub max_newest_metadata_topics: usize,
    #[schemars(schema_with = "schema::positive_integer::<{ MAX_U32_KEY }>")]
    pub max_newest_full_topics: usize,
    #[schemars(schema_with = "schema::positive_integer::<{ MAX_U32_KEY }>")]
    pub max_publish_topics: usize,
    #[schemars(schema_with = "schema::positive_integer::<{ usize::MAX as u64 }>")]
    pub max_envelope_bytes: usize,
    #[schemars(schema_with = "schema::positive_integer::<{ usize::MAX as u64 }>")]
    pub max_request_bytes: usize,
    #[schemars(schema_with = "schema::positive_integer::<{ usize::MAX as u64 }>")]
    pub max_response_bytes: usize,
    #[schemars(schema_with = "schema::positive_integer::<{ MAX_U32_KEY }>")]
    pub max_update_adds: usize,
    #[schemars(schema_with = "schema::positive_integer::<{ MAX_U32_KEY }>")]
    pub max_update_removes: usize,
    #[schemars(schema_with = "schema::positive_integer::<{ MAX_U32_KEY }>")]
    pub max_stream_topics: usize,
    #[schemars(schema_with = "schema::positive_integer::<{ MAX_U32_KEY }>")]
    pub max_static_topics: usize,
    #[schemars(schema_with = "schema::positive_integer::<{ MAX_U32_KEY }>")]
    pub max_lookup_identifiers: usize,
    /// At most two for each action one identity update may carry, the most
    /// ERC-6492 signatures one update holds.
    #[schemars(
        schema_with = "schema::positive_integer::<{ MAX_SCW_SIGNATURES_PER_REQUEST as u64 }>"
    )]
    pub max_scw_signatures: usize,
    #[schemars(schema_with = "schema::positive_integer::<{ MAX_U32_KEY }>")]
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
    // implements: CONF-096
    fn validate(&self) -> Result<(), ConfigError> {
        positive(self.max_push_topics, "limits.max_push_topics")?;
        positive(self.max_query_topics, "limits.max_query_topics")?;
        positive(self.default_query_limit, "limits.default_query_limit")?;
        positive(self.max_query_limit, "limits.max_query_limit")?;
        positive(
            self.max_newest_metadata_topics,
            "limits.max_newest_metadata_topics",
        )?;
        positive(self.max_newest_full_topics, "limits.max_newest_full_topics")?;
        positive(self.max_publish_topics, "limits.max_publish_topics")?;
        positive(self.max_envelope_bytes, "limits.max_envelope_bytes")?;
        positive(self.max_request_bytes, "limits.max_request_bytes")?;
        positive(self.max_response_bytes, "limits.max_response_bytes")?;
        // The transport ceiling is fixed. A larger configured budget would
        // promise clients a request the transport refuses to carry.
        for (field, value) in [
            ("limits.max_request_bytes", self.max_request_bytes),
            ("limits.max_response_bytes", self.max_response_bytes),
        ] {
            if value > MAX_TRANSPORT_BYTES {
                return Err(invalid(field, "must not exceed the 25 MiB transport limit"));
            }
        }
        self.validate_envelope_fit()?;
        positive(self.max_update_adds, "limits.max_update_adds")?;
        positive(self.max_update_removes, "limits.max_update_removes")?;
        positive(self.max_stream_topics, "limits.max_stream_topics")?;
        positive(self.max_static_topics, "limits.max_static_topics")?;
        positive(self.max_lookup_identifiers, "limits.max_lookup_identifiers")?;
        positive(self.max_scw_signatures, "limits.max_scw_signatures")?;
        if self.max_scw_signatures > MAX_SCW_SIGNATURES_PER_REQUEST {
            return Err(invalid(
                "limits.max_scw_signatures",
                "must not exceed two signatures for each action of one identity update",
            ));
        }
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
        positive(self.max_ping_burst, "limits.max_ping_burst")?;
        // Each count is published as a 32-bit integer, so it must fit one.
        for (field, value) in [
            ("limits.max_query_topics", self.max_query_topics),
            ("limits.default_query_limit", self.default_query_limit),
            ("limits.max_query_limit", self.max_query_limit),
            (
                "limits.max_newest_metadata_topics",
                self.max_newest_metadata_topics,
            ),
            ("limits.max_newest_full_topics", self.max_newest_full_topics),
            ("limits.max_publish_topics", self.max_publish_topics),
            ("limits.max_update_adds", self.max_update_adds),
            ("limits.max_update_removes", self.max_update_removes),
            ("limits.max_stream_topics", self.max_stream_topics),
            ("limits.max_static_topics", self.max_static_topics),
            ("limits.max_lookup_identifiers", self.max_lookup_identifiers),
            ("limits.max_identity_entries", self.max_identity_entries),
        ] {
            at_most_u32(value as u64, field)?;
        }
        if self.max_request_bytes < NOTIFICATION_BATCH_MAX_BYTES {
            return Err(invalid(
                "limits.max_request_bytes",
                "must fit a full notification subscription batch",
            ));
        }
        let required = subscription_control_bytes(self.max_update_adds, 11)
            .max(subscription_control_bytes(self.max_static_topics, 6));
        if self.max_response_bytes < required {
            return Err(invalid(
                "limits.max_response_bytes",
                "must fit complete subscription control frames",
            ));
        }
        // The relations come after each key's own checks, so a value outside
        // its own range is refused under its own key.
        if self.max_query_limit < self.default_query_limit {
            return Err(invalid(
                "limits.max_query_limit",
                "must be at least limits.default_query_limit",
            ));
        }
        if self.max_update_adds > self.max_stream_topics {
            return Err(invalid(
                "limits.max_update_adds",
                "must not exceed limits.max_stream_topics",
            ));
        }
        Ok(())
    }

    /// Reserve worst-case metadata and framing in each request and delivery budget.
    /// Saturating arithmetic ensures oversized configured values cannot wrap to fit.
    // implements: OPS-021
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
        {
            return Err(invalid(
                "limits.max_envelope_bytes",
                "envelope plus framing must fit all transport budgets",
            ));
        }
        Ok(())
    }
}

/// Complete control protobuf, excluding the gRPC header: each repeated target
/// includes a 33-byte topic and INT64_MAX sequence; headers cover a UINT64_MAX
/// Applied id or UINT32_MAX Started keepalive interval.
fn subscription_control_bytes(count: usize, header: usize) -> usize {
    const CATCHUP_TARGET_MAX_BYTES: usize = 49;
    let body = count
        .saturating_mul(CATCHUP_TARGET_MAX_BYTES)
        .saturating_add(header);
    body.saturating_add(1 + prost::encoding::encoded_len_varint(body as u64))
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            max_push_topics: push::DEFAULT_MAX_PUSH_TOPICS,
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
mod identifier_tests;
#[cfg(test)]
mod tests;

/// Stdout encoding. Each JSON event occupies one line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    #[default]
    Text,
    Json,
}
