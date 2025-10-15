use js_sys::Uint8Array;
use std::collections::HashMap;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{filter, fmt::format::Pretty};
use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::{JsError, wasm_bindgen};
use xmtp_api::ApiDebugWrapper;
use xmtp_api_grpc::v3::Client as TonicApiClient;
use xmtp_db::{EncryptedMessageStore, EncryptionKey, StorageOption, WasmDb};
use xmtp_id::associations::Identifier as XmtpIdentifier;
use xmtp_mls::Client as MlsClient;
use xmtp_mls::builder::SyncWorkerMode;
use xmtp_mls::context::XmtpMlsLocalContext;
use xmtp_mls::groups::MlsGroup;
use xmtp_mls::identity::IdentityStrategy;
use xmtp_mls::utils::events::upload_debug_archive;
use xmtp_proto::api_client::AggregateStats;

use crate::conversations::Conversations;
use crate::identity::{ApiStats, Identifier, IdentityStats};
use crate::inbox_state::InboxState;

pub type MlsContext = Arc<
  XmtpMlsLocalContext<
    ApiDebugWrapper<TonicApiClient>,
    xmtp_db::DefaultStore,
    xmtp_db::DefaultMlsStore,
  >,
>;
pub type RustXmtpClient = MlsClient<MlsContext>;
pub type RustMlsGroup = MlsGroup<MlsContext>;

#[wasm_bindgen]
pub struct Client {
  account_identifier: Identifier,
  inner_client: Arc<RustXmtpClient>,
  app_version: Option<String>,
}

impl Client {
  pub fn inner_client(&self) -> &Arc<RustXmtpClient> {
    &self.inner_client
  }
}

static LOGGER_INIT: std::sync::OnceLock<Result<(), filter::LevelParseError>> =
  std::sync::OnceLock::new();

#[wasm_bindgen]
#[derive(Copy, Clone, Debug)]
pub enum LogLevel {
  Off = "off",
  Error = "error",
  Warn = "warn",
  Info = "info",
  Debug = "debug",
  Trace = "trace",
}

#[wasm_bindgen]
#[derive(Copy, Clone, Debug)]
pub enum DeviceSyncWorkerMode {
  Enabled = "enabled",
  Disabled = "disabled",
}

impl From<DeviceSyncWorkerMode> for SyncWorkerMode {
  fn from(value: DeviceSyncWorkerMode) -> Self {
    match value {
      DeviceSyncWorkerMode::Enabled => Self::Enabled,
      DeviceSyncWorkerMode::Disabled => Self::Disabled,
      DeviceSyncWorkerMode::__Invalid => unreachable!("DeviceSyncWorkerMode is invalid."),
    }
  }
}

/// Specify options for the logger
#[derive(Default)]
#[wasm_bindgen(getter_with_clone)]
pub struct LogOptions {
  /// enable structured JSON logging to stdout.Useful for third-party log viewers
  pub structured: bool,
  /// enable performance metrics for libxmtp in the `performance` tab
  pub performance: bool,
  /// filter for logs
  pub level: Option<LogLevel>,
}

#[wasm_bindgen]
impl LogOptions {
  #[wasm_bindgen(constructor)]
  pub fn new(structured: bool, performance: bool, level: Option<LogLevel>) -> Self {
    Self {
      structured,
      performance,
      level,
    }
  }
}

fn init_logging(options: LogOptions) -> Result<(), JsError> {
  LOGGER_INIT
    .get_or_init(|| {
      console_error_panic_hook::set_once();

      let filter = if let Some(f) = options.level {
        xmtp_common::filter_directive(f.to_str())
      } else {
        EnvFilter::builder().parse_lossy("info")
      };

      if options.structured {
        let fmt = tracing_subscriber::fmt::layer()
          .json()
          .flatten_event(true)
          .with_level(true)
          .without_time() // need to test whether this would break browsers
          .with_target(true);

        tracing_subscriber::registry().with(filter).with(fmt).init();
      } else {
        let fmt = tracing_subscriber::fmt::layer()
          .with_ansi(false) // not supported by all browsers
          .without_time() // std::time break things, but chrono might work
          .with_writer(tracing_web::MakeWebConsoleWriter::new());

        let subscriber = tracing_subscriber::registry().with(fmt).with(filter);

        if options.performance {
          subscriber
            .with(tracing_web::performance_layer().with_details_from_fields(Pretty::default()))
            .init();
        } else {
          subscriber.init();
        }
      }
      Ok(())
    })
    .clone()?;
  Ok(())
}

