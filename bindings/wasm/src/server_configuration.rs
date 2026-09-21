//! What one backend deployment publishes about itself, as the browser sees it.
//!
//! An app reads the snapshot its client resolved at build,
//! asks for a fresh copy, or learns what a deployment
//! wants before it has a client at all.
//!
//! Every numeric field is a JavaScript `number`. §7 requires that: the wire
//! carries `uint32` and `uint64`, every published value is below 2^53, and an
//! app must be able to compare a limit with `<` without a `BigInt` conversion.
//! The core types hold `usize` and `u64`, and `errors::to_value` serializes
//! those as `BigInt`, so each mirror field below is an `f64` instead. `f64`
//! holds every published value exactly, needs no fallible cast from `usize`,
//! and `tsify` types it as `number`.
//!
//! "Every published value is below 2^53" is not an assumption here: a value
//! above `xmtp_configuration::MAX_PUBLISHED_VALUE` fails validation before a
//! snapshot is ever built, so no `as f64` below can round.

use crate::ErrorWrapper;
use crate::client::Client;
use crate::client::backend::BackendBuilderError;
use serde::{Deserialize, Serialize};
use tsify::Tsify;
use wasm_bindgen::{JsError, prelude::wasm_bindgen};
use xmtp_api::{ApiClientWrapper, strategies};
use xmtp_api_backend::MessageBackendBuilder;

/// The public identity of one signing key the deployment accepts. Never the
/// key itself.
#[derive(Clone, Debug, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
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

/// What a client must present to be admitted. An app acts on `enabled` and
/// `requiredScopes`; the rest is for operator tooling.
#[derive(Clone, Debug, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
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
#[derive(Clone, Debug, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct RetentionConfiguration {
  pub group_message_seconds: f64,
  pub welcome_seconds: f64,
  pub key_package_seconds: f64,
}

impl From<&xmtp_configuration::RetentionConfiguration> for RetentionConfiguration {
  fn from(retention: &xmtp_configuration::RetentionConfiguration) -> Self {
    Self {
      group_message_seconds: retention.group_message_seconds as f64,
      welcome_seconds: retention.welcome_seconds as f64,
      key_package_seconds: retention.key_package_seconds as f64,
    }
  }
}

/// The request shapes the deployment accepts. A client chunks its work to
/// these values rather than to its compiled constants.
#[derive(Clone, Debug, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct LimitsConfiguration {
  pub max_envelope_bytes: f64,
  pub max_request_bytes: f64,
  pub max_response_bytes: f64,
  pub max_publish_topics: f64,
  pub max_query_topics: f64,
  pub max_query_limit: f64,
  pub default_query_limit: f64,
  pub max_newest_metadata_topics: f64,
  pub max_newest_full_topics: f64,
  pub max_update_adds: f64,
  pub max_update_removes: f64,
  pub max_stream_topics: f64,
  pub max_static_topics: f64,
  pub max_lookup_identifiers: f64,
  pub max_scw_signatures: f64,
  pub max_identity_entries: f64,
  pub max_update_frames_per_second: f64,
  pub max_update_burst: f64,
  pub max_ping_frames_per_second: f64,
  pub max_ping_burst: f64,
}

impl From<&xmtp_configuration::LimitsConfiguration> for LimitsConfiguration {
  fn from(limits: &xmtp_configuration::LimitsConfiguration) -> Self {
    Self {
      max_envelope_bytes: limits.max_envelope_bytes as f64,
      max_request_bytes: limits.max_request_bytes as f64,
      max_response_bytes: limits.max_response_bytes as f64,
      max_publish_topics: limits.max_publish_topics as f64,
      max_query_topics: limits.max_query_topics as f64,
      max_query_limit: limits.max_query_limit as f64,
      default_query_limit: limits.default_query_limit as f64,
      max_newest_metadata_topics: limits.max_newest_metadata_topics as f64,
      max_newest_full_topics: limits.max_newest_full_topics as f64,
      max_update_adds: limits.max_update_adds as f64,
      max_update_removes: limits.max_update_removes as f64,
      max_stream_topics: limits.max_stream_topics as f64,
      max_static_topics: limits.max_static_topics as f64,
      max_lookup_identifiers: limits.max_lookup_identifiers as f64,
      max_scw_signatures: limits.max_scw_signatures as f64,
      max_identity_entries: limits.max_identity_entries as f64,
      max_update_frames_per_second: limits.max_update_frames_per_second as f64,
      max_update_burst: limits.max_update_burst as f64,
      max_ping_frames_per_second: limits.max_ping_frames_per_second as f64,
      max_ping_burst: limits.max_ping_burst as f64,
    }
  }
}

/// Advisory group shapes. The deployment publishes them and does not enforce
/// them; the client does.
#[derive(Clone, Debug, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct MlsConfiguration {
  pub max_group_members: f64,
  pub max_installations_per_inbox: f64,
  /// Absent — the property is omitted, so JavaScript reads `undefined` — means
  /// the deployment published nothing and the client keeps its compiled
  /// default. `false` is distinct from absent. The TypeScript type is
  /// `commitLogEnabled?: boolean`, which says exactly that.
  #[tsify(optional)]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub commit_log_enabled: Option<bool>,
}

impl From<&xmtp_configuration::MlsConfiguration> for MlsConfiguration {
  fn from(mls: &xmtp_configuration::MlsConfiguration) -> Self {
    Self {
      max_group_members: mls.max_group_members as f64,
      max_installations_per_inbox: mls.max_installations_per_inbox as f64,
      commit_log_enabled: mls.commit_log_enabled,
    }
  }
}

