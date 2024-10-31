use js_sys::Uint8Array;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
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

use crate::conversations::WasmConversations;
use crate::signatures::WasmSignatureRequestType;

pub type RustXmtpClient = MlsClient<XmtpHttpApiClient>;

#[wasm_bindgen]
pub struct WasmClient {
  account_address: String,
  inner_client: Arc<RustXmtpClient>,
  signature_requests: Arc<Mutex<HashMap<WasmSignatureRequestType, SignatureRequest>>>,
}

impl WasmClient {
  pub fn inner_client(&self) -> &Arc<RustXmtpClient> {
    &self.inner_client
  }

  pub fn signature_requests(
    &self,
  ) -> &Arc<Mutex<HashMap<WasmSignatureRequestType, SignatureRequest>>> {
    &self.signature_requests
  }
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

  #[wasm_bindgen(js_name = requestHistorySync)]
  pub async fn request_history_sync(&self) -> Result<(), JsError> {
    let provider = self
      .inner_client
      .mls_provider()
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;
    let _ = self
      .inner_client
      .send_history_sync_request(&provider)
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
  pub fn conversations(&self) -> WasmConversations {
    WasmConversations::new(self.inner_client.clone())
  }
}
