use crate::ErrorWrapper;
use crate::conversations::Conversations;
use crate::enriched_message::DecodedMessage;
use crate::identity::{ApiStats, Identifier, IdentityExt, IdentityStats};
use crate::inbox_state::InboxState;
use crate::signatures::SignatureRequestHandle;
use napi::bindgen_prelude::{Error, Result, Uint8Array};
use napi_derive::napi;
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};
use xmtp_api_d14n::MessageBackendBuilder;
use xmtp_db::{EncryptedMessageStore, EncryptionKey, NativeDb, StorageOption};
use xmtp_mls::Client as MlsClient;
use xmtp_mls::builder::SyncWorkerMode as XmtpSyncWorkerMode;
use xmtp_mls::cursor_store::SqliteCursorStore;
use xmtp_mls::groups::MlsGroup;
use xmtp_mls::identity::IdentityStrategy;
use xmtp_mls::utils::events::upload_debug_archive;
use xmtp_proto::api_client::AggregateStats;

pub type RustXmtpClient = MlsClient<xmtp_mls::MlsContext>;
pub type RustMlsGroup = MlsGroup<xmtp_mls::MlsContext>;
static LOGGER_INIT: std::sync::OnceLock<Result<()>> = std::sync::OnceLock::new();

#[napi]
#[derive(Clone)]
pub struct Client {
  inner_client: Arc<RustXmtpClient>,
  account_identifier: Identifier,
  app_version: Option<String>,
}

#[napi]
impl Client {
  pub fn inner_client(&self) -> &Arc<RustXmtpClient> {
    &self.inner_client
  }
  #[napi(getter)]
  pub fn account_identifier(&self) -> Identifier {
    self.account_identifier.clone()
  }
}

#[napi(string_enum)]
#[derive(Debug)]
#[allow(non_camel_case_types)]
pub enum LogLevel {
  off,
  error,
  warn,
  info,
  debug,
  trace,
}

#[napi(string_enum)]
#[derive(Debug)]
#[allow(non_camel_case_types)]
pub enum SyncWorkerMode {
  enabled,
  disabled,
}

impl From<SyncWorkerMode> for XmtpSyncWorkerMode {
  fn from(value: SyncWorkerMode) -> Self {
    match value {
      SyncWorkerMode::enabled => Self::Enabled,
      SyncWorkerMode::disabled => Self::Disabled,
    }
  }
}

impl std::fmt::Display for LogLevel {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    use LogLevel::*;
    let s = match self {
      off => "off",
      error => "error",
      warn => "warn",
      info => "info",
      debug => "debug",
      trace => "trace",
    };
    write!(f, "{}", s)
  }
}

/// Specify options for the logger
#[napi(object)]
#[derive(Default)]
pub struct LogOptions {
  /// enable structured JSON logging to stdout.Useful for third-party log viewers
  /// an option so that it does not require being specified in js object.
  pub structured: Option<bool>,
  /// Filter logs by level
  pub level: Option<LogLevel>,
}

#[napi(object)]
pub struct GroupSyncSummary {
  pub num_eligible: u32,
  pub num_synced: u32,
}

impl From<xmtp_mls::groups::welcome_sync::GroupSyncSummary> for GroupSyncSummary {
  fn from(summary: xmtp_mls::groups::welcome_sync::GroupSyncSummary) -> Self {
    Self {
      num_eligible: summary.num_eligible as u32,
      num_synced: summary.num_synced as u32,
    }
  }
}

fn init_logging(options: LogOptions) -> Result<()> {
  LOGGER_INIT
    .get_or_init(|| {
      let filter = if let Some(f) = options.level {
        xmtp_common::filter_directive(&f.to_string())
      } else {
        EnvFilter::builder().parse_lossy("info")
      };
      if options.structured.unwrap_or_default() {
        let fmt = tracing_subscriber::fmt::layer()
          .json()
          .flatten_event(true)
          .with_level(true)
          .with_target(true);

        tracing_subscriber::registry().with(filter).with(fmt).init();
      } else {
        tracing_subscriber::registry()
          .with(fmt::layer())
          .with(filter)
          .init();
      }
      Ok(())
    })
    .as_ref()
    .map_err(ErrorWrapper::from)?;
  Ok(())
}

