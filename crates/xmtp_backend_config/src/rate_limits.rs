//! Per-caller rate limits ([RATE](../../docs/specs/RATE-backend-rate-limits.md)):
//! the `[rate_limits]` table, its checks, and the limits that apply to a
//! caller. The file layer is `RateLimitsSection`; validation resolves it into
//! a `RateLimitConfig`, which the backend reads. Caller key values stay out of
//! diagnostics.

use std::{
    collections::{BTreeMap, HashMap},
    net::IpAddr,
};

use ipnet::{IpNet, Ipv4Net};
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};

use super::{ConfigError, Step, invalid, key_path, schema};

const DEFAULT_MAX_MEMORY_MIB: u32 = 100;
const MAX_MEMORY_MIB: u32 = 65_536;
/// Thirty days.
const MAX_WINDOW_SECONDS: u32 = 2_592_000;
/// A limiter slot records three limits for each category.
const MAX_LIMITS_PER_LIST: usize = 3;

/// A kind of work with its own unit and limits (RATE section 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Category {
    Request,
    Publish,
    IdentityUpdate,
    ChainVerify,
    Push,
    Upload,
    StreamRent,
}

impl Category {
    pub const ALL: [Self; 7] = [
        Self::Request,
        Self::Publish,
        Self::IdentityUpdate,
        Self::ChainVerify,
        Self::Push,
        Self::Upload,
        Self::StreamRent,
    ];

    /// The name the configuration and a rejection use.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Request => "request",
            Self::Publish => "publish",
            Self::IdentityUpdate => "identity_update",
            Self::ChainVerify => "chain_verify",
            Self::Push => "push",
            Self::Upload => "upload",
            Self::StreamRent => "stream_rent",
        }
    }

    /// The category the configuration names `name`.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|category| category.as_str() == name)
    }

    /// The default limits table of RATE section 3.
    pub const fn default_limits(self) -> &'static [LimitConfig] {
        DEFAULT_LIMITS[self.index()]
    }

    const fn index(self) -> usize {
        self as usize
    }
}

const fn limit(count: u32, window_seconds: u32) -> LimitConfig {
    LimitConfig {
        count,
        window_seconds,
    }
}

/// The default limits table of RATE section 3, in `Category::ALL` order.
const DEFAULT_LIMITS: [&[LimitConfig]; 7] = [
    &[limit(6_000, 60), limit(200_000, 3_600)],
    &[limit(1_000, 60), limit(20_000, 3_600)],
    &[limit(10, 60), limit(100, 3_600)],
    &[limit(200, 60), limit(2_000, 3_600)],
    &[limit(2_000, 60), limit(20_000, 3_600)],
    &[limit(120, 60), limit(2_000, 3_600)],
    &[limit(120, 60)],
];

/// The form of a caller key, which selects its tier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AuthType {
    Ip,
    Jwt,
    ApiKey,
}

impl AuthType {
    pub const ALL: [Self; 3] = [Self::Ip, Self::Jwt, Self::ApiKey];

    /// The name the configuration uses for this tier.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ip => "ip",
            Self::Jwt => "jwt",
            Self::ApiKey => "api_key",
        }
    }

    const fn index(self) -> usize {
        match self {
            Self::Ip => 0,
            Self::Jwt => 1,
            Self::ApiKey => 2,
        }
    }

    /// The first byte of `CallerKey::to_bytes`.
    const fn tag(self) -> u8 {
        match self {
            Self::Ip => 1,
            Self::Jwt => 2,
            Self::ApiKey => 3,
        }
    }
}

/// The value a request's charges are recorded under.
/// `Ipv4` holds an IPv4 address and `Ipv6Prefix` the first 64 bits of an
/// IPv6 address; `from_ip` maps an IPv4-mapped IPv6 address to IPv4. Debug
/// output shows an API key name, which is a principal, and redacts every
/// other value: a JWT claim or a client address never reaches a log.
#[derive(Clone, PartialEq, Eq, Hash)]
pub enum CallerKey {
    ApiKey(String),
    /// A token with no `iss` has the empty issuer.
    Jwt {
        issuer: String,
        subject: String,
    },
    Ipv4([u8; 4]),
    Ipv6Prefix([u8; 8]),
}

