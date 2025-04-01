use crate::conversations::Conversations;
use crate::identity::{Identifier, IdentityExt};
use crate::inbox_state::InboxState;
use crate::signatures::SignatureRequestType;
use crate::ErrorWrapper;
use napi::bindgen_prelude::{Error, Result, Uint8Array};
use napi_derive::napi;
use std::collections::HashMap;
use std::ops::Deref;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing_subscriber::{fmt, prelude::*};
pub use xmtp_api_grpc::grpc_api_helper::Client as TonicApiClient;
use xmtp_db::{EncryptedMessageStore, EncryptionKey, StorageOption};
use xmtp_id::associations::builder::SignatureRequest;
use xmtp_mls::groups::scoped_client::LocalScopedGroupClient;
use xmtp_mls::identity::IdentityStrategy;
use xmtp_mls::Client as MlsClient;

pub type RustXmtpClient = MlsClient<TonicApiClient>;
static LOGGER_INIT: std::sync::OnceLock<Result<()>> = std::sync::OnceLock::new();

#[napi]
pub struct Client {
  inner_client: Arc<RustXmtpClient>,
  signature_requests: Arc<Mutex<HashMap<SignatureRequestType, SignatureRequest>>>,
  pub account_identifier: Identifier,
}

impl Client {
  pub fn inner_client(&self) -> &Arc<RustXmtpClient> {
    &self.inner_client
  }

  pub fn signature_requests(&self) -> &Arc<Mutex<HashMap<SignatureRequestType, SignatureRequest>>> {
    &self.signature_requests
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

fn init_logging(options: LogOptions) -> Result<()> {
  LOGGER_INIT
    .get_or_init(|| {
      let filter = if let Some(f) = options.level {
        tracing_subscriber::filter::LevelFilter::from_str(&f.to_string())
      } else {
        Ok(tracing_subscriber::filter::LevelFilter::INFO)
      }
      .map_err(ErrorWrapper::from)?;

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
    .clone()
    .map_err(ErrorWrapper::from)?;
  Ok(())
}

/**
 * Create a client
 *
 * Optionally specify a filter for the log level as a string.
 * It can be one of: `debug`, `info`, `warn`, `error` or 'off'.
 * By default, logging is disabled.
 */
#[allow(clippy::too_many_arguments)]
#[napi]
pub async fn create_client(
  host: String,
  is_secure: bool,
  db_path: Option<String>,
  inbox_id: String,
  account_identifier: Identifier,
  encryption_key: Option<Uint8Array>,
  device_sync_server_url: Option<String>,
  log_options: Option<LogOptions>,
) -> Result<Client> {
  let root_identifier = account_identifier.clone();

  init_logging(log_options.unwrap_or_default())?;
  let api_client = TonicApiClient::create(&host, is_secure)
    .await
    .map_err(|_| Error::from_reason("Error creating Tonic API client"))?;

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
      EncryptedMessageStore::new(storage_option, key)
        .map_err(|e| Error::from_reason(format!("Error Creating Encrypted Message store {}", e)))?
    }
    None => EncryptedMessageStore::new_unencrypted(storage_option)
      .map_err(|e| Error::from_reason(format!("{e} Error creating unencrypted message store")))?,
  };

  let internal_account_identifier = account_identifier.clone().try_into()?;
  let identity_strategy = IdentityStrategy::new(
    inbox_id.clone(),
    internal_account_identifier,
    // this is a temporary solution
    1,
    None,
  );

  let xmtp_client = match device_sync_server_url {
    Some(url) => xmtp_mls::Client::builder(identity_strategy)
      .api_client(api_client)
      .with_remote_verifier()
      .map_err(ErrorWrapper::from)?
      .store(store)
      .device_sync_server_url(&url)
      .build()
      .await
      .map_err(ErrorWrapper::from)?,

    None => xmtp_mls::Client::builder(identity_strategy)
      .api_client(api_client)
      .with_remote_verifier()
      .map_err(ErrorWrapper::from)?
      .store(store)
      .build()
      .await
      .map_err(ErrorWrapper::from)?,
  };

  Ok(Client {
    inner_client: Arc::new(xmtp_client),
    account_identifier: root_identifier,
    signature_requests: Arc::new(Mutex::new(HashMap::new())),
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
  pub async fn register_identity(&self) -> Result<()> {
    if self.is_registered() {
      return Err(Error::from_reason(
        "An identity is already registered with this client",
      ));
    }

    let mut signature_requests = self.signature_requests.lock().await;

    let signature_request = signature_requests
      .get(&SignatureRequestType::CreateInbox)
      .ok_or(Error::from_reason("No signature request found"))?;

    self
      .inner_client
      .register_identity(signature_request.clone())
      .await
      .map_err(ErrorWrapper::from)?;

    signature_requests.remove(&SignatureRequestType::CreateInbox);

    Ok(())
  }

  #[napi]
  pub fn conversations(&self) -> Conversations {
    Conversations::new(self.inner_client.clone())
  }

  #[napi]
  pub async fn send_sync_request(&self) -> Result<()> {
    let provider = self
      .inner_client
      .mls_provider()
      .map_err(ErrorWrapper::from)?;
    self
      .inner_client
      .send_sync_request(&provider)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }

  #[napi]
  pub async fn find_inbox_id_by_identifier(
    &self,
    identifier: Identifier,
  ) -> Result<Option<String>> {
    let conn = self
      .inner_client()
      .store()
      .conn()
      .map_err(ErrorWrapper::from)?;

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
}
