//! Push provider configuration. Provider credentials are never formatted.

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};

use super::{ConfigError, MAX_RETENTION_SECONDS, invalid};

const DEFAULT_RECIPIENT_TTL_SECONDS: i64 = 2_592_000;
const MIN_RECIPIENT_TTL_SECONDS: i64 = 86_400;
const DEFAULT_MAX_ATTEMPTS: i64 = 3;
const MAX_ATTEMPTS: i64 = 10;
const MAX_HOST_BYTES: usize = 253;
const MAX_HOST_LABEL_BYTES: usize = 63;
pub(super) const DEFAULT_MAX_PUSH_TOPICS: i32 = 100_000;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct PushConfig {
    /// Time without renewal before a recipient expires, in seconds.
    #[schemars(range(min = MIN_RECIPIENT_TTL_SECONDS, max = MAX_RETENTION_SECONDS))]
    pub recipient_ttl_seconds: i64,
    /// Maximum provider attempts for each delivery.
    #[schemars(range(min = 1, max = MAX_ATTEMPTS))]
    pub max_attempts: i64,
    /// APNs is available only when this block is present.
    pub apns: Option<ApnsConfig>,
    /// FCM is available only when this block is present.
    pub fcm: Option<FcmConfig>,
    /// HTTPS webhooks are available only when this block is present.
    pub http: Option<HttpConfig>,
}

impl Default for PushConfig {
    fn default() -> Self {
        Self {
            recipient_ttl_seconds: DEFAULT_RECIPIENT_TTL_SECONDS,
            max_attempts: DEFAULT_MAX_ATTEMPTS,
            apns: None,
            fcm: None,
            http: None,
        }
    }
}

#[derive(Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ApnsConfig {
    /// PKCS#8 PEM credential; accepts an env:NAME reference.
    #[schemars(required)]
    pub key: Option<String>,
    /// Apple signing-key identifier.
    #[schemars(required)]
    pub key_id: Option<String>,
    /// Apple developer team identifier.
    #[schemars(required)]
    pub team_id: Option<String>,
    /// Application bundle identifier used as the APNs topic.
    #[schemars(required)]
    pub bundle_id: Option<String>,
    /// Either production or sandbox.
    #[schemars(required, schema_with = "environment_schema")]
    pub environment: Option<String>,
}

impl std::fmt::Debug for ApnsConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApnsConfig")
            .field("key", &"<redacted>")
            .field("key_id", &self.key_id)
            .field("team_id", &self.team_id)
            .field("bundle_id", &self.bundle_id)
            .field("environment", &self.environment)
            .finish()
    }
}

#[derive(Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FcmConfig {
    /// Service-account JSON credential; accepts an env:NAME reference.
    #[schemars(required)]
    pub service_account: Option<String>,
}

impl std::fmt::Debug for FcmConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FcmConfig")
            .field("service_account", &"<redacted>")
            .finish()
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct HttpConfig {
    /// Allowed hosts, with an optional leading *. wildcard. Absent allows all hosts.
    pub allowed_domains: Option<Vec<String>>,
    /// Permit private and local network addresses. Defaults to false.
    pub allow_private_addresses: bool,
}

impl PushConfig {
    /// Validate provider fields without including their values in errors.
    /// Schemars describes the public schema; it does not validate loaded values.
    pub(crate) fn validate(&self) -> Result<(), ConfigError> {
        if !(MIN_RECIPIENT_TTL_SECONDS..=MAX_RETENTION_SECONDS as i64)
            .contains(&self.recipient_ttl_seconds)
        {
            return Err(invalid(
                "push.recipient_ttl_seconds",
                "duration is out of range",
            ));
        }
        if !(1..=MAX_ATTEMPTS).contains(&self.max_attempts) {
            return Err(invalid("push.max_attempts", "must be between 1 and 10"));
        }
        if let Some(apns) = &self.apns {
            for (field, value) in [
                ("push.apns.key", &apns.key),
                ("push.apns.key_id", &apns.key_id),
                ("push.apns.team_id", &apns.team_id),
                ("push.apns.bundle_id", &apns.bundle_id),
                ("push.apns.environment", &apns.environment),
            ] {
                required(value, field)?;
            }
            if !matches!(apns.environment.as_deref(), Some("production" | "sandbox")) {
                return Err(invalid(
                    "push.apns.environment",
                    "must be production or sandbox",
                ));
            }
        }
        if let Some(fcm) = &self.fcm {
            required(&fcm.service_account, "push.fcm.service_account")?;
        }
        if self
            .http
            .as_ref()
            .and_then(|http| http.allowed_domains.as_ref())
            .is_some_and(|domains| domains.iter().any(|domain| !valid_domain(domain)))
        {
            return Err(invalid(
                "push.http.allowed_domains",
                "must contain host names with optional leading *.",
            ));
        }
        Ok(())
    }

    /// Compute expiry without wrapping, including after database clock changes.
    pub(crate) fn expires_at(&self, renewed_ns: i64) -> Result<i64, crate::error::Error> {
        self.recipient_ttl_seconds
            .checked_mul(xmtp_common::NS_IN_SEC)
            .and_then(|duration| renewed_ns.checked_add(duration))
            .filter(|_| renewed_ns >= 0)
            .ok_or(crate::error::Error::PushExpiryOverflow)
    }
}

fn required(value: &Option<String>, field: &'static str) -> Result<(), ConfigError> {
    if value.as_ref().is_none_or(|value| value.is_empty()) {
        return Err(invalid(field, "is required"));
    }
    Ok(())
}

fn environment_schema(_: &mut SchemaGenerator) -> Schema {
    super::schema::with_environment(
        json_schema!({"type": "string", "enum": ["production", "sandbox"]}),
    )
}

/// Accept DNS host names and at most one leading wildcard label.
fn valid_domain(value: &str) -> bool {
    let host = value.strip_prefix("*.").unwrap_or(value);
    !host.is_empty()
        && host.len() <= MAX_HOST_BYTES
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= MAX_HOST_LABEL_BYTES
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}