impl CallerKey {
    // implements: RATE-003
    pub fn from_ip(ip: IpAddr) -> Self {
        match ip {
            IpAddr::V4(ip) => Self::Ipv4(ip.octets()),
            IpAddr::V6(ip) => match ip.to_ipv4_mapped() {
                Some(ip) => Self::Ipv4(ip.octets()),
                None => {
                    let mut prefix = [0; 8];
                    prefix.copy_from_slice(&ip.octets()[..8]);
                    Self::Ipv6Prefix(prefix)
                }
            },
        }
    }

    pub fn auth_type(&self) -> AuthType {
        match self {
            Self::ApiKey(_) => AuthType::ApiKey,
            Self::Jwt { .. } => AuthType::Jwt,
            Self::Ipv4(_) | Self::Ipv6Prefix(_) => AuthType::Ip,
        }
    }

    /// Append the bytes the limiter hashes: an auth type tag, then the
    /// payload, with the issuer length-prefixed. The payload of each auth
    /// type is unambiguous on its own, and an IPv4 and an IPv6 payload differ
    /// in length, so two different keys never give the same bytes.
    pub fn to_bytes(&self, out: &mut Vec<u8>) {
        out.push(self.auth_type().tag());
        match self {
            Self::ApiKey(name) => out.extend_from_slice(name.as_bytes()),
            Self::Jwt { issuer, subject } => {
                out.extend_from_slice(&(issuer.len() as u64).to_be_bytes());
                out.extend_from_slice(issuer.as_bytes());
                out.extend_from_slice(subject.as_bytes());
            }
            Self::Ipv4(octets) => out.extend_from_slice(octets),
            Self::Ipv6Prefix(prefix) => out.extend_from_slice(prefix),
        }
    }
}

impl std::fmt::Debug for CallerKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ApiKey(name) => formatter.debug_tuple("ApiKey").field(name).finish(),
            Self::Jwt { .. } => formatter.write_str("Jwt(<redacted>)"),
            Self::Ipv4(_) => formatter.write_str("Ipv4(<redacted>)"),
            Self::Ipv6Prefix(_) => formatter.write_str("Ipv6Prefix(<redacted>)"),
        }
    }
}

/// One limit: a sustained `count` units per `window_seconds`, with a burst of
/// up to `count` units.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LimitConfig {
    #[schemars(schema_with = "schema::positive_integer::<{ u32::MAX as u64 }>")]
    pub count: u32,
    #[schemars(range(min = 1, max = MAX_WINDOW_SECONDS))]
    pub window_seconds: u32,
}

/// The tier of a category's table that applies to every auth type the table
/// does not name.
const DEFAULT_TIER: &str = "default";

/// The `[rate_limits]` table as the file writes it. `ValidatedConfig::rate_limits`
/// gives the limits it resolves to.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct RateLimitsSection {
    /// Charge requests to callers and refuse work over their limits. False
    /// admits every request.
    pub enabled: bool,
    /// The most memory the limiter's state may use, in MiB.
    #[schemars(range(min = 1, max = MAX_MEMORY_MIB))]
    pub max_memory_mib: u32,
    /// Networks of proxies whose `X-Forwarded-For` entries name the client.
    /// Startup also refuses a network with a bit set after its prefix. An
    /// IPv4-mapped network matches the IPv4 addresses it maps.
    #[schemars(schema_with = "trusted_proxies_schema")]
    pub trusted_proxies: Vec<String>,
    /// Limits for each category, by tier: `default`, `ip`, `jwt`, or
    /// `api_key`. A tier left out gets the category's `default`, and a
    /// category without `default` gets the default limits of RATE section 3.
    /// Names are checked at validation, so a refusal can name the key.
    #[schemars(schema_with = "categories_schema")]
    pub categories: BTreeMap<String, BTreeMap<String, Vec<LimitConfig>>>,
    /// Limits for one caller, which replace every other limit of the
    /// categories they name.
    pub overrides: Vec<RateLimitOverride>,
}

impl Default for RateLimitsSection {
    fn default() -> Self {
        Self {
            enabled: true,
            max_memory_mib: DEFAULT_MAX_MEMORY_MIB,
            trusted_proxies: Vec::new(),
            categories: BTreeMap::new(),
            overrides: Vec::new(),
        }
    }
}

