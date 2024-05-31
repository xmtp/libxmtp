use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;

use crate::conversations::NapiConversations;
use napi::bindgen_prelude::{Error, Result, Uint8Array};
use napi_derive::napi;
use xmtp_api_grpc::grpc_api_helper::Client as TonicApiClient;
use xmtp_cryptography::signature::ed25519_public_key_to_address;
use xmtp_id::associations::{Erc1271Signature, RecoverableEcdsaSignature};
use xmtp_mls::builder::ClientBuilder;
use xmtp_mls::identity::IdentityStrategy;
use xmtp_mls::storage::{EncryptedMessageStore, EncryptionKey, StorageOption};
use xmtp_mls::Client as MlsClient;

pub type RustXmtpClient = MlsClient<TonicApiClient>;

#[napi]
pub struct NapiClient {
  inner_client: Arc<RustXmtpClient>,
  pub account_address: String,
}

#[napi]
pub async fn create_client(
  host: String,
  is_secure: bool,
  db_path: String,
  account_address: String,
  encryption_key: Option<Uint8Array>,
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

  let identity_strategy =
    IdentityStrategy::CreateIfNotFound(account_address.clone().to_lowercase(), None);

  let xmtp_client = ClientBuilder::new(identity_strategy)
    .api_client(api_client)
    .store(store)
    .build()
    .await
    .map_err(|e| Error::from_reason(format!("{}", e)))?;

  Ok(NapiClient {
    inner_client: Arc::new(xmtp_client),
    account_address,
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
    self.inner_client.identity().signature_request().is_none()
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
      .map_err(|e| Error::from_reason(format!("{}", e)))?;

    Ok(results)
  }

  #[napi]
  pub async fn register_ecdsa_identity(&self, signature_bytes: Uint8Array) -> Result<()> {
    if self.is_registered() {
      return Err(Error::from_reason(
        "An identity is already registered with this client",
      ));
    }

    let mut signature_request = match self.inner_client.identity().signature_request() {
      Some(signature_req) => signature_req,
      None => return Err(Error::from_reason("No signature request found")),
    };
    let signature_text = match self.signature_text() {
      Some(text) => text,
      None => return Err(Error::from_reason("No signature text found")),
    };

    let signature = Box::new(RecoverableEcdsaSignature::new(
      signature_text,
      signature_bytes.deref().to_vec(),
    ));

    signature_request
      .add_signature(signature)
      .await
      .map_err(|e| Error::from_reason(format!("{}", e)))?;

    self
      .inner_client
      .register_identity(signature_request)
      .await
      .map_err(|e| Error::from_reason(format!("{}", e)))?;

    return Ok(());
  }

  #[napi]
  pub async fn register_erc1271_identity(
    &self,
    signature_bytes: Uint8Array,
    chain_rpc_url: String,
  ) -> Result<()> {
    if self.is_registered() {
      return Err(Error::from_reason(
        "An identity is already registered with this client",
      ));
    }

    let mut signature_request = match self.inner_client.identity().signature_request() {
      Some(signature_req) => signature_req,
      None => return Err(Error::from_reason("No signature request found")),
    };
    let signature_text = match self.signature_text() {
      Some(text) => text,
      None => return Err(Error::from_reason("No signature text found")),
    };

    let signature = Box::new(
      Erc1271Signature::new_with_rpc(
        signature_text,
        signature_bytes.deref().to_vec(),
        self.account_address.clone(),
        chain_rpc_url,
      )
      .await
      .map_err(|e| Error::from_reason(format!("{}", e)))?,
    );

    signature_request
      .add_signature(signature)
      .await
      .map_err(|e| Error::from_reason(format!("{}", e)))?;

    self
      .inner_client
      .register_identity(signature_request)
      .await
      .map_err(|e| Error::from_reason(format!("{}", e)))?;

    return Ok(());
  }

  #[napi]
  pub fn signature_text(&self) -> Option<String> {
    match self.inner_client.identity().signature_request() {
      Some(signature_req) => Some(signature_req.signature_text()),
      None => None,
    }
  }

  #[napi]
  pub fn release_db_connection(&self) -> Result<()> {
    Ok(
      self
        .inner_client
        .release_db_connection()
        .map_err(|e| Error::from_reason(format!("{}", e)))?,
    )
  }

  #[napi]
  pub async fn db_reconnect(&self) -> Result<()> {
    Ok(
      self
        .inner_client
        .reconnect_db()
        .map_err(|e| Error::from_reason(format!("{}", e)))?,
    )
  }

  #[napi]
  pub fn conversations(&self) -> NapiConversations {
    NapiConversations::new(self.inner_client.clone())
  }
}
