//! Optional caller authentication settings. Key values and URLs stay out of diagnostics.
use super::{ConfigError, invalid, schema};
use jsonwebtoken::Algorithm;
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use xmtp_common::time::{Duration, Instant};

pub const MAX_KID_BYTES: usize = 256;
pub(crate) const MAX_API_KEYS: usize = 256;
pub(crate) const MIN_API_KEY_BYTES: usize = 32;
// Equals crate::auth::verify::MAX_TOKEN_BYTES: a longer key could never be presented.
pub(crate) const MAX_API_KEY_BYTES: usize = 8192;
const MAX_API_KEY_NAME_BYTES: usize = 64;
pub const MAX_LEEWAY_SECONDS: u64 = 300;
pub const MIN_JWKS_REFRESH_SECONDS: u64 = 1;
/// The refresh loop adds up to `period / JITTER_DIVISOR` to each wait.
pub const JITTER_DIVISOR: u64 = 10;
pub const JWKS_FETCH_TIMEOUT: Duration = Duration::from_secs(10);
const DEFAULT_LEEWAY_SECONDS: u64 = 60;
const DEFAULT_JWKS_REFRESH_SECONDS: u64 = 300;
const DEFAULT_JWKS_MAX_STALE_SECONDS: u64 = 3600;

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct AuthConfig {
    /// Fetch signing keys from HTTPS, or HTTP on a loopback host.
    #[schemars(schema_with = "jwks_url_schema")]
    pub jwks_url: Option<String>,
    /// Inline public signing keys. Cannot be combined with jwks_url.
    pub keys: Option<Vec<AuthKeyConfig>>,
    /// Named static API keys. Values can be supplied with env:NAME.
    #[schemars(schema_with = "api_keys_schema")]
    pub api_keys: BTreeMap<String, String>,
    /// Require an audience that matches one of these values when set.
    pub audiences: Option<Vec<String>>,
    /// Require an issuer that matches one of these values when set.
    pub issuers: Option<Vec<String>>,
    /// Require every one of these scopes on every authenticated RPC.
    pub required_scopes: Vec<String>,
    #[schemars(range(max = MAX_LEEWAY_SECONDS))]
    pub leeway_seconds: u64,
    #[schemars(range(min = MIN_JWKS_REFRESH_SECONDS))]
    pub jwks_refresh_seconds: u64,
    /// Must be at least the refresh interval plus the ten-second fetch timeout.
    pub jwks_max_stale_seconds: u64,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AuthKeyConfig {
    #[serde(default)]
    #[schemars(length(min = 1, max = MAX_KID_BYTES))]
    pub kid: String,
    #[schemars(schema_with = "algorithm_schema")]
    pub alg: String,
    /// PEM SubjectPublicKeyInfo for the selected algorithm, or env:NAME.
    #[schemars(schema_with = "public_key_schema")]
    pub public_key: String,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            jwks_url: None,
            keys: None,
            api_keys: BTreeMap::new(),
            audiences: None,
            issuers: None,
            required_scopes: Vec::new(),
            leeway_seconds: DEFAULT_LEEWAY_SECONDS,
            jwks_refresh_seconds: DEFAULT_JWKS_REFRESH_SECONDS,
            jwks_max_stale_seconds: DEFAULT_JWKS_MAX_STALE_SECONDS,
        }
    }
}