#[wasm_bindgen(js_name = createClient)]
#[allow(clippy::too_many_arguments)]
pub async fn create_client(
  host: String,
  inbox_id: String,
  account_identifier: Identifier,
  db_path: Option<String>,
  encryption_key: Option<Uint8Array>,
  device_sync_server_url: Option<String>,
  device_sync_worker_mode: Option<DeviceSyncWorkerMode>,
  log_options: Option<LogOptions>,
  allow_offline: Option<bool>,
  disable_events: Option<bool>,
  app_version: Option<String>,
) -> Result<Client, JsError> {
  init_logging(log_options.unwrap_or_default())?;
  let api_client = TonicApiClient::create(host.clone(), true, app_version.clone())?;

  let sync_api_client = TonicApiClient::create(host.clone(), true, app_version.clone())?;

  let storage_option = match db_path {
    Some(path) => StorageOption::Persistent(path),
    None => StorageOption::Ephemeral,
  };

  let store = match encryption_key {
    Some(key) => {
      let key: Vec<u8> = key.to_vec();
      let _key: EncryptionKey = key
        .try_into()
        .map_err(|_| JsError::new("Malformed 32 byte encryption key"))?;
      let db = WasmDb::new(&storage_option).await?;
      EncryptedMessageStore::new(db)
        .map_err(|e| JsError::new(&format!("Error creating encrypted message store {e}")))?
    }
    None => {
      let db = WasmDb::new(&storage_option).await?;
      EncryptedMessageStore::new(db)
        .map_err(|e| JsError::new(&format!("Error creating unencrypted message store {e}")))?
    }
  };

  let identity_strategy = IdentityStrategy::new(
    inbox_id.clone(),
    account_identifier.clone().try_into()?,
    // this is a temporary solution
    1,
    None,
  );

  let mut builder = match device_sync_server_url {
    Some(url) => xmtp_mls::Client::builder(identity_strategy)
      .api_clients(api_client, sync_api_client)
      .enable_api_debug_wrapper()?
      .with_remote_verifier()?
      .with_allow_offline(allow_offline)
      .with_disable_events(disable_events)
      .store(store)
      .device_sync_server_url(&url),
    None => xmtp_mls::Client::builder(identity_strategy)
      .api_clients(api_client, sync_api_client)
      .enable_api_debug_wrapper()?
      .with_remote_verifier()?
      .with_allow_offline(allow_offline)
      .with_disable_events(disable_events)
      .store(store),
  };

  if let Some(device_sync_worker_mode) = device_sync_worker_mode {
    builder = builder.device_sync_worker_mode(device_sync_worker_mode.into());
  }

  let xmtp_client = builder
    .default_mls_store()
    .map_err(|e| JsError::new(&e.to_string()))?
    .build()
    .await
    .map_err(|e| JsError::new(&e.to_string()))?;

  Ok(Client {
    account_identifier,
    #[allow(clippy::arc_with_non_send_sync)]
    inner_client: Arc::new(xmtp_client),
    app_version,
  })
}

#[wasm_bindgen]
impl Client {
  #[wasm_bindgen(getter, js_name = accountIdentifier)]
  pub fn account_identifier(&self) -> Identifier {
    self.account_identifier.clone()
  }

  #[wasm_bindgen(getter, js_name = inboxId)]
  pub fn inbox_id(&self) -> String {
    self.inner_client.inbox_id().to_string()
  }

  #[wasm_bindgen(getter, js_name = isRegistered)]
  pub fn is_registered(&self) -> bool {
    self.inner_client.identity().is_ready()
  }

  #[wasm_bindgen(getter, js_name = installationId)]
  pub fn installation_id(&self) -> String {
    hex::encode(self.inner_client.installation_public_key())
  }

  #[wasm_bindgen(getter, js_name = installationIdBytes)]
  pub fn installation_id_bytes(&self) -> Uint8Array {
    Uint8Array::from(self.inner_client.installation_public_key().as_slice())
  }

  #[wasm_bindgen(getter, js_name = appVersion)]
  pub fn app_version(&self) -> String {
    self.app_version.clone().unwrap_or("0.0.0".to_string())
  }

