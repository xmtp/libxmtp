//! Remote attachment storage settings. `xmtp_attachments_server` builds the
//! storage target from them.

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{self, MapAccess, Visitor},
};
use std::{fmt, marker::PhantomData};
use url::{Host, Url};
use xmtp_configuration::{
    AttachmentConfigurationError, BACKEND_DEFAULT_MAX_UPLOAD_BYTES,
    MAX_ATTACHMENT_RETENTION_SECONDS, MAX_ATTACHMENT_UPLOAD_BYTES, check_base_url,
    check_max_upload_bytes, check_retention_seconds,
};

use super::{ConfigError, Step, invalid, key_path, schema::with_environment};

pub const DEFAULT_PRESIGN_TTL_SECONDS: u32 = 900;
pub const MIN_PRESIGN_TTL_SECONDS: u32 = 300;
pub const MAX_PRESIGN_TTL_SECONDS: u32 = 3600;

/// Match all text forms of the IPv6 loopback address, with or without `::`.
fn ipv6_loopback_pattern() -> String {
    let zero = "0{1,4}";
    let mut forms = vec![format!("(?:{zero}:){{7}}0{{0,3}}1")];
    for left in 0..=6 {
        for right in 0..=6 - left {
            let before = vec![zero; left].join(":");
            let after = vec![zero; right].join(":");
            let separator = if right == 0 { "" } else { ":" };
            forms.push(format!("{before}::{after}{separator}0{{0,3}}1"));
        }
    }
    forms.join("|")
}

/// Match an IPv6 address whose final two words use an IPv4 address.
fn ipv6_ipv4_tail_pattern(word: &str, tail: &str) -> String {
    let mut forms = vec![format!(r"(?:{word}:){{6}}{tail}")];
    for left in 0..=5 {
        for right in 0..=5 - left {
            let before = vec![word; left].join(":");
            let after = vec![word; right].join(":");
            let separator = if right == 0 { "" } else { ":" };
            forms.push(format!("{before}::{after}{separator}{tail}"));
        }
    }
    forms.join("|")
}

/// Constrain the public URL shape. The runtime performs the final RFC 3986 check.
fn url_shape(allow_trailing_slash: bool) -> Schema {
    let octet = r"(?:0|[1-9][0-9]?|1[0-9]{2}|2[0-4][0-9]|25[0-5])";
    let ipv4_tail = format!(r"{octet}\.{octet}\.{octet}\.{octet}");
    let ipv6_tail = ipv6_ipv4_tail_pattern(r"[0-9A-Fa-f]{1,4}", &ipv4_tail);
    let loopback_tail = ipv6_ipv4_tail_pattern("0{1,4}", r"0\.0\.0\.1");
    let localhost = r"[lL][oO][cC][aA][lL][hH][oO][sS][tT]";
    let loopback = format!(
        r"(?:{localhost}|127\.(?:{octet}\.){{2}}{octet}|\[(?:{}|{loopback_tail})\])",
        ipv6_loopback_pattern(),
    );
    let host = format!(r"(?:[A-Za-z0-9._~!$&'()*+,;=-]+|\[(?:[0-9A-Fa-f:]+|{ipv6_tail})\])");
    let port = r"(?::(?:0*(?:[1-9][0-9]{0,3}|[1-5][0-9]{4}|6[0-4][0-9]{3}|65[0-4][0-9]{2}|655[0-2][0-9]|6553[0-5]))?)?";
    let path = r"(?:/[A-Za-z0-9._~!$&'()*+,;=:@/%-]*)*";
    let no_dot_segments = if allow_trailing_slash {
        ""
    } else {
        r"(?!.*(?:/)(?:\.|%2[eE]){1,2}(?:/|$))"
    };
    let no_trailing_slash = if allow_trailing_slash {
        ""
    } else {
        r"(?!.*[/]$)"
    };
    let origin = if allow_trailing_slash {
        format!(r"(?:[hH][tT][tT][pP][sS]://{host}|[hH][tT][tT][pP]://{loopback})")
    } else {
        format!(r"(?:https://{host}|http://{loopback})")
    };
    let pattern = format!(r"^{no_dot_segments}{no_trailing_slash}{origin}{port}{path}$(?![\s\S])");
    with_environment(json_schema!({"type": "string", "format": "uri", "pattern": pattern}))
}

