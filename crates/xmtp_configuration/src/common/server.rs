//! What one backend deployment publishes about itself, and the provider every
//! consumer reads it through.
//!
//! The backend answers `ConfigurationService.GetConfiguration` with these
//! values. A client fetches them once, stores them, and holds one immutable
//! snapshot for the life of the client. Consumers never read the database for
//! a configuration value; they read the provider.

use crate::{
    BACKEND_DEFAULT_GROUP_MESSAGE_SECONDS, BACKEND_DEFAULT_KEY_PACKAGE_SECONDS,
    BACKEND_DEFAULT_MAX_ENVELOPE_BYTES, BACKEND_DEFAULT_MAX_IDENTITY_ENTRIES,
    BACKEND_DEFAULT_MAX_LOOKUP_IDENTIFIERS, BACKEND_DEFAULT_MAX_NEWEST_FULL_TOPICS,
    BACKEND_DEFAULT_MAX_NEWEST_METADATA_TOPICS, BACKEND_DEFAULT_MAX_PING_BURST,
    BACKEND_DEFAULT_MAX_PING_FRAMES_PER_SECOND, BACKEND_DEFAULT_MAX_PUBLISH_TOPICS,
    BACKEND_DEFAULT_MAX_QUERY_LIMIT, BACKEND_DEFAULT_MAX_QUERY_TOPICS,
    BACKEND_DEFAULT_MAX_REQUEST_BYTES, BACKEND_DEFAULT_MAX_RESPONSE_BYTES,
    BACKEND_DEFAULT_MAX_SCW_SIGNATURES, BACKEND_DEFAULT_MAX_STATIC_TOPICS,
    BACKEND_DEFAULT_MAX_STREAM_TOPICS, BACKEND_DEFAULT_MAX_UPDATE_ADDS,
    BACKEND_DEFAULT_MAX_UPDATE_BURST, BACKEND_DEFAULT_MAX_UPDATE_FRAMES_PER_SECOND,
    BACKEND_DEFAULT_MAX_UPDATE_REMOVES, BACKEND_DEFAULT_QUERY_LIMIT,
    BACKEND_DEFAULT_WELCOME_SECONDS, ENABLE_COMMIT_LOG, MAX_GROUP_SIZE,
    MAX_INSTALLATIONS_PER_INBOX,
};

/// How long after one refresh run ends before the next one starts, before
/// jitter (CFG-046). The first run starts this long after build.
pub const CONFIGURATION_REFRESH_INTERVAL: std::time::Duration =
    std::time::Duration::from_secs(3600);

/// Random spread added to each wait, so a fleet of clients does not refresh
/// in lockstep (CFG-046).
pub const CONFIGURATION_REFRESH_JITTER: std::time::Duration = std::time::Duration::from_secs(360);

/// Attempts in one refresh run. After the last failure the run ends and the
/// stored copy is left alone (CFG-047).
pub const CONFIGURATION_REFRESH_ATTEMPTS: usize = 3;

/// Waits between the attempts of one run (CFG-046).
pub const CONFIGURATION_REFRESH_BACKOFF: [std::time::Duration; 2] = [
    std::time::Duration::from_secs(5),
    std::time::Duration::from_secs(30),
];

/// Longest operator identifier, in bytes. Shared with the backend so one rule
/// governs what it accepts and what a client will store.
pub const MAX_SERVER_IDENTIFIER_BYTES: usize = 256;

/// Why a published configuration cannot be used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ServerConfigurationError {
    #[error(
        "identifier must be 1 to {MAX_SERVER_IDENTIFIER_BYTES} bytes with no whitespace or control characters"
    )]
    Identifier,
    #[error("min_libxmtp_version {version:?} is not a semantic version")]
    MinimumVersion { version: String },
    #[error("{chain:?} is not a CAIP-2 chain identifier")]
    Chain { chain: String },
}

/// Check the shape of an operator identifier. The backend refuses to start
/// with one that fails, and a client refuses to store one.
pub fn validate_server_identifier(identifier: &str) -> Result<(), ServerConfigurationError> {
    if identifier.is_empty()
        || identifier.len() > MAX_SERVER_IDENTIFIER_BYTES
        || identifier
            .chars()
            .any(|c| c.is_whitespace() || c.is_control())
    {
        return Err(ServerConfigurationError::Identifier);
    }
    Ok(())
}

/// A CAIP-2 identifier is `namespace:reference`, both non-empty and free of
/// separators. The client only needs the shape; the verifier owns the routes.
pub fn is_caip2_chain_id(chain: &str) -> bool {
    let Some((namespace, reference)) = chain.split_once(':') else {
        return false;
    };
    !namespace.is_empty()
        && !reference.is_empty()
        && namespace.len() <= 8
        && reference.len() <= 32
        && namespace
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && reference
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Compare two versions on major, minor, and patch only. A prerelease tag
/// never makes a client too old for a minimum it otherwise satisfies.
///
/// Returns `true` when `version` is below `minimum`.
pub fn version_is_below(version: &semver::Version, minimum: &semver::Version) -> bool {
    (version.major, version.minor, version.patch) < (minimum.major, minimum.minor, minimum.patch)
}

/// Public identity of one accepted signing key. Never the key itself.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SigningKeyDescription {
    pub kid: String,
    pub alg: String,
}

