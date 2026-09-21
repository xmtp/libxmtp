//! What one backend deployment publishes about itself, as JavaScript objects
//! (spec 006 §7).
//!
//! `Client#serverConfiguration()` reads the snapshot this client resolved at
//! build. `fetchServerConfiguration` reads a deployment with no
//! database, no client, and no credential.
//! `Client#refreshServerConfiguration()` fetches now and rewrites the stored
//! copy.
//!
//! Every failure of §7 reaches JavaScript through `ErrorWrapper`, so
//! `error.message` is `[ClientError::<Variant>] <message>`. The six configuration
//! codes are distinct strings:
//!
//! - `[ClientError::ConfigurationUnavailable]`
//! - `[ClientError::ConfigurationInvalid]`
//! - `[ClientError::BackendMismatch]`
//! - `[ClientError::ClientVersionTooOld]`
//! - `[ClientError::AuthRequired]`
//! - `[ClientError::ChainNotAccepted]`

use crate::ErrorWrapper;
use crate::client::Client;
use crate::client::backend::BackendBuilderError;
use napi::bindgen_prelude::Result;
use napi_derive::napi;
use xmtp_api::{ApiClientWrapper, strategies};
use xmtp_api_backend::MessageBackendBuilder;

/// A `usize` limit published as `uint32` on the wire (§5.2). Every value the
/// backend can publish fits; a hand-built snapshot that does not saturates
/// rather than wrapping.
fn as_u32(value: usize) -> u32 {
  u32::try_from(value).unwrap_or(u32::MAX)
}

/// A `uint64` published value (§5.2). JavaScript reads it as a `number`
/// because every published value is below 2^53 (§7).
fn as_number<T: TryInto<i64>>(value: T) -> i64 {
  value.try_into().unwrap_or(i64::MAX)
}

/// Public identity of one accepted signing key. Never the key itself.
#[napi(object)]
pub struct SigningKeyDescription {
  pub kid: String,
  pub alg: String,
}

impl From<&xmtp_configuration::SigningKeyDescription> for SigningKeyDescription {
  fn from(key: &xmtp_configuration::SigningKeyDescription) -> Self {
    Self {
      kid: key.kid.clone(),
      alg: key.alg.clone(),
    }
  }
}

/// What a client must present to be admitted. A client acts only on `enabled`
/// and `requiredScopes`; the rest exists for operator tooling.
#[napi(object)]
pub struct AuthConfiguration {
  pub enabled: bool,
  pub keys: Vec<SigningKeyDescription>,
  pub audiences: Vec<String>,
  pub issuers: Vec<String>,
  pub required_scopes: Vec<String>,
}

impl From<&xmtp_configuration::AuthConfiguration> for AuthConfiguration {
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
#[napi(object)]
pub struct RetentionConfiguration {
  pub group_message_seconds: i64,
  pub welcome_seconds: i64,
  pub key_package_seconds: i64,
}

impl From<&xmtp_configuration::RetentionConfiguration> for RetentionConfiguration {
  fn from(retention: &xmtp_configuration::RetentionConfiguration) -> Self {
    Self {
      group_message_seconds: as_number(retention.group_message_seconds),
      welcome_seconds: as_number(retention.welcome_seconds),
      key_package_seconds: as_number(retention.key_package_seconds),
    }
  }
}

/// Request shapes the deployment accepts. A client chunks its work to these
/// values rather than to its compiled constants.
#[napi(object)]
pub struct LimitsConfiguration {
  pub max_envelope_bytes: i64,
  pub max_request_bytes: i64,
  pub max_response_bytes: i64,
  pub max_publish_topics: u32,
  pub max_query_topics: u32,
  pub max_query_limit: u32,
  pub default_query_limit: u32,
  pub max_newest_metadata_topics: u32,
  pub max_newest_full_topics: u32,
  pub max_update_adds: u32,
  pub max_update_removes: u32,
  pub max_stream_topics: u32,
  pub max_static_topics: u32,
  pub max_lookup_identifiers: u32,
  pub max_scw_signatures: u32,
  pub max_identity_entries: u32,
  pub max_update_frames_per_second: u32,
  pub max_update_burst: u32,
  pub max_ping_frames_per_second: u32,
  pub max_ping_burst: u32,
}

impl From<&xmtp_configuration::LimitsConfiguration> for LimitsConfiguration {
  fn from(limits: &xmtp_configuration::LimitsConfiguration) -> Self {
    Self {
      max_envelope_bytes: as_number(limits.max_envelope_bytes),
      max_request_bytes: as_number(limits.max_request_bytes),
      max_response_bytes: as_number(limits.max_response_bytes),
      max_publish_topics: as_u32(limits.max_publish_topics),
      max_query_topics: as_u32(limits.max_query_topics),
      max_query_limit: as_u32(limits.max_query_limit),
      default_query_limit: as_u32(limits.default_query_limit),
      max_newest_metadata_topics: as_u32(limits.max_newest_metadata_topics),
      max_newest_full_topics: as_u32(limits.max_newest_full_topics),
      max_update_adds: as_u32(limits.max_update_adds),
      max_update_removes: as_u32(limits.max_update_removes),
      max_stream_topics: as_u32(limits.max_stream_topics),
      max_static_topics: as_u32(limits.max_static_topics),
      max_lookup_identifiers: as_u32(limits.max_lookup_identifiers),
      max_scw_signatures: as_u32(limits.max_scw_signatures),
      max_identity_entries: as_u32(limits.max_identity_entries),
      max_update_frames_per_second: limits.max_update_frames_per_second,
      max_update_burst: limits.max_update_burst,
      max_ping_frames_per_second: limits.max_ping_frames_per_second,
      max_ping_burst: limits.max_ping_burst,
    }
  }
}

/// Advisory group shapes. The backend publishes them and does not enforce them.
#[napi(object)]
pub struct MlsConfiguration {
  pub max_group_members: u32,
  pub max_installations_per_inbox: u32,
  /// `null` means the client keeps its compiled default. `false` is distinct
  /// from absent, so an operator can switch the commit log off explicitly.
  pub commit_log_enabled: Option<bool>,
}

impl From<&xmtp_configuration::MlsConfiguration> for MlsConfiguration {
  fn from(mls: &xmtp_configuration::MlsConfiguration) -> Self {
    Self {
      max_group_members: as_u32(mls.max_group_members),
      max_installations_per_inbox: as_u32(mls.max_installations_per_inbox),
      commit_log_enabled: mls.commit_log_enabled,
    }
  }
}

/// One immutable snapshot of what a deployment published.
#[napi(object)]
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

// implements: CONF-061
impl From<&xmtp_configuration::ServerConfiguration> for ServerConfiguration {
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

#[napi]
impl Client {
  /// What this deployment published about itself, as resolved at build.
  ///
  /// A refresh rewrites the stored copy; it never changes this value. A new
  /// value takes effect at the next client build.
  #[napi]
  pub fn server_configuration(&self) -> ServerConfiguration {
    self.inner_client().server_configuration().into()
  }

