use js_sys::Uint8Array;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{filter, fmt::format::Pretty};
use wasm_bindgen::prelude::{wasm_bindgen, JsError};
use wasm_bindgen::JsValue;
use xmtp_api_http::XmtpHttpApiClient;
use xmtp_cryptography::signature::ed25519_public_key_to_address;
use xmtp_id::associations::builder::SignatureRequest;
use xmtp_mls::builder::ClientBuilder;
use xmtp_mls::groups::scoped_client::ScopedGroupClient;
use xmtp_mls::identity::IdentityStrategy;
use xmtp_mls::storage::{EncryptedMessageStore, EncryptionKey, StorageOption};
use xmtp_mls::Client as MlsClient;
use xmtp_proto::xmtp::mls::message_contents::DeviceSyncKind;

use crate::conversations::Conversations;
use crate::signatures::SignatureRequestType;

pub type RustXmtpClient = MlsClient<XmtpHttpApiClient>;

#[wasm_bindgen]
pub struct Client {
  account_address: String,
  inner_client: Arc<RustXmtpClient>,
  signature_requests: Arc<Mutex<HashMap<SignatureRequestType, SignatureRequest>>>,
}

impl Client {
  pub fn inner_client(&self) -> &Arc<RustXmtpClient> {
    &self.inner_client
  }

  pub fn signature_requests(&self) -> &Arc<Mutex<HashMap<SignatureRequestType, SignatureRequest>>> {
    &self.signature_requests
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
        tracing_subscriber::filter::LevelFilter::from_str(f.to_str())
      } else {
        Ok(tracing_subscriber::filter::LevelFilter::INFO)
      }?;

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
pub async fn create_client(
  host: String,
  inbox_id: String,
  account_address: String,
  db_path: String,
  encryption_key: Option<Uint8Array>,
  history_sync_url: Option<String>,
  log_options: Option<LogOptions>,
) -> Result<Client, JsError> {
  init_logging(log_options.unwrap_or_default())?;
  xmtp_mls::storage::init_sqlite().await;
  let api_client = XmtpHttpApiClient::new(host.clone()).unwrap();

  let storage_option = StorageOption::Persistent(db_path);

  let store = match encryption_key {
    Some(key) => {
      let key: Vec<u8> = key.to_vec();
      let key: EncryptionKey = key
        .try_into()
        .map_err(|_| JsError::new("Malformed 32 byte encryption key"))?;
      EncryptedMessageStore::new(storage_option, key)
        .await
        .map_err(|_| JsError::new("Error creating encrypted message store"))?
    }
    None => EncryptedMessageStore::new_unencrypted(storage_option)
      .await
      .map_err(|_| JsError::new("Error creating unencrypted message store"))?,
  };

  let identity_strategy = IdentityStrategy::CreateIfNotFound(
    inbox_id.clone(),
    account_address.clone().to_lowercase(),
    // this is a temporary solution
    1,
    None,
  );

  let xmtp_client = match history_sync_url {
    Some(url) => ClientBuilder::new(identity_strategy)
      .api_client(api_client)
      .store(store)
      .history_sync_url(&url)
      .build()
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?,
    None => ClientBuilder::new(identity_strategy)
      .api_client(api_client)
      .store(store)
      .build()
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?,
  };

  Ok(Client {
    account_address,
    inner_client: Arc::new(xmtp_client),
    signature_requests: Arc::new(Mutex::new(HashMap::new())),
  })
}

#[wasm_bindgen]
impl Client {
  #[wasm_bindgen(getter, js_name = accountAddress)]
  pub fn account_address(&self) -> String {
    self.account_address.clone()
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
    ed25519_public_key_to_address(self.inner_client.installation_public_key().as_slice())
  }

  #[wasm_bindgen(js_name = canMessage)]
  pub async fn can_message(&self, account_addresses: Vec<String>) -> Result<JsValue, JsError> {
    let results: HashMap<String, bool> = self
      .inner_client
      .can_message(&account_addresses)
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;

    Ok(serde_wasm_bindgen::to_value(&results)?)
  }

  #[wasm_bindgen(js_name = registerIdentity)]
  pub async fn register_identity(&self) -> Result<(), JsError> {
    if self.is_registered() {
      return Err(JsError::new(
        "An identity is already registered with this client",
      ));
    }

    let mut signature_requests = self.signature_requests.lock().await;

    let signature_request = signature_requests
      .get(&SignatureRequestType::CreateInbox)
      .ok_or(JsError::new("No signature request found"))?;

    self
      .inner_client
      .register_identity(signature_request.clone())
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;

    signature_requests.remove(&SignatureRequestType::CreateInbox);

    Ok(())
  }

  #[wasm_bindgen(js_name = sendHistorySyncRequest)]
  pub async fn send_history_sync_request(&self) -> Result<(), JsError> {
    self.send_sync_request(DeviceSyncKind::MessageHistory).await
  }

  #[wasm_bindgen(js_name = sendConsentSyncRequest)]
  pub async fn send_consent_sync_request(&self) -> Result<(), JsError> {
    self.send_sync_request(DeviceSyncKind::Consent).await
  }

  async fn send_sync_request(&self, kind: DeviceSyncKind) -> Result<(), JsError> {
    let provider = self
      .inner_client
      .mls_provider()
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;
    self
      .inner_client
      .send_sync_request(&provider, kind)
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;

    Ok(())
  }

  #[wasm_bindgen(js_name = findInboxIdByAddress)]
  pub async fn find_inbox_id_by_address(&self, address: String) -> Result<Option<String>, JsError> {
    let inbox_id = self
      .inner_client
      .find_inbox_id_from_address(address)
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;

    Ok(inbox_id)
  }

  #[wasm_bindgen]
  pub fn conversations(&self) -> Conversations {
    Conversations::new(self.inner_client.clone())
  }
}
