use crate::consent_state::{NapiConsent, NapiConsentEntityType, NapiConsentState};
use crate::conversations::NapiConversations;
use crate::inbox_state::NapiInboxState;
use crate::ErrorWrapper;
use napi::bindgen_prelude::{Error, Result, Uint8Array};
use napi_derive::napi;
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;
use tokio::sync::Mutex;
pub use xmtp_api_grpc::grpc_api_helper::Client as TonicApiClient;
use xmtp_cryptography::signature::ed25519_public_key_to_address;
use xmtp_id::associations::builder::SignatureRequest;
use xmtp_id::associations::generate_inbox_id as xmtp_id_generate_inbox_id;
use xmtp_id::associations::unverified::UnverifiedSignature;
use xmtp_mls::api::ApiClientWrapper;
use xmtp_mls::builder::ClientBuilder;
use xmtp_mls::identity::IdentityStrategy;
use xmtp_mls::retry::Retry;
use xmtp_mls::storage::consent_record::StoredConsentRecord;
use xmtp_mls::storage::{EncryptedMessageStore, EncryptionKey, StorageOption};
use xmtp_mls::Client as MlsClient;

pub type RustXmtpClient = MlsClient<TonicApiClient>;

#[napi]
#[derive(Eq, Hash, PartialEq)]
pub enum NapiSignatureRequestType {
  AddWallet,
  CreateInbox,
  RevokeWallet,
  RevokeInstallations,
}

#[napi]
pub struct NapiClient {
  inner_client: Arc<RustXmtpClient>,
  signature_requests: Arc<Mutex<HashMap<NapiSignatureRequestType, SignatureRequest>>>,
  pub account_address: String,
}

#[napi]
pub async fn create_client(
  host: String,
  is_secure: bool,
  db_path: String,
  inbox_id: String,
  account_address: String,
  encryption_key: Option<Uint8Array>,
  history_sync_url: Option<String>,
) -> Result<NapiClient> {
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
        .map_err(|_| Error::from_reason("Error creating encrypted message store"))?
    }
    None => EncryptedMessageStore::new_unencrypted(storage_option)
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
pub async fn get_inbox_id_for_address(
  host: String,
  is_secure: bool,
  account_address: String,
) -> Result<Option<String>> {
  let account_address = account_address.to_lowercase();
  let api_client = ApiClientWrapper::new(
    TonicApiClient::create(host.clone(), is_secure)
      .await
      .map_err(ErrorWrapper::from)?,
    Retry::default(),
  );

  let results = api_client
    .get_inbox_ids(vec![account_address.clone()])
    .await
    .map_err(ErrorWrapper::from)?;

  Ok(results.get(&account_address).cloned())
}

