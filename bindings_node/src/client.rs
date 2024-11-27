use crate::conversations::Conversations;
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
use xmtp_cryptography::signature::ed25519_public_key_to_address;
use xmtp_id::associations::builder::SignatureRequest;
use xmtp_id::associations::{AssociationState, IdentityAction, MemberIdentifier};
use xmtp_id::scw_verifier::MultiSmartContractSignatureVerifier;
use xmtp_mls::api::{ApiClientWrapper, GetIdentityUpdatesV2Filter};
use xmtp_mls::builder::ClientBuilder;
use xmtp_mls::groups::scoped_client::LocalScopedGroupClient;
use xmtp_mls::identity::IdentityStrategy;
use xmtp_mls::retry::Retry;
use xmtp_mls::storage::{association_state, EncryptedMessageStore, EncryptionKey, StorageOption};
use xmtp_mls::Client as MlsClient;
use xmtp_proto::xmtp::mls::message_contents::DeviceSyncKind;

pub type RustXmtpClient = MlsClient<TonicApiClient>;
static LOGGER_INIT: std::sync::OnceLock<Result<()>> = std::sync::OnceLock::new();

#[napi]
pub struct Client {
  inner_client: Arc<RustXmtpClient>,
  signature_requests: Arc<Mutex<HashMap<SignatureRequestType, SignatureRequest>>>,
  pub account_address: String,
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
          .with_timer(tracing_subscriber::fmt::time::ChronoLocal::rfc_3339())
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
  account_address: String,
  encryption_key: Option<Uint8Array>,
  history_sync_url: Option<String>,
  log_options: Option<LogOptions>,
) -> Result<Client> {
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
        .map_err(|_| Error::from_reason("Malformed 32 byte encryption key".to_string()))?;
      EncryptedMessageStore::new(storage_option, key)
        .await
        .map_err(|_| Error::from_reason("Error creating encrypted message store"))?
    }
    None => EncryptedMessageStore::new_unencrypted(storage_option)
      .await
      .map_err(|_| Error::from_reason("Error creating unencrypted message store"))?,
  };

  let identity_strategy = IdentityStrategy::new(
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
      .map_err(ErrorWrapper::from)?,

    None => ClientBuilder::new(identity_strategy)
      .api_client(api_client)
      .store(store)
      .build()
      .await
      .map_err(ErrorWrapper::from)?,
  };

  Ok(Client {
    inner_client: Arc::new(xmtp_client),
    account_address,
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
    ed25519_public_key_to_address(self.inner_client.installation_public_key().as_slice())
  }

  #[napi]
  pub fn installation_id_bytes(&self) -> Uint8Array {
    self.inner_client.installation_public_key().into()
  }

  #[napi]
  pub async fn can_message(&self, account_addresses: Vec<String>) -> Result<HashMap<String, bool>> {
    let results: HashMap<String, bool> = self
      .inner_client
      .can_message(&account_addresses)
      .await
      .map_err(ErrorWrapper::from)?;

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
  pub async fn send_history_sync_request(&self) -> Result<()> {
    self.send_sync_request(DeviceSyncKind::MessageHistory).await
  }

  #[napi]
  pub async fn send_consent_sync_request(&self) -> Result<()> {
    self.send_sync_request(DeviceSyncKind::Consent).await
  }

  async fn send_sync_request(&self, kind: DeviceSyncKind) -> Result<()> {
    let provider = self
      .inner_client
      .mls_provider()
      .map_err(ErrorWrapper::from)?;
    self
      .inner_client
      .send_sync_request(&provider, kind)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(())
  }

  #[napi]
  pub async fn find_inbox_id_by_address(&self, address: String) -> Result<Option<String>> {
    let inbox_id = self
      .inner_client
      .find_inbox_id_from_address(address)
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
  pub async fn is_address_authorized(&self, inbox_id: String, address: String) -> Result<bool> {
    self
      .is_member_of_association_state(&inbox_id, &MemberIdentifier::Address(address))
      .await
  }

  #[napi]
  pub async fn is_installation_authorized(
    &self,
    inbox_id: String,
    installation_id: Uint8Array,
  ) -> Result<bool> {
    self
      .is_member_of_association_state(
        &inbox_id,
        &MemberIdentifier::Installation(installation_id.to_vec()),
      )
      .await
  }

  async fn is_member_of_association_state(
    &self,
    inbox_id: &str,
    identifier: &MemberIdentifier,
  ) -> Result<bool> {
    let client = &self.inner_client;
    let conn = self
      .inner_client
      .store()
      .conn()
      .map_err(ErrorWrapper::from)?;

    let association_state = client
      .get_association_state(&conn, inbox_id, None)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(association_state.get(identifier).is_some())
  }
}

pub async fn is_installation_authorized(
  host: String,
  inbox_id: String,
  installation_id: Uint8Array,
) -> Result<bool> {
  is_member_of_association_state(
    &host,
    &inbox_id,
    &MemberIdentifier::Installation(installation_id.to_vec()),
  )
  .await
}

pub async fn is_address_authorized(
  host: String,
  inbox_id: String,
  address: String,
) -> Result<bool> {
  is_member_of_association_state(&host, &inbox_id, &MemberIdentifier::Address(address)).await
}

async fn is_member_of_association_state(
  host: &str,
  inbox_id: &str,
  identifier: &MemberIdentifier,
) -> Result<bool> {
  let api_client = TonicApiClient::create(host, true)
    .await
    .map_err(ErrorWrapper::from)?;
  let wrapper = ApiClientWrapper::new(Arc::new(api_client), Retry::default());

  let filters = vec![GetIdentityUpdatesV2Filter {
    inbox_id: inbox_id.to_string(),
    sequence_id: None,
  }];
  let mut updates = wrapper
    .get_identity_updates_v2(filters)
    .await
    .map_err(ErrorWrapper::from)?;

  let Some(updates) = updates.remove(inbox_id) else {
    return Err(Error::from_reason("Unable to find provided inbox_id"));
  };
  let updates: Vec<_> = updates.into_iter().map(|u| u.update).collect();

  let scw_verifier =
    MultiSmartContractSignatureVerifier::new_from_env().map_err(ErrorWrapper::from)?;

  let mut association_state = None;

  for update in updates {
    let update = update
      .to_verified(&scw_verifier)
      .await
      .map_err(ErrorWrapper::from)?;
    association_state = Some(
      update
        .update_state(association_state, update.client_timestamp_ns)
        .map_err(ErrorWrapper::from)?,
    );
  }
  let association_state =
    association_state.ok_or(Error::from_reason("Unable to create association state"))?;

  Ok(association_state.get(identifier).is_some())
}