/// One entry of `rate_limits.overrides`: exactly one of `api_key`,
/// `jwt_subject`, and `ip` names the caller.
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RateLimitOverride {
    /// An API key name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "api_key_schema")]
    pub api_key: Option<String>,
    /// The token's `iss`. Absent names a token with no `iss`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "string_schema")]
    pub jwt_issuer: Option<String>,
    /// The token's `sub`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "subject_schema")]
    pub jwt_subject: Option<String>,
    /// An IP address. An IPv6 address names its /64.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "schema::ip_address")]
    pub ip: Option<String>,
    /// Limits by category name.
    #[schemars(schema_with = "override_limits_schema")]
    pub limits: BTreeMap<String, Vec<LimitConfig>>,
}

impl std::fmt::Debug for RateLimitOverride {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let redacted = |value: &Option<String>| value.as_ref().map(|_| "<redacted>");
        formatter
            .debug_struct("RateLimitOverride")
            .field("api_key", &self.api_key)
            .field("jwt_issuer", &redacted(&self.jwt_issuer))
            .field("jwt_subject", &redacted(&self.jwt_subject))
            .field("ip", &redacted(&self.ip))
            .field("limits", &self.limits)
            .finish()
    }
}

/// The limits of each category one override names, by `Category` index.
type OverrideLimits = Vec<Option<Vec<LimitConfig>>>;

/// `network` as the IPv4 network it maps when it lies within the IPv4-mapped
/// range. A peer with an IPv4-mapped address is compared as its IPv4
/// address, which an IPv6 network never contains.
fn ipv4_network(network: IpNet) -> IpNet {
    match network {
        IpNet::V6(v6) if v6.prefix_len() >= 96 => v6
            .addr()
            .to_ipv4_mapped()
            .and_then(|address| Ipv4Net::new(address, v6.prefix_len() - 96).ok())
            .map_or(network, IpNet::V4),
        network => network,
    }
}

/// The rate-limit settings a validated configuration resolves to.
#[derive(Clone, Debug)]
pub struct RateLimitConfig {
    pub enabled: bool,
    pub max_memory_mib: u32,
    /// An IPv4-mapped network is resolved to the IPv4 network it maps.
    pub trusted_proxies: Vec<IpNet>,
    /// Each list holds at most three limits with different windows, and an
    /// empty list admits every spend.
    limits: CallerLimits<Vec<LimitConfig>>,
}

impl RateLimitConfig {
    /// The limits that apply to `caller` in `category`; see [`CallerLimits::get`].
    pub fn limits(&self, category: Category, caller: &CallerKey) -> &[LimitConfig] {
        self.limits.get(category, caller)
    }

    /// Every caller's limits, for a server to convert once at startup.
    pub fn caller_limits(&self) -> &CallerLimits<Vec<LimitConfig>> {
        &self.limits
    }
}

/// A value, such as a limit list, for each category and each caller: one per
/// auth type, and one for each category an override names.
#[derive(Clone, Debug)]
pub struct CallerLimits<T> {
    /// By `Category` index, then `AuthType` index, with the category's
    /// `default` and the default limits table applied.
    tiers: Vec<[T; 3]>,
    /// By `Category` index.
    overrides: HashMap<CallerKey, Vec<Option<T>>>,
}

impl<T> CallerLimits<T> {
    /// The value that applies to `caller` in `category`, most specific first:
    /// the caller's override for the category, else its auth type's, else the
    /// category's `default`, else the default limits table.
    // implements: RATE-025
    pub fn get(&self, category: Category, caller: &CallerKey) -> &T {
        self.overrides
            .get(caller)
            .and_then(|values| values[category.index()].as_ref())
            .unwrap_or(&self.tiers[category.index()][caller.auth_type().index()])
    }