#[napi]
pub fn generate_inbox_id(account_address: String) -> String {
  let account_address = account_address.to_lowercase();
  // ensure that the nonce is always 1 for now since this will only be used for the
  // create_client function above, which also has a hard-coded nonce of 1
  xmtp_id_generate_inbox_id(&account_address, &1)
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
      .can_message(account_addresses)
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
  pub async fn create_inbox_signature_text(&self) -> Result<Option<String>> {
    let signature_request = match self.inner_client.identity().signature_request() {
      Some(signature_req) => signature_req,
      // this should never happen since we're checking for it above in is_registered
      None => return Err(Error::from_reason("No signature request found")),
    };
    let signature_text = signature_request.signature_text();
    let mut signature_requests = self.signature_requests.lock().await;

    signature_requests.insert(NapiSignatureRequestType::CreateInbox, signature_request);

    Ok(Some(signature_text))
  }

  #[napi]
  pub fn conversations(&self) -> NapiConversations {
    NapiConversations::new(self.inner_client.clone())
  }

  #[napi]
  pub async fn request_history_sync(&self) -> Result<()> {
    let _ = self
      .inner_client
      .send_history_request()
      .await
      .map_err(ErrorWrapper::from);

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

  /**
   * Get the client's inbox state.
   *
   * If `refresh_from_network` is true, the client will go to the network first to refresh the state.
   * Otherwise, the state will be read from the local database.
   */
  #[napi]
  pub async fn inbox_state(&self, refresh_from_network: bool) -> Result<NapiInboxState> {
    let state = self
      .inner_client
      .inbox_state(refresh_from_network)
      .await
      .map_err(ErrorWrapper::from)?;
    Ok(state.into())
  }

  #[napi]
  pub async fn get_latest_inbox_state(&self, inbox_id: String) -> Result<NapiInboxState> {
    let conn = self
      .inner_client
      .store()
      .conn()
      .map_err(ErrorWrapper::from)?;
    let state = self
      .inner_client
      .get_latest_association_state(&conn, &inbox_id)
      .await
      .map_err(ErrorWrapper::from)?;
    Ok(state.into())
  }

  #[napi]
  pub async fn add_wallet_signature_text(
    &self,
    existing_wallet_address: String,
    new_wallet_address: String,
  ) -> Result<String> {
    let signature_request = self
      .inner_client
      .associate_wallet(
        existing_wallet_address.to_lowercase(),
        new_wallet_address.to_lowercase(),
      )
      .map_err(ErrorWrapper::from)?;
    let signature_text = signature_request.signature_text();
    let mut signature_requests = self.signature_requests.lock().await;

    signature_requests.insert(NapiSignatureRequestType::AddWallet, signature_request);

    Ok(signature_text)
  }

  #[napi]
  pub async fn revoke_wallet_signature_text(&self, wallet_address: String) -> Result<String> {
    let signature_request = self
      .inner_client
      .revoke_wallets(vec![wallet_address.to_lowercase()])
      .await
      .map_err(ErrorWrapper::from)?;
    let signature_text = signature_request.signature_text();
    let mut signature_requests = self.signature_requests.lock().await;

    signature_requests.insert(NapiSignatureRequestType::RevokeWallet, signature_request);

    Ok(signature_text)
  }

  #[napi]
  pub async fn revoke_installations_signature_text(&self) -> Result<String> {
    let installation_id = self.inner_client.installation_public_key();
    let inbox_state = self
      .inner_client
      .inbox_state(true)
      .await
      .map_err(ErrorWrapper::from)?;
    let other_installation_ids = inbox_state
      .installation_ids()
      .into_iter()
      .filter(|id| id != &installation_id)
      .collect();
    let signature_request = self
      .inner_client
      .revoke_installations(other_installation_ids)
      .await
      .map_err(ErrorWrapper::from)?;
    let signature_text = signature_request.signature_text();
    let mut signature_requests = self.signature_requests.lock().await;

    signature_requests.insert(
      NapiSignatureRequestType::RevokeInstallations,
      signature_request,
    );

    Ok(signature_text)
  }

  #[napi]
  pub async fn add_signature(
    &self,
    signature_type: NapiSignatureRequestType,
    signature_bytes: Uint8Array,
  ) -> Result<()> {
    let mut signature_requests = self.signature_requests.lock().await;

    if let Some(signature_request) = signature_requests.get_mut(&signature_type) {
      let signature = UnverifiedSignature::new_recoverable_ecdsa(signature_bytes.deref().to_vec());

      signature_request
        .add_signature(
          signature,
          self
            .inner_client
            .smart_contract_signature_verifier()
            .as_ref(),
        )
        .await
        .map_err(ErrorWrapper::from)?;
    } else {
      return Err(Error::from_reason("Signature request not found"));
    }

    Ok(())
  }

  #[napi]
  pub async fn apply_signature_requests(&self) -> Result<()> {
    let mut signature_requests = self.signature_requests.lock().await;

    let request_types: Vec<NapiSignatureRequestType> = signature_requests.keys().cloned().collect();
    for signature_request_type in request_types {
      // ignore the create inbox request since it's applied with register_identity
      if signature_request_type == NapiSignatureRequestType::CreateInbox {
        continue;
      }

      if let Some(signature_request) = signature_requests.get(&signature_request_type) {
        self
          .inner_client
          .apply_signature_request(signature_request.clone())
          .await
          .map_err(ErrorWrapper::from)?;

        // remove the signature request after applying it
        signature_requests.remove(&signature_request_type);
      }
    }

    Ok(())
  }

  #[napi]
  pub async fn set_consent_states(&self, records: Vec<NapiConsent>) -> Result<()> {
    let inner = self.inner_client.as_ref();
    let stored_records: Vec<StoredConsentRecord> =
      records.into_iter().map(StoredConsentRecord::from).collect();

    inner
      .set_consent_states(stored_records)
      .await
      .map_err(ErrorWrapper::from)?;
    Ok(())
  }

  #[napi]
  pub async fn get_consent_state(
    &self,
    entity_type: NapiConsentEntityType,
    entity: String,
  ) -> Result<NapiConsentState> {
    let inner = self.inner_client.as_ref();
    let result = inner
      .get_consent_state(entity_type.into(), entity)
      .await
      .map_err(ErrorWrapper::from)?;

    Ok(result.into())
  }
}
