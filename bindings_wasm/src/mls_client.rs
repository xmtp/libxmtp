use js_sys::Uint8Array;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use wasm_bindgen::prelude::{wasm_bindgen, JsError};
use wasm_bindgen::JsValue;
use xmtp_api_http::XmtpHttpApiClient;
use xmtp_cryptography::signature::ed25519_public_key_to_address;
use xmtp_id::associations::builder::SignatureRequest;
use xmtp_id::associations::unverified::UnverifiedSignature;
use xmtp_mls::builder::ClientBuilder;
use xmtp_mls::identity::IdentityStrategy;
use xmtp_mls::storage::consent_record::StoredConsentRecord;
use xmtp_mls::storage::{EncryptedMessageStore, EncryptionKey, StorageOption};
use xmtp_mls::Client as MlsClient;

use crate::consent_state::{WasmConsent, WasmConsentEntityType, WasmConsentState};
use crate::conversations::WasmConversations;
use crate::inbox_state::WasmInboxState;

pub type RustXmtpClient = MlsClient<XmtpHttpApiClient>;

#[wasm_bindgen]
#[derive(Clone, Eq, Hash, PartialEq)]
pub enum WasmSignatureRequestType {
  AddWallet,
  CreateInbox,
  RevokeWallet,
  RevokeInstallations,
}

#[wasm_bindgen]
pub struct WasmClient {
  account_address: String,
  inner_client: Arc<RustXmtpClient>,
  signature_requests: Arc<Mutex<HashMap<WasmSignatureRequestType, SignatureRequest>>>,
}

#[wasm_bindgen(js_name = createClient)]
pub async fn create_client(
  host: String,
  inbox_id: String,
  account_address: String,
  db_path: String,
  encryption_key: Option<Uint8Array>,
  history_sync_url: Option<String>,
) -> Result<WasmClient, JsError> {
  xmtp_mls::utils::wasm::init().await;
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

  Ok(WasmClient {
    account_address,
    inner_client: Arc::new(xmtp_client),
    signature_requests: Arc::new(Mutex::new(HashMap::new())),
  })
}

#[wasm_bindgen]
impl WasmClient {
  #[wasm_bindgen(getter, js_name = accountAddress)]
  pub fn account_address(&self) -> String {
    self.account_address.clone()
  }

  #[wasm_bindgen(getter, js_name = inboxId)]
  pub fn inbox_id(&self) -> String {
    self.inner_client.inbox_id()
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
      .can_message(account_addresses)
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

    let mut signature_requests = self.signature_requests.lock().map_err(|e| {
      JsError::new(format!("Failed to lock signature requests mutex: {}", e).as_str())
    })?;

    let signature_request = signature_requests
      .get(&WasmSignatureRequestType::CreateInbox)
      .ok_or(JsError::new("No signature request found"))?;

    self
      .inner_client
      .register_identity(signature_request.clone())
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;

    signature_requests.remove(&WasmSignatureRequestType::CreateInbox);

    Ok(())
  }

  #[wasm_bindgen(js_name = createInboxSignatureText)]
  pub async fn create_inbox_signature_text(&self) -> Result<Option<String>, JsError> {
    let signature_request = match self.inner_client.identity().signature_request() {
      Some(signature_req) => signature_req,
      // this should never happen since we're checking for it above in is_registered
      None => return Err(JsError::new("No signature request found")),
    };
    let signature_text = signature_request.signature_text();
    let mut signature_requests = self.signature_requests.lock().map_err(|e| {
      JsError::new(format!("Failed to lock signature requests mutex: {}", e).as_str())
    })?;

    signature_requests.insert(WasmSignatureRequestType::CreateInbox, signature_request);

    Ok(Some(signature_text))
  }

  #[wasm_bindgen(js_name = addWalletSignatureText)]
  pub async fn add_wallet_signature_text(
    &self,
    existing_wallet_address: String,
    new_wallet_address: String,
  ) -> Result<String, JsError> {
    let signature_request = self
      .inner_client
      .associate_wallet(
        existing_wallet_address.to_lowercase(),
        new_wallet_address.to_lowercase(),
      )
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;
    let signature_text = signature_request.signature_text();
    let mut signature_requests = self.signature_requests.lock().map_err(|e| {
      JsError::new(format!("Failed to lock signature requests mutex: {}", e).as_str())
    })?;

    signature_requests.insert(WasmSignatureRequestType::AddWallet, signature_request);

    Ok(signature_text)
  }

  #[wasm_bindgen(js_name = revokeWalletSignatureText)]
  pub async fn revoke_wallet_signature_text(
    &self,
    wallet_address: String,
  ) -> Result<String, JsError> {
    let signature_request = self
      .inner_client
      .revoke_wallets(vec![wallet_address.to_lowercase()])
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;
    let signature_text = signature_request.signature_text();
    let mut signature_requests = self.signature_requests.lock().map_err(|e| {
      JsError::new(format!("Failed to lock signature requests mutex: {}", e).as_str())
    })?;

    signature_requests.insert(WasmSignatureRequestType::RevokeWallet, signature_request);

    Ok(signature_text)
  }