fn base_url_schema(_: &mut SchemaGenerator) -> Schema {
    url_shape(false)
}

fn endpoint_schema(_: &mut SchemaGenerator) -> Schema {
    url_shape(true)
}

/// Find IPv4 host text that URL parsing normalized.
fn has_noncanonical_ipv4_host(endpoint: &str, parsed: &Url) -> bool {
    let Some(Host::Ipv4(address)) = parsed.host() else {
        return false;
    };
    let Some((_, rest)) = endpoint.split_once("://") else {
        return false;
    };
    let authority = rest.split('/').next().unwrap_or(rest);
    if authority.starts_with('[') {
        return false;
    }
    let host = authority.split(':').next().unwrap_or(authority);
    host != address.to_string()
}

fn region_schema(_: &mut SchemaGenerator) -> Schema {
    with_environment(json_schema!({"type": "string", "pattern": r"^[^\s]+$(?![\s\S])"}))
}

fn bucket_schema(_: &mut SchemaGenerator) -> Schema {
    with_environment(
        json_schema!({"type": "string", "pattern": r"^(?!\.{1,2}$(?![\s\S]))[^/]+$(?![\s\S])"}),
    )
}

fn key_prefix_schema(_: &mut SchemaGenerator) -> Schema {
    let literal = json_schema!({
        "type": "string",
        "pattern": r"^(?!/)(?!.*//)(?!.*(?:^|/)\.{1,2}(?:/|$))[A-Za-z0-9_./-]*(?![\s\S])"
    });
    let mut schema = with_environment(literal);
    schema.insert("default".to_owned(), "".into());
    schema
}

/// Operator settings for remote attachment storage.
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AttachmentsConfig {
    /// Public download URL. An `env:NAME` value is resolved before validation.
    #[schemars(schema_with = "base_url_schema")]
    pub base_url: String,
    #[schemars(range(min = 1, max = MAX_ATTACHMENT_UPLOAD_BYTES))]
    pub max_upload_bytes: Option<u64>,
    #[schemars(range(min = 0, max = MAX_ATTACHMENT_RETENTION_SECONDS))]
    pub retention_seconds: Option<u64>,
    pub target: TargetConfig,
}

impl std::fmt::Debug for AttachmentsConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AttachmentsConfig")
            .field("base_url", &"[redacted]")
            .field("max_upload_bytes", &self.max_upload_bytes)
            .field("retention_seconds", &self.retention_seconds)
            .field("target", &self.target)
            .finish()
    }
}

impl AttachmentsConfig {
    pub fn upload_ceiling(&self) -> u64 {
        self.max_upload_bytes
            .unwrap_or(BACKEND_DEFAULT_MAX_UPLOAD_BYTES)
    }

    /// Reject settings that cannot be published or enforced.
    // implements: ATCH-002, ATCH-003, ATCH-004
    pub(crate) fn validate(&self) -> Result<(), ConfigError> {
        check_base_url(&self.base_url).map_err(shared)?;
        check_max_upload_bytes(self.upload_ceiling()).map_err(shared)?;
        check_retention_seconds(self.retention_seconds.unwrap_or_default()).map_err(shared)?;
        match &self.target {
            TargetConfig::S3(s3) => s3.validate(),
        }
    }
}

/// A storage target. The variant name is an operator config key.
#[derive(Clone, Serialize, JsonSchema)]
pub enum TargetConfig {
    S3(S3Config),
}

/// Decode through a table with one optional key per target, so a key that
/// names no target is refused by name. Serde decodes an externally tagged
/// enum from a table that holds exactly one key, and refuses a second key as
/// a type error at the table.
impl<'de> Deserialize<'de> for TargetConfig {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct TargetTable {
            #[serde(rename = "S3")]
            s3: Option<S3Config>,
        }
        match table_only::<TargetTable, D>(deserializer)?.s3 {
            Some(s3) => Ok(Self::S3(s3)),
            None => Err(de::Error::missing_field("S3")),
        }
    }
}