/**
 * Create a client.
 *
 * Optionally specify a filter for the log level as a string.
 * It can be one of: `debug`, `info`, `warn`, `error` or 'off'.
 * By default, logging is disabled.
 */
#[allow(clippy::too_many_arguments)]
#[napi]
pub async fn create_client(
  v3_host: String,
  gateway_host: Option<String>,
  is_secure: bool,
  db_path: Option<String>,
  inbox_id: String,
  account_identifier: Identifier,
  encryption_key: Option<Uint8Array>,
  device_sync_server_url: Option<String>,
  device_sync_worker_mode: Option<SyncWorkerMode>,
  log_options: Option<LogOptions>,
  allow_offline: Option<bool>,
  disable_events: Option<bool>,
  app_version: Option<String>,
) -> Result<Client> {
  let root_identifier = account_identifier.clone();
  init_logging(log_options.unwrap_or_default())?;
  let mut backend = MessageBackendBuilder::default();
  backend
    .v3_host(&v3_host)
    .maybe_gateway_host(gateway_host)
    .app_version(app_version.clone().unwrap_or_default())
    .is_secure(is_secure);

  let storage_option = match db_path {
    Some(path) => StorageOption::Persistent(path),
    None => StorageOption::Ephemeral,
  };

  let store = match encryption_key {
    Some(key) => {
      let key: Vec<u8> = key.deref().into();
      let key: EncryptionKey = key
        .try_into()
        .map_err(|_| Error::from_reason("Malformed 32 byte encryption key"))?;
      let db = NativeDb::new(&storage_option, key)
        .map_err(|e| Error::from_reason(format!("Error creating native database {}", e)))?;
      EncryptedMessageStore::new(db)
        .map_err(|e| Error::from_reason(format!("Error Creating Encrypted Message store {}", e)))?
    }
    None => {
      let db = NativeDb::new_unencrypted(&storage_option)
        .map_err(|e| Error::from_reason(e.to_string()))?;
      EncryptedMessageStore::new(db).map_err(|e| Error::from_reason(e.to_string()))?
    }
  };

  let internal_account_identifier = account_identifier.clone().try_into()?;
  let identity_strategy = IdentityStrategy::new(
    inbox_id.clone(),
    internal_account_identifier,
    // this is a temporary solution
    1,
    None,
  );

  let cursor_store = SqliteCursorStore::new(store.db());
  backend.cursor_store(cursor_store);
  let api_client = backend.clone().build().map_err(ErrorWrapper::from)?;
  let sync_api_client = backend.clone().build().map_err(ErrorWrapper::from)?;

  let mut builder = xmtp_mls::Client::builder(identity_strategy)
    .api_clients(api_client, sync_api_client)
    .enable_api_stats()
    .map_err(ErrorWrapper::from)?
    .enable_api_debug_wrapper()
    .map_err(ErrorWrapper::from)?
    .with_remote_verifier()
    .map_err(ErrorWrapper::from)?
    .with_allow_offline(allow_offline)
    .with_disable_events(disable_events)
    .store(store);

  if let Some(u) = device_sync_server_url {
    builder = builder.device_sync_server_url(&u);
  };

  if let Some(device_sync_worker_mode) = device_sync_worker_mode {
    builder = builder.device_sync_worker_mode(device_sync_worker_mode.into());
  };

  let xmtp_client = builder
    .default_mls_store()
    .map_err(ErrorWrapper::from)?
    .build()
    .await
    .map_err(ErrorWrapper::from)?;

  Ok(Client {
    inner_client: Arc::new(xmtp_client),
    account_identifier: root_identifier,
    app_version,
  })
}

#[napi]
impl Client {
  #[napi]
  pub fn inbox_id(&self) -> String {
    self.inner_client.inbox_id().to_string()
  }

  #[napi]
  pub fn is_registered(&self) -> bool {
    self.inner_client.identity().is_ready()
  }

  #[napi]
  pub fn installation_id(&self) -> String {
    hex::encode(self.inner_client.installation_public_key())
  }

