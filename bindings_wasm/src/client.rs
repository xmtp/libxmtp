use js_sys::Uint8Array;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{filter, fmt::format::Pretty};
use wasm_bindgen::prelude::{wasm_bindgen, JsError};
use wasm_bindgen::JsValue;
use xmtp_api_http::XmtpHttpApiClient;
use xmtp_db::{EncryptedMessageStore, EncryptionKey, StorageOption};
use xmtp_id::associations::builder::SignatureRequest;
use xmtp_id::associations::Identifier as XmtpIdentifier;
use xmtp_mls::identity::IdentityStrategy;
use xmtp_mls::Client as MlsClient;
use xmtp_proto::xmtp::mls::message_contents::DeviceSyncKind;

use crate::conversations::Conversations;
use crate::identity::Identifier;
use crate::inbox_state::InboxState;
use crate::signatures::SignatureRequestType;

pub type RustXmtpClient = MlsClient<XmtpHttpApiClient>;

#[wasm_bindgen]
pub struct Client {
  account_identifier: Identifier,
  inner_client: Arc<RustXmtpClient>,
  pub(crate) signature_requests: HashMap<SignatureRequestType, SignatureRequest>,
}

impl Client {
  pub fn inner_client(&self) -> &Arc<RustXmtpClient> {
    &self.inner_client
  }

  pub fn signature_requests(&self) -> &HashMap<SignatureRequestType, SignatureRequest> {
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
  account_identifier: Identifier,
  db_path: Option<String>,
  encryption_key: Option<Uint8Array>,
  history_sync_url: Option<String>,
  log_options: Option<LogOptions>,
) -> Result<Client, JsError> {
  init_logging(log_options.unwrap_or_default())?;
  xmtp_db::init_sqlite().await;
  let api_client = XmtpHttpApiClient::new(host.clone(), "0.0.0".into())?;

  let storage_option = match db_path {
    Some(path) => StorageOption::Persistent(path),
    None => StorageOption::Ephemeral,
  };

  let store = match encryption_key {
    Some(key) => {
      let key: Vec<u8> = key.to_vec();
      let key: EncryptionKey = key
        .try_into()
        .map_err(|_| JsError::new("Malformed 32 byte encryption key"))?;
      EncryptedMessageStore::new(storage_option, key)
        .map_err(|e| JsError::new(&format!("Error creating encrypted message store {e}")))?
    }
    None => EncryptedMessageStore::new_unencrypted(storage_option)
      .map_err(|e| JsError::new(&format!("Error creating unencrypted message store {e}")))?,
  };

  let identity_strategy = IdentityStrategy::new(
    inbox_id.clone(),
    account_identifier.clone().try_into()?,
    // this is a temporary solution
    1,
    None,
  );

  let xmtp_client = match history_sync_url {
    Some(url) => xmtp_mls::Client::builder(identity_strategy)
      .api_client(api_client)
      .with_remote_verifier()?
      .store(store)
      .history_sync_url(&url)
      .build()
      .await
      .map_err(|e| JsError::new(&e.to_string()))?,
    None => xmtp_mls::Client::builder(identity_strategy)
      .api_client(api_client)
      .with_remote_verifier()?
      .store(store)
      .build()
      .await
      .map_err(|e| JsError::new(&e.to_string()))?,
  };

  Ok(Client {
    account_identifier,
    inner_client: Arc::new(xmtp_client),
    signature_requests: HashMap::new(),
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

  #[wasm_bindgen(js_name = registerIdentity)]
  pub async fn register_identity(&mut self) -> Result<(), JsError> {
    if self.is_registered() {
      return Err(JsError::new(
        "An identity is already registered with this client",
      ));
    }

    let signature_request = self
      .signature_requests
      .get(&SignatureRequestType::CreateInbox)
      .ok_or(JsError::new("No signature request found"))?
      .clone();
    self
      .inner_client
      .register_identity(signature_request)
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;

    self
      .signature_requests
      .remove(&SignatureRequestType::CreateInbox);

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

  #[wasm_bindgen(js_name = findInboxIdByIdentifier)]
  pub async fn find_inbox_id_by_identifier(
    &self,
    identifier: Identifier,
  ) -> Result<Option<String>, JsError> {
    let conn = self
      .inner_client
      .store()
      .conn()
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;
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
}