impl std::fmt::Debug for TargetConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::S3(_) => f.write_str("S3([redacted])"),
        }
    }
}

/// S3-compatible path-style target settings.
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct S3Config {
    /// Storage URL. An `env:NAME` value is resolved before validation.
    #[schemars(schema_with = "endpoint_schema")]
    pub endpoint: String,
    /// Region name. An `env:NAME` value is resolved before validation.
    #[schemars(schema_with = "region_schema")]
    pub region: String,
    /// Bucket name. An `env:NAME` value is resolved before validation.
    #[schemars(schema_with = "bucket_schema")]
    pub bucket: String,
    /// The prefix can be empty. Other prefixes use ASCII letters, digits,
    /// `_`, `-`, `.`, and slash separators. A trailing slash is allowed.
    /// Leading slashes, empty interior segments, and `.` or `..` segments are invalid.
    /// An `env:NAME` value is resolved before validation.
    #[serde(default)]
    #[schemars(schema_with = "key_prefix_schema")]
    pub key_prefix: String,
    pub credentials: CredentialsConfig,
    #[schemars(range(min = MIN_PRESIGN_TTL_SECONDS, max = MAX_PRESIGN_TTL_SECONDS))]
    pub presign_ttl_seconds: Option<u32>,
}

impl std::fmt::Debug for S3Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("S3Config")
            .field("endpoint", &"[redacted]")
            .field("region", &self.region)
            .field("bucket", &"[redacted]")
            .field("key_prefix", &"[redacted]")
            .field("credentials", &self.credentials)
            .finish()
    }
}

impl S3Config {
    pub fn presign_ttl_seconds(&self) -> u32 {
        self.presign_ttl_seconds
            .unwrap_or(DEFAULT_PRESIGN_TTL_SECONDS)
    }

    pub(crate) fn validate(&self) -> Result<(), ConfigError> {
        if self.endpoint.bytes().any(|byte| {
            !byte.is_ascii()
                || byte.is_ascii_control()
                || byte.is_ascii_whitespace()
                || byte == b'\\'
        }) || self.endpoint.split_once("://").is_some_and(|(_, rest)| {
            rest.split('/')
                .next()
                .is_some_and(|host| host.contains('%'))
        }) {
            return Err(invalid(
                "attachments.target.S3.endpoint",
                "is not a URL as written",
            ));
        }
        let url = Url::parse(&self.endpoint)
            .map_err(|_| invalid("attachments.target.S3.endpoint", "must be an absolute URL"))?;
        if has_noncanonical_ipv4_host(&self.endpoint, &url) {
            return Err(invalid(
                "attachments.target.S3.endpoint",
                "is not a URL as written",
            ));
        }
        if !matches!(url.scheme(), "http" | "https")
            || url.host().is_none()
            || url.query().is_some()
            || url.fragment().is_some()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(invalid(
                "attachments.target.S3.endpoint",
                "invalid HTTP endpoint",
            ));
        }
        // The shared URL check owns the loopback rule for HTTP. Check only the
        // origin here so S3 endpoints can retain a path prefix.
        check_base_url(&url.origin().ascii_serialization()).map_err(|_| {
            invalid(
                "attachments.target.S3.endpoint",
                "must use HTTPS or HTTP on a loopback host",
            )
        })?;
        // implements: ATCH-081
        if self.region.is_empty() || self.region.chars().any(char::is_whitespace) {
            return Err(invalid(
                "attachments.target.S3.region",
                "must name a region",
            ));
        }
        // implements: ATCH-081
        if self.bucket.is_empty()
            || self.bucket.contains('/')
            || self.bucket == "."
            || self.bucket == ".."
        {
            return Err(invalid(
                "attachments.target.S3.bucket",
                "must name one bucket",
            ));
        }
        if self
            .key_prefix
            .chars()
            .any(|c| !(c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '-' | '.')))
            || self.key_prefix.starts_with('/')
            || self.key_prefix.contains("//")
            || self
                .key_prefix
                .split('/')
                .any(|part| part == "." || part == "..")
        {
            return Err(invalid(
                "attachments.target.S3.key_prefix",
                "contains an unsafe path character",
            ));
        }
        // implements: ATCH-083
        if !(MIN_PRESIGN_TTL_SECONDS..=MAX_PRESIGN_TTL_SECONDS)
            .contains(&self.presign_ttl_seconds())
        {
            return Err(invalid(
                "attachments.target.S3.presign_ttl_seconds",
                "outside 300..=3600",
            ));
        }
        self.credentials.validate()
    }
}

