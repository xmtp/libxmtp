use crate::conversations::NapiConversations;
use crate::inbox_state::NapiInboxState;
use crate::signatures::NapiSignatureRequestType;
use crate::ErrorWrapper;
use napi::bindgen_prelude::{Error, Result, Uint8Array};
use napi_derive::napi;
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::{Arc, Once};
use tokio::sync::Mutex;
use tracing_subscriber::{filter::EnvFilter, fmt, prelude::*};
pub use xmtp_api_grpc::grpc_api_helper::Client as TonicApiClient;
use xmtp_cryptography::signature::ed25519_public_key_to_address;
use xmtp_id::associations::builder::SignatureRequest;
use xmtp_mls::builder::ClientBuilder;
use xmtp_mls::groups::scoped_client::LocalScopedGroupClient;
use xmtp_mls::identity::IdentityStrategy;
use xmtp_mls::storage::{EncryptedMessageStore, EncryptionKey, StorageOption};
use xmtp_mls::Client as MlsClient;
use xmtp_proto::xmtp::mls::message_contents::DeviceSyncKind;

pub type RustXmtpClient = MlsClient<TonicApiClient>;
static LOGGER_INIT: Once = Once::new();

#[napi]
pub struct NapiClient {
  inner_client: Arc<RustXmtpClient>,
  signature_requests: Arc<Mutex<HashMap<NapiSignatureRequestType, SignatureRequest>>>,
  pub account_address: String,
}

impl NapiClient {
  pub fn inner_client(&self) -> &Arc<RustXmtpClient> {
    &self.inner_client
  }

  pub fn signature_requests(
    &self,
  ) -> &Arc<Mutex<HashMap<NapiSignatureRequestType, SignatureRequest>>> {
    &self.signature_requests
  }
}

/// Create an MLS client
/// Optionally specify a filter for the log level as a string.
/// It can be one of: `debug`, `info`, `warn`, `error` or 'off'.
/// By default, logging is set to `info`. `off` completely disables logging.
#[allow(clippy::too_many_arguments)]
#[napi]
pub async fn create_client(
  host: String,
  is_secure: bool,
  db_path: String,
  inbox_id: String,
  account_address: String,
  encryption_key: Option<Uint8Array>,
  history_sync_url: Option<String>,
  env_filter: Option<String>,
) -> Result<NapiClient> {
  LOGGER_INIT.call_once(|| {
    let filter = EnvFilter::builder()
      .with_regex(false)
      .with_default_directive(tracing::metadata::LevelFilter::INFO.into())
      .parse_lossy(env_filter.unwrap_or_default());

    tracing_subscriber::registry()
      .with(fmt::layer())
      .with(filter)
      .init();
  });
  let api_client = TonicApiClient::create(host.clone(), is_secure)
    .await
    .map_err(|_| Error::from_reason("Error creating Tonic API client"))?;

  let storage_option = StorageOption::Persistent(db_path);

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
      .map_err(ErrorWrapper::from)?,

    None => ClientBuilder::new(identity_strategy)
      .api_client(api_client)
      .store(store)
      .build()
      .await
      .map_err(ErrorWrapper::from)?,
  };

  Ok(NapiClient {
    inner_client: Arc::new(xmtp_client),
    account_address,
    signature_requests: Arc::new(Mutex::new(HashMap::new())),
  })
}

#[napi]
impl NapiClient {
  #[napi]
  pub fn inbox_id(&self) -> String {
    self.inner_client.inbox_id()
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
      .get(&NapiSignatureRequestType::CreateInbox)
      .ok_or(Error::from_reason("No signature request found"))?;

    self
      .inner_client
      .register_identity(signature_request.clone())
      .await
      .map_err(ErrorWrapper::from)?;

    signature_requests.remove(&NapiSignatureRequestType::CreateInbox);

    Ok(())
  }

  #[napi]
  pub fn conversations(&self) -> NapiConversations {
    NapiConversations::new(self.inner_client.clone())
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
  ) -> Result<Vec<NapiInboxState>> {
    let state = self
      .inner_client
      .inbox_addresses(refresh_from_network, inbox_ids)
      .await
      .map_err(ErrorWrapper::from)?;
    Ok(state.into_iter().map(Into::into).collect())
  }
}
