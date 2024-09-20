use crate::conversations::NapiConversations;
use napi::bindgen_prelude::{BigInt, Error, Result, Uint8Array};
use napi_derive::napi;
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;
pub use xmtp_api_grpc::grpc_api_helper::Client as TonicApiClient;
use xmtp_cryptography::signature::ed25519_public_key_to_address;
use xmtp_id::associations::unverified::UnverifiedSignature;
use xmtp_id::associations::{generate_inbox_id as xmtp_id_generate_inbox_id, AssociationState};
use xmtp_id::associations::{AccountId, MemberIdentifier};
use xmtp_mls::api::ApiClientWrapper;
use xmtp_mls::builder::ClientBuilder;
use xmtp_mls::identity::IdentityStrategy;
use xmtp_mls::retry::Retry;
use xmtp_mls::storage::{EncryptedMessageStore, EncryptionKey, StorageOption};
use xmtp_mls::Client as MlsClient;

pub type RustXmtpClient = MlsClient<TonicApiClient>;

#[napi(object)]
pub struct NapiInboxState {
    pub inbox_id: String,
    pub recovery_address: String,
    pub installation_ids: Vec<String>,
    pub account_addresses: Vec<String>,
}

impl From<AssociationState> for NapiInboxState {
    fn from(state: AssociationState) -> Self {
        Self {
            inbox_id: state.inbox_id().to_string(),
            recovery_address: state.recovery_address().to_string(),
            installation_ids: state
                .installation_ids()
                .into_iter()
                .map(|id| ed25519_public_key_to_address(id.as_slice()))
                .collect(),
            account_addresses: state.account_addresses(),
        }
    }
}

#[napi]
pub struct NapiClient {
    inner_client: Arc<RustXmtpClient>,
    signatures: HashMap<MemberIdentifier, UnverifiedSignature>,
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
            .map_err(|e| Error::from_reason(format!("{}", e)))?,
        None => ClientBuilder::new(identity_strategy)
            .api_client(api_client)
            .store(store)
            .build()
            .await
            .map_err(|e| Error::from_reason(format!("{}", e)))?,
    };

    Ok(NapiClient {
        inner_client: Arc::new(xmtp_client),
        account_address,
        signatures: HashMap::new(),
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
            .map_err(|e| Error::from_reason(format!("{}", e)))?,
        Retry::default(),
    );

    let results = api_client
        .get_inbox_ids(vec![account_address.clone()])
        .await
        .map_err(|e| Error::from_reason(format!("{}", e)))?;

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
        self.inner_client.identity().signature_request().is_none()
    }

    #[napi]
    pub fn installation_id(&self) -> String {
        ed25519_public_key_to_address(self.inner_client.installation_public_key().as_slice())
    }

    #[napi]
    pub async fn can_message(
        &self,
        account_addresses: Vec<String>,
    ) -> Result<HashMap<String, bool>> {
        let results: HashMap<String, bool> = self
            .inner_client
            .can_message(account_addresses)
            .await
            .map_err(|e| Error::from_reason(format!("{}", e)))?;

        Ok(results)
    }

    #[napi]
    pub fn add_ecdsa_signature(&mut self, signature_bytes: Uint8Array) -> Result<()> {
        if self.is_registered() {
            return Err(Error::from_reason(
                "An identity is already registered with this client",
            ));
        }

        let signature =
            UnverifiedSignature::new_recoverable_ecdsa(signature_bytes.deref().to_vec());

        self.signatures.insert(
            MemberIdentifier::Address(self.account_address.clone().to_lowercase()),
            signature,
        );

        Ok(())
    }

    #[napi]
    pub fn add_scw_signature(
        &mut self,
        signature_bytes: Uint8Array,
        chain_id: BigInt,
        account_address: String,
        // TODO:nm remove this
        _chain_rpc_url: String,
        block_number: BigInt,
    ) -> Result<()> {
        if self.is_registered() {
            return Err(Error::from_reason(
                "An identity is already registered with this client",
            ));
        }

        let (_, chain_id_u64, _) = chain_id.get_u64();

        let account_id = AccountId::new_evm(chain_id_u64, account_address.clone());

        let signature = UnverifiedSignature::new_smart_contract_wallet(
            signature_bytes.deref().to_vec(),
            account_id,
            block_number.get_u64().1,
        );

        self.signatures.insert(
            MemberIdentifier::Address(account_address.clone().to_lowercase()),
            signature,
        );

        Ok(())
    }

    #[napi]
    pub async fn register_identity(&self) -> Result<()> {
        if self.is_registered() {
            return Err(Error::from_reason(
                "An identity is already registered with this client",
            ));
        }

        if self.signatures.is_empty() {
            return Err(Error::from_reason(
                "No client signatures found, add at least 1 before registering",
            ));
        }

        let mut signature_request = match self.inner_client.identity().signature_request() {
            Some(signature_req) => signature_req,
            // this should never happen since we're checking for it above in is_registered
            None => return Err(Error::from_reason("No signature request found")),
        };

        // apply added signatures to the signature request
        for signature in self.signatures.values() {
            signature_request
                .add_signature(
                    signature.clone(),
                    self.inner_client
                        .smart_contract_signature_verifier()
                        .as_ref(),
                )
                .await
                .map_err(|e| Error::from_reason(format!("{}", e)))?;
        }

        self.inner_client
            .register_identity(signature_request)
            .await
            .map_err(|e| Error::from_reason(format!("{}", e)))?;

        Ok(())
    }

    #[napi]
    pub fn signature_text(&self) -> Option<String> {
        self.inner_client
            .identity()
            .signature_request()
            .map(|signature_req| signature_req.signature_text())
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
            .map_err(|e| Error::from_reason(format!("{}", e)));

        Ok(())
    }

    #[napi]
    pub async fn find_inbox_id_by_address(&self, address: String) -> Result<Option<String>> {
        let inbox_id = self
            .inner_client
            .find_inbox_id_from_address(address)
            .await
            .map_err(|e| Error::from_reason(format!("{}", e)))?;

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
            .map_err(|e| Error::from_reason(format!("{}", e)))?;
        Ok(state.into())
    }
}