/// What a client must present to be admitted. The client acts only on
/// `enabled` and `required_scopes`; the rest exists for operator tooling.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AuthConfiguration {
    pub enabled: bool,
    pub keys: Vec<SigningKeyDescription>,
    pub audiences: Vec<String>,
    pub issuers: Vec<String>,
    pub required_scopes: Vec<String>,
}

/// How long the deployment keeps each payload kind, in seconds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetentionConfiguration {
    pub group_message_seconds: u64,
    pub welcome_seconds: u64,
    pub key_package_seconds: u64,
}

impl Default for RetentionConfiguration {
    fn default() -> Self {
        Self {
            group_message_seconds: BACKEND_DEFAULT_GROUP_MESSAGE_SECONDS,
            welcome_seconds: BACKEND_DEFAULT_WELCOME_SECONDS,
            key_package_seconds: BACKEND_DEFAULT_KEY_PACKAGE_SECONDS,
        }
    }
}

/// Request shapes the deployment accepts. A client chunks its work to these
/// values rather than to its compiled constants.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LimitsConfiguration {
    pub max_envelope_bytes: usize,
    pub max_request_bytes: usize,
    pub max_response_bytes: usize,
    pub max_publish_topics: usize,
    pub max_query_topics: usize,
    pub max_query_limit: usize,
    pub default_query_limit: usize,
    pub max_newest_metadata_topics: usize,
    pub max_newest_full_topics: usize,
    pub max_update_adds: usize,
    pub max_update_removes: usize,
    pub max_stream_topics: usize,
    pub max_static_topics: usize,
    pub max_lookup_identifiers: usize,
    pub max_scw_signatures: usize,
    pub max_identity_entries: usize,
    pub max_update_frames_per_second: u32,
    pub max_update_burst: u32,
    pub max_ping_frames_per_second: u32,
    pub max_ping_burst: u32,
}

impl Default for LimitsConfiguration {
    fn default() -> Self {
        Self {
            max_envelope_bytes: BACKEND_DEFAULT_MAX_ENVELOPE_BYTES,
            max_request_bytes: BACKEND_DEFAULT_MAX_REQUEST_BYTES,
            max_response_bytes: BACKEND_DEFAULT_MAX_RESPONSE_BYTES,
            max_publish_topics: BACKEND_DEFAULT_MAX_PUBLISH_TOPICS,
            max_query_topics: BACKEND_DEFAULT_MAX_QUERY_TOPICS,
            max_query_limit: BACKEND_DEFAULT_MAX_QUERY_LIMIT,
            default_query_limit: BACKEND_DEFAULT_QUERY_LIMIT,
            max_newest_metadata_topics: BACKEND_DEFAULT_MAX_NEWEST_METADATA_TOPICS,
            max_newest_full_topics: BACKEND_DEFAULT_MAX_NEWEST_FULL_TOPICS,
            max_update_adds: BACKEND_DEFAULT_MAX_UPDATE_ADDS,
            max_update_removes: BACKEND_DEFAULT_MAX_UPDATE_REMOVES,
            max_stream_topics: BACKEND_DEFAULT_MAX_STREAM_TOPICS,
            max_static_topics: BACKEND_DEFAULT_MAX_STATIC_TOPICS,
            max_lookup_identifiers: BACKEND_DEFAULT_MAX_LOOKUP_IDENTIFIERS,
            max_scw_signatures: BACKEND_DEFAULT_MAX_SCW_SIGNATURES,
            max_identity_entries: BACKEND_DEFAULT_MAX_IDENTITY_ENTRIES,
            max_update_frames_per_second: BACKEND_DEFAULT_MAX_UPDATE_FRAMES_PER_SECOND,
            max_update_burst: BACKEND_DEFAULT_MAX_UPDATE_BURST,
            max_ping_frames_per_second: BACKEND_DEFAULT_MAX_PING_FRAMES_PER_SECOND,
            max_ping_burst: BACKEND_DEFAULT_MAX_PING_BURST,
        }
    }
}

impl LimitsConfiguration {
    /// The same snapshot with every zero replaced by the compiled default.
    ///
    /// CFG-031 already applies this rule to what arrives on the wire, so no
    /// published configuration can carry a zero. A snapshot built in Rust and
    /// handed in through a `ConfigProvider` (CFG-033) skips that conversion,
    /// and a zero chunk dimension would panic the transport that slices its
    /// work into chunks of it (CFG-064). Applying the wire rule once more,
    /// where the transport reads the value, means it never can.
    pub fn without_zeroes(&self) -> Self {
        let default = Self::default();
        macro_rules! or_default {
            ($($field:ident),+ $(,)?) => {
                Self {
                    $($field: if self.$field == 0 { default.$field } else { self.$field }),+
                }
            };
        }
        or_default!(
            max_envelope_bytes,
            max_request_bytes,
            max_response_bytes,
            max_publish_topics,
            max_query_topics,
            max_query_limit,
            default_query_limit,
            max_newest_metadata_topics,
            max_newest_full_topics,
            max_update_adds,
            max_update_removes,
            max_stream_topics,
            max_static_topics,
            max_lookup_identifiers,
            max_scw_signatures,
            max_identity_entries,
            max_update_frames_per_second,
            max_update_burst,
            max_ping_frames_per_second,
            max_ping_burst,
        )
    }
}