  #[wasm_bindgen(js_name = revokeInstallationsSignatureText)]
  pub async fn revoke_installations_signature_text(&self) -> Result<String, JsError> {
    let installation_id = self.inner_client.installation_public_key();
    let inbox_state = self
      .inner_client
      .inbox_state(true)
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;
    let other_installation_ids = inbox_state
      .installation_ids()
      .into_iter()
      .filter(|id| id != &installation_id)
      .collect();
    let signature_request = self
      .inner_client
      .revoke_installations(other_installation_ids)
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;
    let signature_text = signature_request.signature_text();
    let mut signature_requests = self.signature_requests.lock().map_err(|e| {
      JsError::new(format!("Failed to lock signature requests mutex: {}", e).as_str())
    })?;

    signature_requests.insert(
      WasmSignatureRequestType::RevokeInstallations,
      signature_request,
    );

    Ok(signature_text)
  }

  #[wasm_bindgen(js_name = addSignature)]
  pub async fn add_signature(
    &self,
    signature_type: WasmSignatureRequestType,
    signature_bytes: Uint8Array,
  ) -> Result<(), JsError> {
    let mut signature_requests = self.signature_requests.lock().map_err(|e| {
      JsError::new(format!("Failed to lock signature requests mutex: {}", e).as_str())
    })?;

    if let Some(signature_request) = signature_requests.get_mut(&signature_type) {
      let signature = UnverifiedSignature::new_recoverable_ecdsa(signature_bytes.to_vec());

      signature_request
        .add_signature(
          signature,
          self
            .inner_client
            .smart_contract_signature_verifier()
            .as_ref(),
        )
        .await
        .map_err(|e| JsError::new(format!("{}", e).as_str()))?;
    } else {
      return Err(JsError::new("Signature request not found"));
    }

    Ok(())
  }

  #[wasm_bindgen(js_name = applySignatureRequests)]
  pub async fn apply_signature_requests(&self) -> Result<(), JsError> {
    let mut signature_requests = self.signature_requests.lock().map_err(|e| {
      JsError::new(format!("Failed to lock signature requests mutex: {}", e).as_str())
    })?;

    let request_types: Vec<WasmSignatureRequestType> = signature_requests.keys().cloned().collect();
    for signature_request_type in request_types {
      // ignore the create inbox request since it's applied with register_identity
      if signature_request_type == WasmSignatureRequestType::CreateInbox {
        continue;
      }

      if let Some(signature_request) = signature_requests.get(&signature_request_type) {
        self
          .inner_client
          .apply_signature_request(signature_request.clone())
          .await
          .map_err(|e| JsError::new(format!("{}", e).as_str()))?;

        // remove the signature request after applying it
        signature_requests.remove(&signature_request_type);
      }
    }

    Ok(())
  }

  #[wasm_bindgen(js_name = requestHistorySync)]
  pub async fn request_history_sync(&self) -> Result<(), JsError> {
    let _ = self
      .inner_client
      .send_history_request()
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

  /**
   * Get the client's inbox state.
   *
   * If `refresh_from_network` is true, the client will go to the network first to refresh the state.
   * Otherwise, the state will be read from the local database.
   */
  #[wasm_bindgen(js_name = inboxState)]
  pub async fn inbox_state(&self, refresh_from_network: bool) -> Result<WasmInboxState, JsError> {
    let state = self
      .inner_client
      .inbox_state(refresh_from_network)
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;
    Ok(state.into())
  }

  #[wasm_bindgen(js_name = getLatestInboxState)]
  pub async fn get_latest_inbox_state(&self, inbox_id: String) -> Result<WasmInboxState, JsError> {
    let conn = self
      .inner_client
      .store()
      .conn()
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;
    let state = self
      .inner_client
      .get_latest_association_state(&conn, &inbox_id)
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;
    Ok(state.into())
  }

  #[wasm_bindgen(js_name = setConsentStates)]
  pub async fn set_consent_states(&self, records: Vec<WasmConsent>) -> Result<(), JsError> {
    let inner = self.inner_client.as_ref();
    let stored_records: Vec<StoredConsentRecord> =
      records.into_iter().map(StoredConsentRecord::from).collect();

    inner
      .set_consent_states(stored_records)
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;
    Ok(())
  }

  #[wasm_bindgen(js_name = getConsentState)]
  pub async fn get_consent_state(
    &self,
    entity_type: WasmConsentEntityType,
    entity: String,
  ) -> Result<WasmConsentState, JsError> {
    let inner = self.inner_client.as_ref();
    let result = inner
      .get_consent_state(entity_type.into(), entity)
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;

    Ok(result.into())
  }

  #[wasm_bindgen]
  pub fn conversations(&self) -> WasmConversations {
    WasmConversations::new(self.inner_client.clone())
  }
}
