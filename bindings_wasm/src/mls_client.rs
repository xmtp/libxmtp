use js_sys::Uint8Array;
use std::collections::HashMap;
use std::sync::Arc;
use wasm_bindgen::prelude::{wasm_bindgen, JsError};
use wasm_bindgen::JsValue;
use xmtp_api_http::XmtpHttpApiClient;
use xmtp_cryptography::signature::ed25519_public_key_to_address;
use xmtp_id::associations::generate_inbox_id as xmtp_id_generate_inbox_id;
use xmtp_id::associations::{
  AccountId, MemberIdentifier, RecoverableEcdsaSignature, Signature, SmartContractWalletSignature,
};
use xmtp_mls::api::ApiClientWrapper;
use xmtp_mls::builder::ClientBuilder;
use xmtp_mls::identity::IdentityStrategy;
use xmtp_mls::retry::Retry;
use xmtp_mls::storage::{EncryptedMessageStore, EncryptionKey, StorageOption};
use xmtp_mls::Client as MlsClient;

pub type RustXmtpClient = MlsClient<XmtpHttpApiClient>;

#[wasm_bindgen]
pub struct WasmClient {
  account_address: String,
  inner_client: Arc<RustXmtpClient>,
  signatures: HashMap<MemberIdentifier, Box<dyn Signature>>,
}

#[wasm_bindgen]
pub async fn create_client(
  host: String,
  inbox_id: String,
  account_address: String,
  encryption_key: Option<Uint8Array>,
  history_sync_url: Option<String>,
) -> Result<WasmClient, JsError> {
  let api_client = XmtpHttpApiClient::create(host.clone());

  let storage_option = StorageOption::Ephemeral;
  let store = match encryption_key {
    Some(key) => {
      let key: Vec<u8> = key.to_vec();
      let key: EncryptionKey = key
        .try_into()
        .map_err(|_| JsError::new("Malformed 32 byte encryption key"))?;
      EncryptedMessageStore::new(storage_option, key)
        .map_err(|_| JsError::new("Error creating encrypted message store"))?
    }
    None => EncryptedMessageStore::new_unencrypted(storage_option)
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
    signatures: HashMap::new(),
  })
}

#[wasm_bindgen]
pub async fn get_inbox_id_for_address(
  host: String,
  account_address: String,
) -> Result<Option<String>, JsError> {
  let account_address = account_address.to_lowercase();
  let api_client = ApiClientWrapper::new(XmtpHttpApiClient::create(host.clone()), Retry::default());

  let results = api_client
    .get_inbox_ids(vec![account_address.clone()])
    .await
    .map_err(|e| JsError::new(format!("{}", e).as_str()))?;

  Ok(results.get(&account_address).cloned())
}

#[wasm_bindgen]
pub fn generate_inbox_id(account_address: String) -> String {
  let account_address = account_address.to_lowercase();
  // ensure that the nonce is always 1 for now since this will only be used for the
  // create_client function above, which also has a hard-coded nonce of 1
  xmtp_id_generate_inbox_id(&account_address, &1)
}

#[wasm_bindgen]
impl WasmClient {
  #[wasm_bindgen(getter)]
  pub fn account_address(&self) -> String {
    self.account_address.clone()
  }

  #[wasm_bindgen(getter)]
  pub fn inbox_id(&self) -> String {
    self.inner_client.inbox_id()
  }

  #[wasm_bindgen(getter)]
  pub fn is_registered(&self) -> bool {
    self.inner_client.identity().signature_request().is_none()
  }

  #[wasm_bindgen(getter)]
  pub fn installation_id(&self) -> String {
    ed25519_public_key_to_address(self.inner_client.installation_public_key().as_slice())
  }

  #[wasm_bindgen]
  pub async fn can_message(&self, account_addresses: Vec<String>) -> Result<JsValue, JsError> {
    let results: HashMap<String, bool> = self
      .inner_client
      .can_message(account_addresses)
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;

    Ok(serde_wasm_bindgen::to_value(&results)?)
  }

  #[wasm_bindgen]
  pub fn add_ecdsa_signature(&mut self, signature_bytes: Uint8Array) -> Result<(), JsError> {
    if self.is_registered() {
      return Err(JsError::new(
        "An identity is already registered with this client",
      ));
    }

    let signature_text = match self.signature_text() {
      Some(text) => text,
      None => return Err(JsError::new("No signature text found")),
    };

    let signature = Box::new(RecoverableEcdsaSignature::new(
      signature_text.into(),
      signature_bytes.to_vec(),
    ));

    self.signatures.insert(
      MemberIdentifier::Address(self.account_address.clone().to_lowercase()),
      signature,
    );

    Ok(())
  }

  #[wasm_bindgen]
  pub fn add_scw_signature(
    &mut self,
    signature_bytes: Uint8Array,
    chain_id: String,
    account_address: String,
    chain_rpc_url: String,
    block_number: u64,
  ) -> Result<(), JsError> {
    if self.is_registered() {
      return Err(JsError::new(
        "An identity is already registered with this client",
      ));
    }

    let signature_text = match self.signature_text() {
      Some(text) => text,
      None => return Err(JsError::new("No signature text found")),
    };

    let account_id = AccountId::new(chain_id, account_address.clone());

    let signature = Box::new(SmartContractWalletSignature::new(
      signature_text,
      signature_bytes.to_vec(),
      account_id,
      chain_rpc_url,
      block_number,
    ));

    self.signatures.insert(
      MemberIdentifier::Address(account_address.clone().to_lowercase()),
      signature,
    );

    Ok(())
  }

  #[wasm_bindgen]
  pub async fn register_identity(&self) -> Result<(), JsError> {
    if self.is_registered() {
      return Err(JsError::new(
        "An identity is already registered with this client",
      ));
    }

    if self.signatures.is_empty() {
      return Err(JsError::new(
        "No client signatures found, add at least 1 before registering",
      ));
    }

    let mut signature_request = match self.inner_client.identity().signature_request() {
      Some(signature_req) => signature_req,
      // this should never happen since we're checking for it above in is_registered
      None => return Err(JsError::new("No signature request found")),
    };

    // apply added signatures to the signature request
    for signature in self.signatures.values() {
      signature_request
        .add_signature(signature.clone())
        .await
        .map_err(|e| JsError::new(format!("{}", e).as_str()))?;
    }

    self
      .inner_client
      .register_identity(signature_request)
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;

    Ok(())
  }

  #[wasm_bindgen]
  pub fn signature_text(&self) -> Option<String> {
    self
      .inner_client
      .identity()
      .signature_request()
      .map(|signature_req| signature_req.signature_text())
  }

  #[wasm_bindgen]
  pub async fn request_history_sync(&self) -> Result<(), JsError> {
    let _ = self
      .inner_client
      .send_history_request()
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;

    Ok(())
  }

  #[wasm_bindgen]
  pub async fn find_inbox_id_by_address(&self, address: String) -> Result<Option<String>, JsError> {
    let inbox_id = self
      .inner_client
      .find_inbox_id_from_address(address)
      .await
      .map_err(|e| JsError::new(format!("{}", e).as_str()))?;

    Ok(inbox_id)
  }
}