/// One immutable snapshot of what a deployment published (§5.2).
#[derive(Clone, Debug, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct ServerConfiguration {
  /// The stable operator-chosen name this database is bound to. Empty only
  /// before a first fetch has succeeded.
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

/// Read a deployment's configuration with no database, no client, and no
/// credential.
///
/// The transport arguments are the ones every other entry point takes: the
/// backend URL and the optional app version sent as `x-app-version`. No auth
/// callback or handle is accepted, because this RPC is served without one.
#[wasm_bindgen(js_name = fetchServerConfiguration)]
pub async fn fetch_server_configuration(
  host: String,
  #[wasm_bindgen(js_name = appVersion)] app_version: Option<String>,
) -> Result<ServerConfiguration, JsError> {
  let mut backend = MessageBackendBuilder::default();
  backend
    .host(&host)
    .app_version(app_version.unwrap_or_default());
  let api_client = backend
    .build()
    .map_err(BackendBuilderError)
    .map_err(ErrorWrapper::js)?;
  let api = ApiClientWrapper::new(api_client, strategies::exponential_cooldown());

  let configuration = xmtp_mls::server_configuration::fetch_server_configuration(&api)
    .await
    .map_err(ErrorWrapper::js)?;
  Ok((&configuration).into())
}

#[wasm_bindgen]
impl Client {
  /// What this deployment published, as resolved when this client was built.
  /// A refresh rewrites the stored copy; it never changes
  /// this value.
  #[wasm_bindgen(js_name = serverConfiguration)]
  pub fn server_configuration(&self) -> ServerConfiguration {
    self.inner_client().server_configuration().into()
  }

  /// Fetch the deployment configuration now and rewrite the stored copy.
  ///
  /// Applies the same validation, storage, and identifier binding the refresh
  /// worker applies. The snapshot this client holds is unchanged; a new value
  /// takes effect at the next build.
  #[wasm_bindgen(js_name = refreshServerConfiguration)]
  pub async fn refresh_server_configuration(&self) -> Result<ServerConfiguration, JsError> {
    let configuration = self
      .inner_client()
      .refresh_server_configuration()
      .await
      .map_err(ErrorWrapper::js)?;
    Ok((&configuration).into())
  }
}

#[cfg(all(test, target_arch = "wasm32"))]
mod tests {
  use super::*;
  use crate::tests::create_test_client;
  use xmtp_configuration::backend_test_url;

  // verifies: CONF-061, CONF-074
  #[xmtp_common::test(unwrap_try = true)]
  async fn server_configuration_exposes_every_field() {
    let client = create_test_client(None).await;
    let configuration = client.server_configuration();

    assert!(!configuration.identifier.is_empty());
    let _: &String = &configuration.server_version;
    let _: &String = &configuration.min_libxmtp_version;

    let auth = &configuration.auth;
    let _: bool = auth.enabled;
    for key in &auth.keys {
      let _: (&String, &String) = (&key.kid, &key.alg);
    }
    let _: &Vec<String> = &auth.audiences;
    let _: &Vec<String> = &auth.issuers;
    let _: &Vec<String> = &auth.required_scopes;

    let retention = &configuration.retention;
    assert!(retention.group_message_seconds > 0.0);
    assert!(retention.welcome_seconds > 0.0);
    assert!(retention.key_package_seconds > 0.0);

    let limits = &configuration.limits;
    for limit in [
      limits.max_envelope_bytes,
      limits.max_request_bytes,
      limits.max_response_bytes,
      limits.max_publish_topics,
      limits.max_query_topics,
      limits.max_query_limit,
      limits.default_query_limit,
      limits.max_newest_metadata_topics,
      limits.max_newest_full_topics,
      limits.max_update_adds,
      limits.max_update_removes,
      limits.max_stream_topics,
      limits.max_static_topics,
      limits.max_lookup_identifiers,
      limits.max_scw_signatures,
      limits.max_identity_entries,
      limits.max_update_frames_per_second,
      limits.max_update_burst,
      limits.max_ping_frames_per_second,
      limits.max_ping_burst,
    ] {
      // §7: every published value is a JavaScript number no larger than
      // `MAX_PUBLISHED_VALUE`, which validation enforces, so the
      // `f64` an app reads is the value the deployment published.
      assert!(limit > 0.0);
      assert!(limit <= xmtp_configuration::MAX_PUBLISHED_VALUE as f64);
    }

    let mls = &configuration.mls;
    assert!(mls.max_group_members > 0.0);
    assert!(mls.max_installations_per_inbox > 0.0);
    let _: Option<bool> = mls.commit_log_enabled;

    let _: &Vec<String> = &configuration.smart_contract_wallet_chains;

    // An explicit refresh answers with the same deployment.
    let refreshed = client.refresh_server_configuration().await?;
    assert_eq!(refreshed.identifier, configuration.identifier);

    client.close().await?;
  }

  // verifies: CONF-062
  #[xmtp_common::test(unwrap_try = true)]
  async fn fetch_server_configuration_needs_no_client() {
    let configuration = fetch_server_configuration(backend_test_url(), None).await?;
    assert!(!configuration.identifier.is_empty());
    assert!(configuration.limits.max_envelope_bytes > 0.0);
  }
}