    /// The same table with every value mapped by `f`, or the first error `f`
    /// returns. A server uses it to build its own form of each list once.
    pub fn try_map<U, E>(
        &self,
        mut f: impl FnMut(&T) -> Result<U, E>,
    ) -> Result<CallerLimits<U>, E> {
        let mut tiers = Vec::with_capacity(self.tiers.len());
        for [ip, jwt, api_key] in &self.tiers {
            tiers.push([f(ip)?, f(jwt)?, f(api_key)?]);
        }
        let mut overrides = HashMap::with_capacity(self.overrides.len());
        for (caller, values) in &self.overrides {
            let mut mapped = Vec::with_capacity(values.len());
            for value in values {
                mapped.push(value.as_ref().map(&mut f).transpose()?);
            }
            overrides.insert(caller.clone(), mapped);
        }
        Ok(CallerLimits { tiers, overrides })
    }
}

impl RateLimitsSection {
    /// Check every key, and resolve the limits each caller gets.
    pub(crate) fn validate(&self) -> Result<RateLimitConfig, ConfigError> {
        if !(1..=MAX_MEMORY_MIB).contains(&self.max_memory_mib) {
            return Err(invalid(
                "rate_limits.max_memory_mib",
                "must be between 1 and 65536",
            ));
        }
        let trusted_proxies = self
            .trusted_proxies
            .iter()
            .enumerate()
            .map(|(index, network)| {
                super::ip_network(network).map(ipv4_network).ok_or_else(|| {
                    invalid(
                        key_path("rate_limits.trusted_proxies", Step::Entry(index)),
                        "must be an IP network with no bit set after its prefix",
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut configured: [Option<&BTreeMap<String, Vec<LimitConfig>>>; 7] = [None; 7];
        for (name, tiers) in &self.categories {
            let path = key_path("rate_limits.categories", Step::Key(name));
            let category = category_named(name, &path)?;
            for (tier, limits) in tiers {
                let path = key_path(&path, Step::Key(tier));
                if tier != DEFAULT_TIER
                    && !AuthType::ALL
                        .iter()
                        .any(|auth_type| auth_type.as_str() == tier)
                {
                    return Err(invalid(path, NOT_ALLOWED));
                }
                validate_limits(&path, limits)?;
            }
            configured[category.index()] = Some(tiers);
        }
        let overrides = self.resolve_overrides()?;
        let tiers = Category::ALL
            .map(|category| {
                AuthType::ALL.map(|auth_type| {
                    configured[category.index()]
                        .and_then(|tiers| {
                            tiers
                                .get(auth_type.as_str())
                                .or_else(|| tiers.get(DEFAULT_TIER))
                        })
                        .map_or_else(|| category.default_limits().to_vec(), Clone::clone)
                })
            })
            .into();
        Ok(RateLimitConfig {
            enabled: self.enabled,
            max_memory_mib: self.max_memory_mib,
            trusted_proxies,
            limits: CallerLimits { tiers, overrides },
        })
    }

    /// Check each override, then that each names one caller key no other
    /// override names.
    // implements: CONF-094
    fn resolve_overrides(&self) -> Result<HashMap<CallerKey, OverrideLimits>, ConfigError> {
        let mut overrides = HashMap::with_capacity(self.overrides.len());
        for (index, entry) in self.overrides.iter().enumerate() {
            let path = key_path("rate_limits.overrides", Step::Entry(index));
            let field = |name| key_path(&path, Step::Key(name));
            if entry
                .api_key
                .as_deref()
                .is_some_and(|name| !super::auth::is_api_key_name(name))
            {
                return Err(invalid(
                    field("api_key"),
                    "must match ^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$",
                ));
            }
            if entry.jwt_subject.as_deref() == Some("") {
                return Err(invalid(field("jwt_subject"), "must not be empty"));
            }
            let ip = match entry.ip.as_deref() {
                Some(ip) => Some(
                    super::ip_address(ip)
                        .ok_or_else(|| invalid(field("ip"), "must be an IP address"))?,
                ),
                None => None,
            };
            let limits_path = field("limits");
            let mut limits: OverrideLimits = vec![None; Category::ALL.len()];
            for (name, list) in &entry.limits {
                let path = key_path(&limits_path, Step::Key(name));
                let category = category_named(name, &path)?;
                validate_limits(&path, list)?;
                limits[category.index()] = Some(list.clone());
            }
            if entry.jwt_issuer.is_some() && entry.jwt_subject.is_none() {
                return Err(invalid(
                    field("jwt_issuer"),
                    "must not be set without jwt_subject",
                ));
            }
            let key = match (&entry.api_key, &entry.jwt_subject, ip) {
                (Some(name), None, None) => CallerKey::ApiKey(name.clone()),
                (None, Some(subject), None) => CallerKey::Jwt {
                    issuer: entry.jwt_issuer.clone().unwrap_or_default(),
                    subject: subject.clone(),
                },
                (None, None, Some(ip)) => CallerKey::from_ip(ip),
                _ => {
                    return Err(invalid(
                        path,
                        "must set exactly one of api_key, jwt_subject, and ip",
                    ));
                }
            };
            if overrides.insert(key, limits).is_some() {
                return Err(invalid(
                    path,
                    "must not name the same caller as an earlier override",
                ));
            }
        }
        Ok(overrides)
    }
}

/// The refusal of a key outside the names its row lists, in the words of a
/// typed-decoding refusal.
const NOT_ALLOWED: &str = "is not one of the allowed values";

/// The category the key at `path` names.
fn category_named(name: &str, path: &str) -> Result<Category, ConfigError> {
    Category::from_name(name).ok_or_else(|| invalid(path, NOT_ALLOWED))
}

/// Check each limit of the list at `path`, then that the list fits a
/// limiter slot: at most three limits, each with its own window.
// implements: CONF-094
fn validate_limits(path: &str, limits: &[LimitConfig]) -> Result<(), ConfigError> {
    for (index, limit) in limits.iter().enumerate() {
        let entry = key_path(path, Step::Entry(index));
        if limit.count == 0 {
            return Err(invalid(
                key_path(&entry, Step::Key("count")),
                "must be greater than zero",
            ));
        }
        if !(1..=MAX_WINDOW_SECONDS).contains(&limit.window_seconds) {
            return Err(invalid(
                key_path(&entry, Step::Key("window_seconds")),
                "must be between 1 and 2592000",
            ));
        }
    }
    if limits.len() > MAX_LIMITS_PER_LIST {
        return Err(invalid(path, "must hold at most 3 limits"));
    }
    for (index, limit) in limits.iter().enumerate() {
        if limits[..index]
            .iter()
            .any(|earlier| earlier.window_seconds == limit.window_seconds)
        {
            return Err(invalid(
                key_path(
                    &key_path(path, Step::Entry(index)),
                    Step::Key("window_seconds"),
                ),
                "must differ from every other window in the list",
            ));
        }
    }
    Ok(())
}

const CATEGORY_NAMES: [&str; 7] = {
    let mut names = [""; 7];
    let mut index = 0;
    while index < Category::ALL.len() {
        names[index] = Category::ALL[index].as_str();
        index += 1;
    }
    names
};

fn limit_list_schema(generator: &mut SchemaGenerator) -> Schema {
    json_schema!({"type": "array", "items": generator.subschema_for::<LimitConfig>()})
}

fn categories_schema(generator: &mut SchemaGenerator) -> Schema {
    json_schema!({
        "type": "object",
        "propertyNames": {"enum": CATEGORY_NAMES},
        "additionalProperties": {
            "type": "object",
            "propertyNames": {"enum": [DEFAULT_TIER, "ip", "jwt", "api_key"]},
            "additionalProperties": limit_list_schema(generator)
        }
    })
}

fn override_limits_schema(generator: &mut SchemaGenerator) -> Schema {
    json_schema!({
        "type": "object",
        "propertyNames": {"enum": CATEGORY_NAMES},
        "additionalProperties": limit_list_schema(generator)
    })
}

fn trusted_proxies_schema(generator: &mut SchemaGenerator) -> Schema {
    json_schema!({"type": "array", "items": schema::ip_network(generator)})
}

fn api_key_schema(_: &mut SchemaGenerator) -> Schema {
    schema::with_environment(json_schema!({
        "type": "string",
        "pattern": "^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$(?![\\s\\S])"
    }))
}

fn string_schema(_: &mut SchemaGenerator) -> Schema {
    json_schema!({"type": "string"})
}

fn subject_schema(_: &mut SchemaGenerator) -> Schema {
    json_schema!({"type": "string", "minLength": 1})
}

#[cfg(test)]
mod tests;
