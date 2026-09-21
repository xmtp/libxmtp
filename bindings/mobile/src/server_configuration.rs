//! What one backend deployment publishes about itself, mirrored for Kotlin and
//! Swift (spec 006 §7).
//!
//! Every record here is a plain translation of the matching
//! [`xmtp_configuration`] type. The client owns the fetch, the validation, and
//! the stored copy; this module only changes the shape. `usize` fields become
//! `u64` because §7 maps them to `Long` on Android and `UInt64` on iOS, and
//! every published value is below 2^53.

use crate::FfiError;
use crate::logger::init_logger;
use xmtp_api::{ApiClientWrapper, strategies};
use xmtp_api_backend::MessageBackendBuilder;

/// Public identity of one accepted signing key. Never the key itself.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct FfiSigningKeyDescription {
    pub kid: String,
    pub alg: String,
}

impl From<&xmtp_configuration::SigningKeyDescription> for FfiSigningKeyDescription {
    fn from(key: &xmtp_configuration::SigningKeyDescription) -> Self {
        Self {
            kid: key.kid.clone(),
            alg: key.alg.clone(),
        }
    }
}

/// What a client must present to be admitted. An app acts on `enabled` and
/// `required_scopes`; the rest is there for operator tooling.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct FfiAuthConfiguration {
    pub enabled: bool,
    pub keys: Vec<FfiSigningKeyDescription>,
    pub audiences: Vec<String>,
    pub issuers: Vec<String>,
    pub required_scopes: Vec<String>,
}

impl From<&xmtp_configuration::AuthConfiguration> for FfiAuthConfiguration {
    fn from(auth: &xmtp_configuration::AuthConfiguration) -> Self {
        Self {
            enabled: auth.enabled,
            keys: auth.keys.iter().map(Into::into).collect(),
            audiences: auth.audiences.clone(),
            issuers: auth.issuers.clone(),
            required_scopes: auth.required_scopes.clone(),
        }
    }
}

/// How long the deployment keeps each payload kind, in seconds.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct FfiRetentionConfiguration {
    pub group_message_seconds: u64,
    pub welcome_seconds: u64,
    pub key_package_seconds: u64,
}

impl From<&xmtp_configuration::RetentionConfiguration> for FfiRetentionConfiguration {
    fn from(retention: &xmtp_configuration::RetentionConfiguration) -> Self {
        Self {
            group_message_seconds: retention.group_message_seconds,
            welcome_seconds: retention.welcome_seconds,
            key_package_seconds: retention.key_package_seconds,
        }
    }
}

/// Request shapes the deployment accepts. The client chunks its work to these
/// values; an app reads them to size its own batches.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct FfiLimitsConfiguration {
    pub max_envelope_bytes: u64,
    pub max_request_bytes: u64,
    pub max_response_bytes: u64,
    pub max_publish_topics: u64,
    pub max_query_topics: u64,
    pub max_query_limit: u64,
    pub default_query_limit: u64,
    pub max_newest_metadata_topics: u64,
    pub max_newest_full_topics: u64,
    pub max_update_adds: u64,
    pub max_update_removes: u64,
    pub max_stream_topics: u64,
    pub max_static_topics: u64,
    pub max_lookup_identifiers: u64,
    pub max_scw_signatures: u64,
    pub max_identity_entries: u64,
    pub max_update_frames_per_second: u32,
    pub max_update_burst: u32,
    pub max_ping_frames_per_second: u32,
    pub max_ping_burst: u32,
}

impl From<&xmtp_configuration::LimitsConfiguration> for FfiLimitsConfiguration {
    fn from(limits: &xmtp_configuration::LimitsConfiguration) -> Self {
        Self {
            max_envelope_bytes: widen(limits.max_envelope_bytes),
            max_request_bytes: widen(limits.max_request_bytes),
            max_response_bytes: widen(limits.max_response_bytes),
            max_publish_topics: widen(limits.max_publish_topics),
            max_query_topics: widen(limits.max_query_topics),
            max_query_limit: widen(limits.max_query_limit),
            default_query_limit: widen(limits.default_query_limit),
            max_newest_metadata_topics: widen(limits.max_newest_metadata_topics),
            max_newest_full_topics: widen(limits.max_newest_full_topics),
            max_update_adds: widen(limits.max_update_adds),
            max_update_removes: widen(limits.max_update_removes),
            max_stream_topics: widen(limits.max_stream_topics),
            max_static_topics: widen(limits.max_static_topics),
            max_lookup_identifiers: widen(limits.max_lookup_identifiers),
            max_scw_signatures: widen(limits.max_scw_signatures),
            max_identity_entries: widen(limits.max_identity_entries),
            max_update_frames_per_second: limits.max_update_frames_per_second,
            max_update_burst: limits.max_update_burst,
            max_ping_frames_per_second: limits.max_ping_frames_per_second,
            max_ping_burst: limits.max_ping_burst,
        }
    }
}

