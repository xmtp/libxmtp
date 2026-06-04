use crate::ErrorWrapper;
use crate::client::Client;
use crate::client::backend::Backend;
use crate::client::gateway_auth::{AuthCallback, AuthHandle};
use crate::client::options::{
  ClientMode, LogLevel, LogOptions, SyncWorkerMode, WorkerConfigOptions,
};
use crate::identity::Identifier;
use napi::bindgen_prelude::{BigInt, Error, Result, Uint8Array};
use napi_derive::napi;
use std::ops::Deref;
use std::sync::Arc;
use xmtp_api_d14n::MessageBackendBuilder;
use xmtp_configuration::{MAX_DB_POOL_SIZE, MIN_DB_POOL_SIZE};
use xmtp_db::{EncryptedMessageStore, EncryptionKey, NativeDb};
use xmtp_logging::{Level, LoggingConfig, TelemetryConfig, XmtpLoggingBuilder};
use xmtp_mls::XmtpApiClient;
use xmtp_mls::cursor_store::SqliteCursorStore;
use xmtp_mls::identity::IdentityStrategy;

// Holds the global logging handle for the process so `flush_telemetry` can flush
// spans before exit. Installed exactly once on first `create_client` call.
static LOGGING_HANDLE: std::sync::OnceLock<xmtp_logging::LoggingHandle> =
  std::sync::OnceLock::new();

fn map_level(l: &LogLevel) -> Level {
  match l {
    LogLevel::Off => Level::Off,
    LogLevel::Error => Level::Error,
    LogLevel::Warn => Level::Warn,
    LogLevel::Info => Level::Info,
    LogLevel::Debug => Level::Debug,
    LogLevel::Trace => Level::Trace,
  }
}

fn init_logging(options: LogOptions) -> Result<()> {
  // Already installed (by us or another crate) — nothing to do.
  if LOGGING_HANDLE.get().is_some() {
    return Ok(());
  }

  // Preserve the old default of "info" when no level is provided.
  let level = options.level.as_ref().map(map_level).unwrap_or(Level::Info);

  // Only configure telemetry when an endpoint is set, so we don't spawn an
  // exporter that just logs connection errors.
  let telemetry = options.otel_endpoint.clone().map(|endpoint| {
    let resource_attributes: Vec<(String, String)> = options
      .resource_attributes
      .clone()
      .unwrap_or_default()
      .into_iter()
      .collect();
    TelemetryConfig {
      endpoint: Some(endpoint),
      resource_attributes,
    }
  });

  let cfg = LoggingConfig {
    level,
    json: options.structured.unwrap_or_default(),
    file: None,
    telemetry,
    native: false,
    performance: false,
  };

  // `install()` installs a global subscriber and only succeeds once per process.
  match XmtpLoggingBuilder::from_config(cfg).install() {
    Ok(handle) => {
      let _ = LOGGING_HANDLE.set(handle);
      Ok(())
    }
    // Someone else installed first (e.g. another crate) — treat as success.
    Err(xmtp_logging::Error::AlreadyInitialized) => Ok(()),
    Err(e) => Err(Error::from_reason(format!(
      "failed to initialize logging: {e}"
    ))),
  }
}

/// Flush buffered telemetry spans before process exit. Call once on graceful
/// shutdown; no-op if telemetry was never enabled. Process-global (not per-client).
#[napi]
pub fn flush_telemetry() {
  if let Some(h) = LOGGING_HANDLE.get() {
    h.flush();
  }
}

#[napi(object)]
pub struct DbOptions {
  pub db_path: Option<String>,
  pub encryption_key: Option<Uint8Array>,
  pub max_db_pool_size: Option<u32>,
  pub min_db_pool_size: Option<u32>,
}

impl DbOptions {
  pub fn new(
    db_path: Option<String>,
    encryption_key: Option<Uint8Array>,
    max_db_pool_size: Option<u32>,
    min_db_pool_size: Option<u32>,
  ) -> Self {
    Self {
      db_path,
      encryption_key,
      max_db_pool_size,
      min_db_pool_size,
    }
  }
}

fn build_store(db: DbOptions) -> Result<EncryptedMessageStore<NativeDb>> {
  let DbOptions {
    db_path,
    encryption_key,
    max_db_pool_size,
    min_db_pool_size,
  } = db;

  let db = if let Some(path) = db_path {
    NativeDb::builder().persistent(path)
  } else {
    NativeDb::builder().ephemeral()
  };

  let db = if let Some(max_size) = max_db_pool_size {
    db.max_pool_size(max_size)
  } else {
    db.max_pool_size(MAX_DB_POOL_SIZE)
  };

  let db = if let Some(min_size) = min_db_pool_size {
    db.min_pool_size(min_size)
  } else {
    db.min_pool_size(MIN_DB_POOL_SIZE)
  };

  let db = if let Some(key) = encryption_key {
    let key: Vec<u8> = key.deref().into();
    let key: EncryptionKey = key
      .try_into()
      .map_err(|_| Error::from_reason("Malformed 32 byte encryption key"))?;
    db.key(key).build()
  } else {
    db.build_unencrypted()
  }
  .map_err(ErrorWrapper::from)?;

  Ok(EncryptedMessageStore::new(db).map_err(ErrorWrapper::from)?)
}