impl AuthConfig {
    /// Reject invalid key sources, keys, RPC names, and timer relationships.
    /// Key errors identify the entry but never include the key value.
    pub fn validate(&self) -> Result<(), ConfigError> {
        match (&self.jwks_url, &self.keys) {
            (Some(_), Some(_)) => {
                return Err(invalid(
                    "auth.jwks_url/auth.keys",
                    "set exactly one key source",
                ));
            }
            (None, None) if self.api_keys.is_empty() => {
                return Err(invalid("auth", "set api_keys, jwks_url, or keys"));
            }
            (_, Some(keys)) if keys.is_empty() => {
                return Err(invalid("auth.keys", "must not be empty"));
            }
            _ => {}
        }
        for (name, value) in &self.api_keys {
            let error = |reason| ConfigError::Auth {
                field: format!("auth.api_keys.{name}"),
                reason,
            };
            if name.is_empty()
                || name.len() > MAX_API_KEY_NAME_BYTES
                || !name.as_bytes()[0].is_ascii_alphanumeric()
                || !name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
            {
                return Err(error("name must match ^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$"));
            }
            if !(MIN_API_KEY_BYTES..=MAX_API_KEY_BYTES).contains(&value.len()) {
                return Err(error("value must be 32 to 8192 bytes"));
            }
            if !value.bytes().all(|byte| (0x21..=0x7e).contains(&byte)) {
                return Err(error("value must be printable ASCII without whitespace"));
            }
        }
        if self.api_keys.len() > MAX_API_KEYS {
            return Err(ConfigError::Auth {
                field: "auth.api_keys".into(),
                reason: "must not exceed 256 keys",
            });
        }
        let mut names_by_value = BTreeMap::new();
        for (name, value) in &self.api_keys {
            if let Some(previous) = names_by_value.insert(value, name) {
                return Err(ConfigError::Auth {
                    field: format!("auth.api_keys.{previous}/{name}"),
                    reason: "values must be unique across names",
                });
            }
        }
        // An empty list would turn its claim into a requirement that nothing
        // can satisfy, so every token would be rejected by a config that looks
        // valid. Absent means "do not check"; empty is a mistake.
        for (field, values) in [
            ("auth.audiences", &self.audiences),
            ("auth.issuers", &self.issuers),
        ] {
            if values.as_ref().is_some_and(|values| values.is_empty()) {
                return Err(invalid(field, "must not be empty when set"));
            }
        }
        if let Some(url) = &self.jwks_url {
            parse_jwks_url(url)?;
        }
        let mut kids = BTreeSet::new();
        for (index, key) in self.keys.iter().flatten().enumerate() {
            let field = format!("auth.keys[{index}]");
            let error = |reason| ConfigError::Auth {
                field: field.clone(),
                reason,
            };
            if key.kid.is_empty() || key.kid.len() > MAX_KID_BYTES || !kids.insert(&key.kid) {
                return Err(error(
                    "kid must be non-empty, unique, and at most 256 bytes",
                ));
            }
            let alg = algorithm(&key.alg)
                .ok_or_else(|| error("alg must be a supported asymmetric algorithm"))?;
            crate::auth::keys::parse_public_key(&key.public_key, alg)
                .map_err(|_| error("public_key must be SubjectPublicKeyInfo for alg"))?;
        }
        if self.leeway_seconds > MAX_LEEWAY_SECONDS {
            return Err(invalid("auth.leeway_seconds", "must not exceed 300"));
        }
        if self.jwks_refresh_seconds < MIN_JWKS_REFRESH_SECONDS {
            return Err(invalid("auth.jwks_refresh_seconds", "must be at least 1"));
        }
        // One refresh cycle must fit inside the staleness window, or a healthy
        // server drains. A cycle is the period, plus up to 10% jitter, plus a
        // fetch that may run to its timeout.
        if self
            .jwks_refresh_seconds
            .checked_add(self.jwks_refresh_seconds.div_ceil(JITTER_DIVISOR))
            .and_then(|min| min.checked_add(JWKS_FETCH_TIMEOUT.as_secs()))
            .is_none_or(|min| self.jwks_max_stale_seconds < min)
        {
            return Err(invalid(
                "auth.jwks_max_stale_seconds",
                "must cover jwks_refresh_seconds plus its jitter plus 10",
            ));
        }
        if Instant::now()
            .checked_add(Duration::from_secs(self.jwks_max_stale_seconds))
            .is_none()
        {
            return Err(invalid(
                "auth.jwks_max_stale_seconds",
                "deadline cannot be represented on this host",
            ));
        }
        Ok(())
    }
}

impl std::fmt::Debug for AuthConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthConfig")
            .field(
                "mode",
                &if self.jwks_url.is_some() {
                    "jwks"
                } else {
                    "inline"
                },
            )
            .field("key_count", &self.keys.as_ref().map_or(0, Vec::len))
            .field("api_key_count", &self.api_keys.len())
            .field("required_scopes", &self.required_scopes)
            .finish()
    }
}

pub(crate) fn algorithm(name: &str) -> Option<Algorithm> {
    match name {
        "RS256" => Some(Algorithm::RS256),
        "RS384" => Some(Algorithm::RS384),
        "RS512" => Some(Algorithm::RS512),
        "ES256" => Some(Algorithm::ES256),
        "ES384" => Some(Algorithm::ES384),
        "EdDSA" => Some(Algorithm::EdDSA),
        _ => None,
    }
}

/// Parse the key source without including its value in errors.
/// Plain HTTP is limited to literal loopback addresses and localhost.
pub(crate) fn parse_jwks_url(value: &str) -> Result<url::Url, ConfigError> {
    let error = || invalid("auth.jwks_url", "must be HTTPS or HTTP on a loopback host");
    let url = url::Url::parse(value).map_err(|_| error())?;
    if url.host().is_none()
        || !(url.scheme() == "https" || (url.scheme() == "http" && is_loopback(&url)))
    {
        return Err(error());
    }
    Ok(url)
}

pub(crate) fn is_loopback(url: &url::Url) -> bool {
    match url.host() {
        Some(url::Host::Domain("localhost")) => true,
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        _ => false,
    }
}
fn public_key_schema(_: &mut SchemaGenerator) -> Schema {
    schema::with_environment(
        json_schema!({"type": "string", "pattern": "^-----BEGIN PUBLIC KEY-----"}),
    )
}
fn jwks_url_schema(_: &mut SchemaGenerator) -> Schema {
    json_schema!({"anyOf": [schema::with_environment(json_schema!({"type": "string", "format": "uri", "pattern": "^https?://"})), {"type": "null"}]})
}
fn api_keys_schema(_: &mut SchemaGenerator) -> Schema {
    json_schema!({
        "type": "object",
        "maxProperties": MAX_API_KEYS,
        "propertyNames": {"pattern": "^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$(?![\\s\\S])"},
        "additionalProperties": schema::with_environment(json_schema!({
            "type": "string",
            "minLength": MIN_API_KEY_BYTES,
            "maxLength": MAX_API_KEY_BYTES,
            "pattern": "^[!-~]+$(?![\\s\\S])"
        }))
    })
}
fn algorithm_schema(_: &mut SchemaGenerator) -> Schema {
    json_schema!({"type": "string", "enum": ["RS256", "RS384", "RS512", "ES256", "ES384", "EdDSA"]})
}

#[cfg(test)]
mod tests;
