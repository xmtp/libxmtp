use crate::ErrorWrapper;
use crate::client::Client;
use crate::client::gateway_auth::{AuthCallback, AuthHandle};
use crate::client::options::{ClientMode, LogOptions, SyncWorkerMode};
use crate::identity::Identifier;
use napi::bindgen_prelude::{BigInt, Error, Result, Uint8Array};
use napi_derive::napi;
use std::ops::Deref;
use std::sync::Arc;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};
use xmtp_api_d14n::MessageBackendBuilder;
use xmtp_configuration::{MAX_DB_POOL_SIZE, MIN_DB_POOL_SIZE};
use xmtp_db::{EncryptedMessageStore, EncryptionKey, NativeDb};
use xmtp_mls::cursor_store::SqliteCursorStore;
use xmtp_mls::identity::IdentityStrategy;

static LOGGER_INIT: std::sync::OnceLock<Result<()>> = std::sync::OnceLock::new();

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
  db: DbOptions,
  inbox_id: String,
  account_identifier: Identifier,
  device_sync_server_url: Option<String>,
  device_sync_worker_mode: Option<SyncWorkerMode>,
  log_options: Option<LogOptions>,
  allow_offline: Option<bool>,
  app_version: Option<String>,
  nonce: Option<BigInt>,
  auth_callback: Option<&AuthCallback>,
  auth_handle: Option<&AuthHandle>,
  client_mode: Option<ClientMode>,
) -> Result<Client> {
  let client_mode = client_mode.unwrap_or_default();
  let root_identifier = account_identifier.clone();
  init_logging(log_options.unwrap_or_default())?;
  let mut backend = MessageBackendBuilder::default();
  backend
    .v3_host(&v3_host)
    .maybe_gateway_host(gateway_host)
    .readonly(matches!(client_mode, ClientMode::Notification))
    .maybe_auth_callback(auth_callback.map(|c| Arc::new(c.clone()) as _))
    .maybe_auth_handle(auth_handle.map(|h| h.clone().into()))
    .app_version(app_version.clone().unwrap_or_default())
    .is_secure(is_secure);

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
  let store = EncryptedMessageStore::new(db).map_err(ErrorWrapper::from)?;

  let nonce = match nonce {
    Some(n) => {
      let (signed, value, lossless) = n.get_u64();
      if signed {
        return Err(Error::from_reason("`nonce` must be non-negative"));
      }
      if !lossless {
        return Err(Error::from_reason("`nonce` is too large"));
      }
      value
    }
    None => 1,
  };
  let internal_account_identifier = account_identifier.clone().try_into()?;
  let identity_strategy = IdentityStrategy::new(
    inbox_id.clone(),
    internal_account_identifier,
    // this is a temporary solution
    nonce,
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