fn parse_nonce(nonce: Option<BigInt>) -> Result<u64> {
  match nonce {
    Some(n) => {
      let (signed, value, lossless) = n.get_u64();
      if signed {
        return Err(Error::from_reason("`nonce` must be non-negative"));
      }
      if !lossless {
        return Err(Error::from_reason("`nonce` is too large"));
      }
      Ok(value)
    }
    None => Ok(1),
  }
}

#[allow(clippy::too_many_arguments)]
async fn create_client_inner(
  api_client: XmtpApiClient,
  store: EncryptedMessageStore<NativeDb>,
  inbox_id: String,
  account_identifier: Identifier,
  device_sync_worker_mode: Option<SyncWorkerMode>,
  worker_config: Option<WorkerConfigOptions>,
  allow_offline: Option<bool>,
  app_version: Option<String>,
  nonce: u64,
) -> Result<Client> {
  let root_identifier = account_identifier.clone();
  let internal_account_identifier = account_identifier.try_into()?;
  let identity_strategy = IdentityStrategy::new(inbox_id, internal_account_identifier, nonce, None);

  let mut builder = xmtp_mls::Client::builder(identity_strategy)
    .api_client(api_client)
    .enable_api_stats()
    .map_err(ErrorWrapper::from)?
    .with_remote_verifier()
    .map_err(ErrorWrapper::from)?
    .with_allow_offline(allow_offline)
    .store(store);

  if let Some(device_sync_worker_mode) = device_sync_worker_mode {
    builder = builder.device_sync_worker_mode(device_sync_worker_mode.into());
  };

  if let Some(worker_config) = worker_config {
    builder = builder.worker_config(worker_config.into());
  }

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
  db: DbOptions,
  inbox_id: String,
  account_identifier: Identifier,
  device_sync_worker_mode: Option<SyncWorkerMode>,
  worker_config: Option<WorkerConfigOptions>,
  log_options: Option<LogOptions>,
  allow_offline: Option<bool>,
  app_version: Option<String>,
  nonce: Option<BigInt>,
  auth_callback: Option<&AuthCallback>,
  auth_handle: Option<&AuthHandle>,
  client_mode: Option<ClientMode>,
) -> Result<Client> {
  let client_mode = client_mode.unwrap_or_default();
  init_logging(log_options.unwrap_or_default())?;

  let mut backend = MessageBackendBuilder::default();
  backend
    .v3_host(&v3_host)
    .maybe_gateway_host(gateway_host)
    .readonly(matches!(client_mode, ClientMode::Notification))
    .maybe_auth_callback(auth_callback.map(|c| Arc::new(c.clone()) as _))
    .maybe_auth_handle(auth_handle.map(|h| h.clone().into()))
    .app_version(app_version.clone().unwrap_or_default());

  let store = build_store(db)?;
  let nonce = parse_nonce(nonce)?;

  let cursor_store = SqliteCursorStore::new(store.db());
  backend.cursor_store(cursor_store);
  let api_client = backend.build_optional_d14n().map_err(ErrorWrapper::from)?;

  create_client_inner(
    api_client,
    store,
    inbox_id,
    account_identifier,
    device_sync_worker_mode,
    worker_config,
    allow_offline,
    app_version,
    nonce,
  )
  .await
}

/// Create a client from a pre-built Backend.
///
/// The Backend encapsulates all API configuration (env, hosts, auth, TLS).
/// This function only needs identity and database configuration.
#[allow(clippy::too_many_arguments)]
#[napi]
pub async fn create_client_with_backend(
  backend: &Backend,
  db: DbOptions,
  inbox_id: String,
  account_identifier: Identifier,
  device_sync_worker_mode: Option<SyncWorkerMode>,
  worker_config: Option<WorkerConfigOptions>,
  log_options: Option<LogOptions>,
  allow_offline: Option<bool>,
  nonce: Option<BigInt>,
) -> Result<Client> {
  init_logging(log_options.unwrap_or_default())?;

  let store = build_store(db)?;
  let nonce = parse_nonce(nonce)?;

  let cursor_store = SqliteCursorStore::new(store.db());
  let mut mbb = MessageBackendBuilder::default();
  mbb.cursor_store(cursor_store);
  let api_client = mbb
    .from_bundle(backend.bundle.clone())
    .map_err(ErrorWrapper::from)?;

  create_client_inner(
    api_client,
    store,
    inbox_id,
    account_identifier,
    device_sync_worker_mode,
    worker_config,
    allow_offline,
    Some(backend.app_version()),
    nonce,
  )
  .await
}