  #[wasm_bindgen(getter, js_name = libxmtpVersion)]
  pub fn libxmtp_version(&self) -> String {
    env!("CARGO_PKG_VERSION").to_string()
  }

  #[wasm_bindgen(js_name = canMessage)]
  /// Output booleans should be zipped with the index of input identifiers
  pub async fn can_message(
    &self,
    account_identifiers: Vec<Identifier>,
  ) -> Result<JsValue, JsError> {
    let account_identifiers: Result<Vec<XmtpIdentifier>, JsError> = account_identifiers
      .iter()
      .cloned()
      .map(|ident| ident.try_into())
      .collect();
    let account_identifiers = account_identifiers?;

    let results = self
      .inner_client
      .can_message(&account_identifiers)
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;

    let results: HashMap<_, _> = results
      .into_iter()
      .map(|(k, v)| (format!("{k}"), v))
      .collect();

    Ok(crate::to_value(&results)?)
  }

  #[wasm_bindgen(js_name = sendSyncRequest)]
  pub async fn send_sync_request(&self) -> Result<(), JsError> {
    self
      .inner_client
      .device_sync_client()
      .send_sync_request()
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;

    Ok(())
  }

  #[wasm_bindgen(js_name = findInboxIdByIdentifier)]
  pub async fn find_inbox_id_by_identifier(
    &self,
    identifier: Identifier,
  ) -> Result<Option<String>, JsError> {
    let conn = self.inner_client.context.store().db();
    let inbox_id = self
      .inner_client
      .find_inbox_id_from_identifier(&conn, identifier.try_into()?)
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;

    Ok(inbox_id)
  }

  #[wasm_bindgen(js_name = inboxStateFromInboxIds)]
  pub async fn inbox_state_from_inbox_ids(
    &self,
    inbox_ids: Vec<String>,
    refresh_from_network: bool,
  ) -> Result<Vec<InboxState>, JsError> {
    let state = self
      .inner_client
      .inbox_addresses(
        refresh_from_network,
        inbox_ids.iter().map(String::as_str).collect(),
      )
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;
    Ok(state.into_iter().map(Into::into).collect())
  }

  #[wasm_bindgen]
  pub fn conversations(&self) -> Conversations {
    Conversations::new(self.inner_client.clone())
  }

  #[wasm_bindgen(js_name = syncPreferences)]
  pub async fn sync_preferences(&self) -> Result<u32, JsError> {
    let inner = self.inner_client.as_ref();

    let num_groups_synced: usize = inner
      .sync_all_welcomes_and_history_sync_groups()
      .await
      .map_err(|e| JsError::new(&format!("{e}")))?;

    let num_groups_synced: u32 = num_groups_synced
      .try_into()
      .map_err(|_| JsError::new("Failed to convert usize to u32"))?;

    Ok(num_groups_synced)
  }

  #[wasm_bindgen(js_name = apiStatistics)]
  pub fn api_statistics(&self) -> ApiStats {
    self.inner_client.api_stats().into()
  }

  #[wasm_bindgen(js_name = apiIdentityStatistics)]
  pub fn api_identity_statistics(&self) -> IdentityStats {
    self.inner_client.identity_api_stats().into()
  }

  #[wasm_bindgen(js_name = apiAggregateStatistics)]
  pub fn api_aggregate_statistics(&self) -> String {
    let api = self.inner_client.api_stats();
    let identity = self.inner_client.identity_api_stats();
    let aggregate = AggregateStats { mls: api, identity };
    format!("{:?}", aggregate)
  }

  #[wasm_bindgen(js_name = clearAllStatistics)]
  pub fn clear_all_statistics(&self) {
    self.inner_client.clear_stats()
  }

  #[wasm_bindgen(js_name = uploadDebugArchive)]
  pub async fn upload_debug_archive(&self, server_url: String) -> Result<String, JsError> {
    let db = self.inner_client().context.db();

    upload_debug_archive(db, Some(server_url))
      .await
      .map_err(|e| JsError::new(&format!("{e}")))
  }

  #[wasm_bindgen(js_name = deleteMessage)]
  pub fn delete_message(&self, message_id: Vec<u8>) -> Result<u32, JsError> {
    let deleted_count = self
      .inner_client
      .delete_message(message_id)
      .map_err(|e| JsError::new(&format!("{e}")))?;
    Ok(deleted_count as u32)
  }
}