  #[napi]
  pub fn installation_id_bytes(&self) -> Uint8Array {
    self.inner_client.installation_public_key().into()
  }

  #[napi]
  pub fn app_version(&self) -> String {
    self.app_version.clone().unwrap_or_default()
  }

  #[napi]
  pub fn libxmtp_version(&self) -> String {
    env!("CARGO_PKG_VERSION").to_string()
  }

  #[napi]
  /// The resulting vec will be the same length as the input and should be zipped for the results.
  pub async fn can_message(
    &self,
    account_identities: Vec<Identifier>,
  ) -> Result<HashMap<String, bool>> {
    let ident = account_identities.to_internal()?;
    let results = self
      .inner_client
      .can_message(&ident)
      .await
      .map_err(ErrorWrapper::from)?;

    let results = results
      .into_iter()
      .map(|(k, v)| (format!("{k}"), v))
      .collect();

    Ok(results)
  }

  #[napi]
  pub async fn register_identity(&self, signature_request: &SignatureRequestHandle) -> Result<()> {
    if self.is_registered() {
      return Err(Error::from_reason(
        "An identity is already registered with this client",
      ));
    }

    let inner = signature_request.inner().lock().await;

    self
      .inner_client
      .register_identity(inner.clone())
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }

  #[napi]
  pub fn conversations(&self) -> Conversations {
    Conversations::new(self.inner_client.clone())
  }

  #[napi]
  pub async fn send_sync_request(&self) -> Result<()> {
    self
      .inner_client
      .device_sync_client()
      .send_sync_request()
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }

  #[napi]
  pub async fn find_inbox_id_by_identifier(
    &self,
    identifier: Identifier,
  ) -> Result<Option<String>> {
    let conn = self.inner_client().context.store().db();

    let inbox_id = self
      .inner_client
      .find_inbox_id_from_identifier(&conn, identifier.try_into()?)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(inbox_id)
  }

  #[napi]
  pub async fn addresses_from_inbox_id(
    &self,
    refresh_from_network: bool,
    inbox_ids: Vec<String>,
  ) -> Result<Vec<InboxState>> {
    let state = self
      .inner_client
      .inbox_addresses(
        refresh_from_network,
        inbox_ids.iter().map(String::as_str).collect(),
      )
      .await
      .map_err(ErrorWrapper::from)?;
    Ok(state.into_iter().map(Into::into).collect())
  }

  #[napi]
  pub async fn sync_preferences(&self) -> Result<GroupSyncSummary> {
    let inner = self.inner_client.as_ref();

    let summary = inner
      .sync_all_welcomes_and_history_sync_groups()
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(summary.into())
  }

  #[napi]
  pub fn api_statistics(&self) -> ApiStats {
    self.inner_client.api_stats().into()
  }

  #[napi]
  pub fn api_identity_statistics(&self) -> IdentityStats {
    self.inner_client.identity_api_stats().into()
  }

  #[napi]
  pub fn api_aggregate_statistics(&self) -> String {
    let api = self.inner_client.api_stats();
    let identity = self.inner_client.identity_api_stats();
    let aggregate = AggregateStats { mls: api, identity };
    format!("{:?}", aggregate)
  }

  #[napi]
  pub fn clear_all_statistics(&self) {
    self.inner_client.clear_stats()
  }

  #[napi]
  pub async fn upload_debug_archive(&self, server_url: String) -> Result<String> {
    let db = self.inner_client().context.db();
    Ok(
      upload_debug_archive(db, Some(server_url))
        .await
        .map_err(ErrorWrapper::from)?,
    )
  }

  #[napi]
  pub fn delete_message(&self, message_id: Uint8Array) -> Result<u32> {
    let deleted_count = self
      .inner_client
      .delete_message(message_id.to_vec())
      .map_err(ErrorWrapper::from)?;
    Ok(deleted_count as u32)
  }

  #[napi]
  pub async fn enriched_message(&self, message_id: Vec<u8>) -> Result<DecodedMessage> {
    let message = self
      .inner_client
      .message_v2(message_id)
      .map_err(ErrorWrapper::from)?;

    Ok(message.into())
  }
}