/// Advisory group shapes. The backend publishes them and does not enforce them.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct FfiMlsConfiguration {
    pub max_group_members: u64,
    pub max_installations_per_inbox: u64,
    /// Absent means the client keeps its compiled default. `false` is distinct
    /// from absent, so an operator can switch the commit log off explicitly.
    pub commit_log_enabled: Option<bool>,
}

impl From<&xmtp_configuration::MlsConfiguration> for FfiMlsConfiguration {
    fn from(mls: &xmtp_configuration::MlsConfiguration) -> Self {
        Self {
            max_group_members: widen(mls.max_group_members),
            max_installations_per_inbox: widen(mls.max_installations_per_inbox),
            commit_log_enabled: mls.commit_log_enabled,
        }
    }
}

/// One immutable snapshot of what a deployment published.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct FfiServerConfiguration {
    /// Stable operator-chosen name. Empty only before a first fetch succeeds.
    pub identifier: String,
    pub server_version: String,
    /// Empty when the operator published no minimum.
    pub min_libxmtp_version: String,
    pub auth: FfiAuthConfiguration,
    pub retention: FfiRetentionConfiguration,
    pub limits: FfiLimitsConfiguration,
    pub mls: FfiMlsConfiguration,
    /// CAIP-2 chain ids this deployment verifies smart contract wallet
    /// signatures on. Empty rejects every app-supplied signature.
    pub smart_contract_wallet_chains: Vec<String>,
}

// implements: CONF-061
impl From<&xmtp_configuration::ServerConfiguration> for FfiServerConfiguration {
    fn from(configuration: &xmtp_configuration::ServerConfiguration) -> Self {
        Self {
            identifier: configuration.identifier.clone(),
            server_version: configuration.server_version.clone(),
            min_libxmtp_version: configuration.min_libxmtp_version.clone(),
            auth: (&configuration.auth).into(),
            retention: (&configuration.retention).into(),
            limits: (&configuration.limits).into(),
            mls: (&configuration.mls).into(),
            smart_contract_wallet_chains: configuration.smart_contract_wallet_chains.clone(),
        }
    }
}

/// Widen a published count for the foreign side (§7).
///
/// Validation keeps every published value at or below
/// `xmtp_configuration::MAX_PUBLISHED_VALUE`, so a 64-bit target never
/// loses a bit here and no target this binding builds for has a `usize` wider
/// than 64 bits.
fn widen(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

/// Read a deployment's configuration with no database, no client, and no
/// credential.
///
/// An app calls this before it decides how to build a client, so it can learn
/// whether the deployment requires authentication, which scopes it wants, and
/// which chains it accepts. Nothing is stored and no identifier binding is
/// applied: there is no database to bind to.
///
/// `backend_url` and `app_version` are the transport arguments
/// [`crate::connect_to_backend`] takes. No auth callback and no auth handle is
/// attached, so the built client carries no auth middleware at all.
#[uniffi::export(async_runtime = "tokio")]
#[xmtp_common::err_span]
pub async fn fetch_server_configuration(
    backend_url: String,
    app_version: Option<String>,
) -> Result<FfiServerConfiguration, FfiError> {
    init_logger();
    // See `connect_to_backend` — install the rustls provider before an HTTP
    // client is built. Idempotent.
    xmtp_cryptography::install_crypto_provider();
    let api_client = MessageBackendBuilder::default()
        .host(&backend_url)
        .app_version(app_version.unwrap_or_default())
        .build()?;
    let api = ApiClientWrapper::new(api_client, strategies::exponential_cooldown());
    let configuration = xmtp_mls::server_configuration::fetch_server_configuration(&api).await?;
    Ok((&configuration).into())
}