/// Advisory group shapes. The backend publishes them and does not enforce them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MlsConfiguration {
    pub max_group_members: usize,
    pub max_installations_per_inbox: usize,
    /// Absent means the client keeps its compiled default. `false` is distinct
    /// from absent, so an operator can switch the commit log off explicitly.
    pub commit_log_enabled: Option<bool>,
}

impl MlsConfiguration {
    /// Whether this deployment wants clients to write and read the commit log.
    pub fn commit_log_enabled(&self) -> bool {
        self.commit_log_enabled.unwrap_or(ENABLE_COMMIT_LOG)
    }
}

impl Default for MlsConfiguration {
    fn default() -> Self {
        Self {
            max_group_members: MAX_GROUP_SIZE,
            max_installations_per_inbox: MAX_INSTALLATIONS_PER_INBOX,
            commit_log_enabled: None,
        }
    }
}

/// One immutable snapshot of what a deployment published.
///
/// `Default` is the compiled fallback used before any fetch succeeds, and for
/// any field the deployment left at zero.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ServerConfiguration {
    /// Stable operator-chosen name. Empty only before a first fetch succeeds.
    pub identifier: String,
    pub server_version: String,
    /// Empty when the operator published no minimum.
    pub min_libxmtp_version: String,
    pub auth: AuthConfiguration,
    pub retention: RetentionConfiguration,
    pub limits: LimitsConfiguration,
    pub mls: MlsConfiguration,
    /// CAIP-2 chain ids this deployment verifies smart contract wallet
    /// signatures on. Empty rejects every app-supplied signature.
    pub smart_contract_wallet_chains: Vec<String>,
}

impl ServerConfiguration {
    /// Apply the rules of CFG-044: the identifier must be well formed, any
    /// minimum version must parse, and every chain must be CAIP-2.
    pub fn validate(&self) -> Result<(), ServerConfigurationError> {
        validate_server_identifier(&self.identifier)?;
        self.minimum_version()?;
        for chain in &self.smart_contract_wallet_chains {
            if !is_caip2_chain_id(chain) {
                return Err(ServerConfigurationError::Chain {
                    chain: chain.clone(),
                });
            }
        }
        Ok(())
    }

    /// The published minimum, parsed. `None` admits every client version.
    pub fn minimum_version(&self) -> Result<Option<semver::Version>, ServerConfigurationError> {
        if self.min_libxmtp_version.is_empty() {
            return Ok(None);
        }
        semver::Version::parse(&self.min_libxmtp_version)
            .map(Some)
            .map_err(|_| ServerConfigurationError::MinimumVersion {
                version: self.min_libxmtp_version.clone(),
            })
    }

    /// Whether this deployment accepts a smart contract wallet signature on
    /// `chain`. An empty list accepts none.
    pub fn accepts_chain(&self, chain: &str) -> bool {
        self.smart_contract_wallet_chains
            .iter()
            .any(|accepted| accepted == chain)
    }
}

/// One immutable configuration, read by every consumer in section 6.4.
///
/// A provider never touches the database. The snapshot is resolved once at
/// build and handed to the client whole.
pub trait ConfigProvider:
    std::fmt::Debug + xmtp_common::MaybeSend + xmtp_common::MaybeSync
{
    fn server_configuration(&self) -> &ServerConfiguration;
}

/// The provider backed by the copy stored in the client database.
#[derive(Debug, Clone, Default)]
pub struct StoredConfigProvider(ServerConfiguration);

impl StoredConfigProvider {
    pub fn new(configuration: ServerConfiguration) -> Self {
        Self(configuration)
    }
}

impl ConfigProvider for StoredConfigProvider {
    fn server_configuration(&self) -> &ServerConfiguration {
        &self.0
    }
}

/// A provider a test constructs from any values, with no database and no
/// fetch. Passing one to the builder disables fetch, store, refresh, and the
/// identifier check.
#[derive(Debug, Clone, Default)]
pub struct StaticConfigProvider(ServerConfiguration);

impl StaticConfigProvider {
    pub fn new(configuration: ServerConfiguration) -> Self {
        Self(configuration)
    }

    /// Start from the compiled defaults and change only what a test needs.
    pub fn edited(edit: impl FnOnce(&mut ServerConfiguration)) -> Self {
        let mut configuration = ServerConfiguration {
            identifier: "org.xmtp.static".to_owned(),
            ..Default::default()
        };
        edit(&mut configuration);
        Self(configuration)
    }
}

impl ConfigProvider for StaticConfigProvider {
    fn server_configuration(&self) -> &ServerConfiguration {
        &self.0
    }
}

#[cfg(test)]
mod tests;