/// Credential source for signing. Debug never prints its fields.
#[derive(Clone, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CredentialsConfig {
    Static {
        #[schemars(length(min = 1))]
        access_key_id: String,
        #[schemars(length(min = 1))]
        secret_access_key: String,
        session_token: Option<String>,
    },
    DefaultChain,
    Environment,
    Profile {
        #[schemars(length(min = 1))]
        name: String,
    },
    Sso {
        #[schemars(length(min = 1))]
        account_id: String,
        #[schemars(length(min = 1))]
        region: String,
        #[schemars(length(min = 1))]
        role_name: String,
        #[schemars(length(min = 1))]
        start_url: String,
        session_name: Option<String>,
    },
    Process {
        #[schemars(length(min = 1))]
        command: String,
    },
    WebIdentity,
    Container,
    Instance,
    AssumeRole {
        #[schemars(length(min = 1))]
        role_arn: String,
        external_id: Option<String>,
        session_name: Option<String>,
    },
}

impl std::fmt::Debug for CredentialsConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = match self {
            Self::Static { .. } => "static",
            Self::DefaultChain => "default_chain",
            Self::Environment => "environment",
            Self::Profile { .. } => "profile",
            Self::Sso { .. } => "sso",
            Self::Process { .. } => "process",
            Self::WebIdentity => "web_identity",
            Self::Container => "container",
            Self::Instance => "instance",
            Self::AssumeRole { .. } => "assume_role",
        };
        f.debug_struct("CredentialsConfig")
            .field("kind", &kind)
            .finish()
    }
}

/// Decode `T` from a table only. toml decodes a struct from an array by
/// position, and this decoder refuses an array as a value of the wrong type.
/// The table's keys still pass through the caller's map, so a refusal inside
/// it names its key.
fn table_only<'de, T: Deserialize<'de>, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<T, D::Error> {
    struct Table<T>(PhantomData<T>);
    impl<'de, T: Deserialize<'de>> Visitor<'de> for Table<T> {
        type Value = T;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a table")
        }

        fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<T, A::Error> {
            T::deserialize(de::value::MapAccessDeserializer::new(map))
        }
    }
    deserializer.deserialize_map(Table(PhantomData))
}

/// Decode through `CredentialsTable`, so a key that the kind does not take is
/// refused by name. Serde drops the other keys of a unit variant of an
/// internally tagged enum, even with `deny_unknown_fields`, and it buffers the
/// table, so a refusal inside it loses the key.
// implements: CONF-083
impl<'de> Deserialize<'de> for CredentialsConfig {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut table: CredentialsTable = table_only(deserializer)?;
        let required = |value: Option<String>, key| -> Result<String, D::Error> {
            value.ok_or_else(|| de::Error::missing_field(key))
        };
        let credentials = match table.kind {
            CredentialKind::Static => Self::Static {
                access_key_id: required(table.access_key_id.take(), "access_key_id")?,
                secret_access_key: required(table.secret_access_key.take(), "secret_access_key")?,
                session_token: table.session_token.take(),
            },
            CredentialKind::DefaultChain => Self::DefaultChain,
            CredentialKind::Environment => Self::Environment,
            CredentialKind::Profile => Self::Profile {
                name: required(table.name.take(), "name")?,
            },
            CredentialKind::Sso => Self::Sso {
                account_id: required(table.account_id.take(), "account_id")?,
                region: required(table.region.take(), "region")?,
                role_name: required(table.role_name.take(), "role_name")?,
                start_url: required(table.start_url.take(), "start_url")?,
                session_name: table.session_name.take(),
            },
            CredentialKind::Process => Self::Process {
                command: required(table.command.take(), "command")?,
            },
            CredentialKind::WebIdentity => Self::WebIdentity,
            CredentialKind::Container => Self::Container,
            CredentialKind::Instance => Self::Instance,
            CredentialKind::AssumeRole => Self::AssumeRole {
                role_arn: required(table.role_arn.take(), "role_arn")?,
                external_id: table.external_id.take(),
                session_name: table.session_name.take(),
            },
        };
        match table.left_over() {
            Some(key) => Err(de::Error::custom(format_args!("unknown field `{key}`"))),
            None => Ok(credentials),
        }
    }
}