  /// Fetch the deployment configuration now, rewrite the stored copy, and
  /// return what was fetched.
  ///
  /// Applies the same validation, storage, and identifier
  /// binding the hourly refresh worker applies. The snapshot this
  /// client is holding is unchanged.
  ///
  /// Rejects with `[ClientError::ConfigurationUnavailable]`,
  /// `[ClientError::ConfigurationInvalid]`, `[ClientError::BackendMismatch]`,
  /// or `[ClientError::ClientVersionTooOld]`.
  #[napi]
  #[xmtp_common::err_span]
  pub async fn refresh_server_configuration(&self) -> Result<ServerConfiguration> {
    let configuration = self
      .inner_client()
      .refresh_server_configuration()
      .await
      .map_err(ErrorWrapper::from)?;
    Ok((&configuration).into())
  }
}

/// Read what a deployment publishes about itself with no database, no client,
/// and no credential.
///
/// Lets an app learn `auth.enabled`, `auth.requiredScopes`, and the accepted
/// smart contract wallet chains before it decides how to build a client.
/// Nothing is stored and no identifier binding is applied: there is no
/// database to bind to.
///
/// Rejects with `[ClientError::ConfigurationUnavailable]` when the deployment
/// does not answer — a backend older than spec 006 answers `UNIMPLEMENTED` —
/// and with `[ClientError::ConfigurationInvalid]` when it publishes a
/// configuration no client can use.
#[napi]
#[xmtp_common::err_span]
pub async fn fetch_server_configuration(
  host: String,
  app_version: Option<String>,
) -> Result<ServerConfiguration> {
  // Same reason as `BackendBuilder::build`: the `#[ctor::ctor(unsafe)]` in
  // `xmtp_cryptography` does not fire on every platform, and this is the first
  // thing an app calls. Idempotent. See issue #3846.
  xmtp_cryptography::install_crypto_provider();

  let mut builder = MessageBackendBuilder::default();
  builder
    .host(&host)
    .app_version(app_version.unwrap_or_default());

  let api_client = builder
    .build()
    .map_err(BackendBuilderError)
    .map_err(ErrorWrapper::from)?;
  let api = ApiClientWrapper::new(api_client, strategies::exponential_cooldown());

  let configuration = xmtp_mls::server_configuration::fetch_server_configuration(&api)
    .await
    .map_err(ErrorWrapper::from)?;

  Ok((&configuration).into())
}

#[cfg(test)]
mod tests {
  use crate::ErrorWrapper;
  use std::collections::BTreeSet;
  use xmtp_mls::client::ClientError;
  use xmtp_mls::server_configuration::ConfigurationFetchError;

  // verifies: CONF-064
  #[xmtp_common::test(unwrap_try = true)]
  fn configuration_codes_reach_node_errors() {
    let cases = [
      (
        "ClientError::ConfigurationUnavailable",
        ClientError::ConfigurationUnavailable(Box::new(ConfigurationFetchError::Api(
          xmtp_api::ApiError::EnvelopeTooLarge,
        ))),
      ),
      (
        "ClientError::ConfigurationInvalid",
        ClientError::ConfigurationInvalid(xmtp_configuration::ServerConfigurationError::Identifier),
      ),
      (
        "ClientError::BackendMismatch",
        ClientError::BackendMismatch {
          stored: "org.xmtp.one".to_owned(),
          received: "org.xmtp.two".to_owned(),
        },
      ),
      (
        "ClientError::ClientVersionTooOld",
        ClientError::ClientVersionTooOld {
          client: "1.0.0".to_owned(),
          minimum: "2.0.0".to_owned(),
        },
      ),
      (
        "ClientError::AuthRequired",
        ClientError::AuthRequired {
          required_scopes: vec!["messages:write".to_owned()],
        },
      ),
      (
        "ClientError::ChainNotAccepted",
        ClientError::ChainNotAccepted {
          chain: "eip155:1".to_owned(),
          accepted: vec!["eip155:8453".to_owned()],
        },
      ),
    ];

    let mut codes = BTreeSet::new();
    for (code, error) in cases {
      let message = format!("{error}");
      let napi_error: napi::Error = ErrorWrapper::from(error).into();
      assert_eq!(napi_error.reason, format!("[{code}] {message}"));
      assert!(codes.insert(code), "{code} is not distinct");
    }
    assert_eq!(codes.len(), 6);
  }
}
