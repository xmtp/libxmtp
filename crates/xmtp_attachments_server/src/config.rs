use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};
use url::Url;
use xmtp_configuration::{
    BACKEND_DEFAULT_MAX_UPLOAD_BYTES, MAX_ATTACHMENT_RETENTION_SECONDS,
    MAX_ATTACHMENT_UPLOAD_BYTES, check_base_url, check_max_upload_bytes, check_retention_seconds,
};

pub const DEFAULT_PRESIGN_TTL_SECONDS: u32 = 900;
pub const MIN_PRESIGN_TTL_SECONDS: u32 = 300;
pub const MAX_PRESIGN_TTL_SECONDS: u32 = 3600;

/// Accept a literal value or an environment reference resolved before validation.
fn with_environment(literal: Schema) -> Schema {
    json_schema!({"anyOf": [literal, {"type": "string", "pattern": "^env:[^=\\x00]+$(?![\\s\\S])"}]})
}

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

/// Constrain the public URL shape. The runtime performs the final RFC 3986 check.
fn url_shape(allow_trailing_slash: bool) -> Schema {
    let octet = r"(?:0|[1-9][0-9]?|1[0-9]{2}|2[0-4][0-9]|25[0-5])";
    let localhost = r"[lL][oO][cC][aA][lL][hH][oO][sS][tT]";
    let loopback = format!(
        r"(?:{localhost}|127\.(?:{octet}\.){{2}}{octet}|\[(?:{})\])",
        ipv6_loopback_pattern()
    );
    let host = r"(?:[A-Za-z0-9._~!$&'()*+,;=-]+|\[[0-9A-Fa-f:.]+\])";
    let port = r"(?::(?:0*(?:[0-9]{1,4}|[1-5][0-9]{4}|6[0-4][0-9]{3}|65[0-4][0-9]{2}|655[0-2][0-9]|6553[0-5]))?)?";
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
    let pattern = format!(
        r"^{no_dot_segments}{no_trailing_slash}(?:https://{host}|http://{loopback}){port}{path}$(?![\s\S])"
    );
    with_environment(json_schema!({"type": "string", "format": "uri", "pattern": pattern}))
}

fn base_url_schema(_: &mut SchemaGenerator) -> Schema {
    url_shape(false)
}

fn endpoint_schema(_: &mut SchemaGenerator) -> Schema {
    url_shape(true)
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
    pub fn validate(&self) -> Result<(), ConfigInvalid> {
        check_base_url(&self.base_url).map_err(ConfigInvalid::from_shared)?;
        check_max_upload_bytes(self.upload_ceiling()).map_err(ConfigInvalid::from_shared)?;
        check_retention_seconds(self.retention_seconds.unwrap_or_default())
            .map_err(ConfigInvalid::from_shared)?;
        match &self.target {
            TargetConfig::S3(s3) => s3.validate(),
        }
    }
}

/// A storage target. The variant name is an operator config key.
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
pub enum TargetConfig {
    S3(S3Config),
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
    pub region: String,
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

    pub(crate) fn validate(&self) -> Result<(), ConfigInvalid> {
        let url = Url::parse(&self.endpoint).map_err(|_| {
            ConfigInvalid::new("attachments.target.S3.endpoint", "must be an absolute URL")
        })?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host().is_none()
            || url.query().is_some()
            || url.fragment().is_some()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(ConfigInvalid::new(
                "attachments.target.S3.endpoint",
                "invalid HTTP endpoint",
            ));
        }
        // The shared URL check owns the loopback rule for HTTP. Check only the
        // origin here so S3 endpoints can retain a path prefix.
        check_base_url(&url.origin().ascii_serialization()).map_err(|_| {
            ConfigInvalid::new(
                "attachments.target.S3.endpoint",
                "must use HTTPS or HTTP on a loopback host",
            )
        })?;
        if self.region.is_empty() || self.region.chars().any(char::is_whitespace) {
            return Err(ConfigInvalid::new(
                "attachments.target.S3.region",
                "must name a region",
            ));
        }
        if self.bucket.is_empty()
            || self.bucket.contains('/')
            || self.bucket == "."
            || self.bucket == ".."
        {
            return Err(ConfigInvalid::new(
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
            return Err(ConfigInvalid::new(
                "attachments.target.S3.key_prefix",
                "contains an unsafe path character",
            ));
        }
        if !(MIN_PRESIGN_TTL_SECONDS..=MAX_PRESIGN_TTL_SECONDS)
            .contains(&self.presign_ttl_seconds())
        {
            return Err(ConfigInvalid::new(
                "attachments.target.S3.presign_ttl_seconds",
                "outside 300..=3600",
            ));
        }
        self.credentials.validate()
    }
}

/// Credential source for signing. Debug never prints its fields.
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CredentialsConfig {
    Static {
        access_key_id: String,
        secret_access_key: String,
        session_token: Option<String>,
    },
    DefaultChain,
    Environment,
    Profile {
        name: String,
    },
    Sso {
        account_id: String,
        region: String,
        role_name: String,
        start_url: String,
        session_name: Option<String>,
    },
    Process {
        command: String,
    },
    WebIdentity,
    Container,
    Instance,
    AssumeRole {
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

impl CredentialsConfig {
    fn validate(&self) -> Result<(), ConfigInvalid> {
        let valid = match self {
            Self::Static {
                access_key_id,
                secret_access_key,
                ..
            } => !access_key_id.is_empty() && !secret_access_key.is_empty(),
            Self::Profile { name } => !name.is_empty(),
            Self::Sso {
                account_id,
                region,
                role_name,
                start_url,
                ..
            } => {
                !account_id.is_empty()
                    && !region.is_empty()
                    && !role_name.is_empty()
                    && !start_url.is_empty()
            }
            Self::Process { command } => !command.is_empty(),
            Self::AssumeRole { role_arn, .. } => !role_arn.is_empty(),
            _ => true,
        };
        if valid {
            Ok(())
        } else {
            Err(ConfigInvalid::new(
                "attachments.target.S3.credentials",
                "required field is empty",
            ))
        }
    }
}

/// A non-secret startup error that names the failed config key.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("{field}: {reason}")]
pub struct ConfigInvalid {
    pub field: &'static str,
    pub reason: &'static str,
}

impl ConfigInvalid {
    fn new(field: &'static str, reason: &'static str) -> Self {
        Self { field, reason }
    }

    fn from_shared(error: xmtp_configuration::AttachmentConfigurationError) -> Self {
        let field = match error.field() {
            "base_url" => "attachments.base_url",
            "max_upload_bytes" => "attachments.max_upload_bytes",
            "retention_seconds" => "attachments.retention_seconds",
            _ => unreachable!("shared attachment configuration field"),
        };
        Self::new(field, error.reason())
    }
}

#[cfg(test)]
mod tests;