/// The credentials table as decoded: its kind, and every key that any kind
/// takes. Serde refuses a key that no kind takes.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CredentialsTable {
    #[serde(deserialize_with = "kind_name")]
    kind: CredentialKind,
    access_key_id: Option<String>,
    secret_access_key: Option<String>,
    session_token: Option<String>,
    name: Option<String>,
    account_id: Option<String>,
    region: Option<String>,
    role_name: Option<String>,
    start_url: Option<String>,
    session_name: Option<String>,
    command: Option<String>,
    role_arn: Option<String>,
    external_id: Option<String>,
}

impl CredentialsTable {
    /// The first key that the kind did not take.
    fn left_over(&self) -> Option<&'static str> {
        [
            ("access_key_id", &self.access_key_id),
            ("secret_access_key", &self.secret_access_key),
            ("session_token", &self.session_token),
            ("name", &self.name),
            ("account_id", &self.account_id),
            ("region", &self.region),
            ("role_name", &self.role_name),
            ("start_url", &self.start_url),
            ("session_name", &self.session_name),
            ("command", &self.command),
            ("role_arn", &self.role_arn),
            ("external_id", &self.external_id),
        ]
        .into_iter()
        .find_map(|(key, value)| value.is_some().then_some(key))
    }
}

/// Decode a credential kind from a string only. toml offers a one-key table
/// as an enum, which the derived decoder alone would take as its variant.
fn kind_name<'de, D: Deserializer<'de>>(deserializer: D) -> Result<CredentialKind, D::Error> {
    struct Name;
    impl<'de> Visitor<'de> for Name {
        type Value = CredentialKind;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a credential kind")
        }

        fn visit_str<E: de::Error>(self, name: &str) -> Result<CredentialKind, E> {
            CredentialKind::deserialize(de::value::StrDeserializer::<E>::new(name))
        }
    }
    deserializer.deserialize_str(Name)
}

/// The `kind` of a credentials table, one per `CredentialsConfig` variant.
/// Decode it with `kind_name`.
#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum CredentialKind {
    Static,
    DefaultChain,
    Environment,
    Profile,
    Sso,
    Process,
    WebIdentity,
    Container,
    Instance,
    AssumeRole,
}

impl CredentialsConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        let required = |field: &str, value: &str| {
            if value.is_empty() {
                Err(invalid(field, "must not be empty"))
            } else {
                Ok(())
            }
        };
        match self {
            Self::Static {
                access_key_id,
                secret_access_key,
                ..
            } => {
                required(
                    "attachments.target.S3.credentials.access_key_id",
                    access_key_id,
                )?;
                required(
                    "attachments.target.S3.credentials.secret_access_key",
                    secret_access_key,
                )
            }
            Self::Profile { name } => required("attachments.target.S3.credentials.name", name),
            Self::Sso {
                account_id,
                region,
                role_name,
                start_url,
                ..
            } => {
                required("attachments.target.S3.credentials.account_id", account_id)?;
                required("attachments.target.S3.credentials.region", region)?;
                required("attachments.target.S3.credentials.role_name", role_name)?;
                required("attachments.target.S3.credentials.start_url", start_url)
            }
            Self::Process { command } => {
                required("attachments.target.S3.credentials.command", command)
            }
            Self::AssumeRole { role_arn, .. } => {
                required("attachments.target.S3.credentials.role_arn", role_arn)
            }
            _ => Ok(()),
        }
    }
}

/// Name the key of a shared attachment rule's refusal.
fn shared(error: AttachmentConfigurationError) -> ConfigError {
    invalid(
        key_path("attachments", Step::Key(error.field())),
        error.reason(),
    )
}

#[cfg(test)]
mod tests;
