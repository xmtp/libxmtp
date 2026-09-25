use schemars::JsonSchema;
use serde::Deserialize;
use url::{Host, Url};
use xmtp_configuration::BACKEND_DEFAULT_MAX_UPLOAD_BYTES;

pub const MAX_UPLOAD_BYTES: u64 = u32::MAX as u64;
pub const DEFAULT_PRESIGN_TTL_SECONDS: u32 = 900;
pub const MIN_PRESIGN_TTL_SECONDS: u32 = 300;
pub const MAX_PRESIGN_TTL_SECONDS: u32 = 3600;

/// Operator settings for remote attachment storage.
#[derive(Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AttachmentsConfig {
    pub base_url: String,
    pub max_upload_bytes: Option<u64>,
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
    pub fn validate(&self) -> Result<(), ConfigInvalid> {
        let url = Url::parse(&self.base_url)
            .map_err(|_| ConfigInvalid::new("attachments.base_url", "must be an absolute URL"))?;
        let permitted_scheme = match url.scheme() {
            "https" => true,
            "http" => match url.host() {
                Some(Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
                Some(Host::Ipv4(ip)) => ip.is_loopback(),
                Some(Host::Ipv6(ip)) => ip.is_loopback(),
                None => false,
            },
            _ => false,
        };
        if !permitted_scheme
            || url.host().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(ConfigInvalid::new(
                "attachments.base_url",
                "requires HTTPS or HTTP on loopback",
            ));
        }
        if url.query().is_some() || url.fragment().is_some() || self.base_url.ends_with('/') {
            return Err(ConfigInvalid::new(
                "attachments.base_url",
                "must have no query, fragment, or trailing slash",
            ));
        }
        if !(1..=MAX_UPLOAD_BYTES).contains(&self.upload_ceiling()) {
            return Err(ConfigInvalid::new(
                "attachments.max_upload_bytes",
                "outside 1..=4294967295",
            ));
        }
        match &self.target {
            TargetConfig::S3(s3) => s3.validate(),
        }
    }
}

/// A storage target. The variant name is an operator config key.
#[derive(Clone, Deserialize, JsonSchema)]
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
#[derive(Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct S3Config {
    pub endpoint: String,
    pub region: String,
    pub bucket: String,
    #[serde(default)]
    pub key_prefix: String,
    pub credentials: CredentialsConfig,
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
#[derive(Clone, Deserialize, JsonSchema)]
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
}

#[cfg(test)]
mod tests;
