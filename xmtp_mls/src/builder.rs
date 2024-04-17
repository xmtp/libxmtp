#[cfg(test)]
use std::println as debug;

#[cfg(not(test))]
use log::debug;
use log::info;
use thiserror::Error;

use xmtp_proto::api_client::{XmtpIdentityClient, XmtpMlsClient};

use crate::{
    api::ApiClientWrapper,
    client::{Client, Network},
    identity::{Identity, IdentityError},
    retry::Retry,
    storage::{identity::StoredIdentity, EncryptedMessageStore},
    utils::address::sanitize_evm_addresses,
    xmtp_openmls_provider::XmtpOpenMlsProvider,
    Fetch, InboxOwner, StorageError,
};

#[derive(Error, Debug)]
pub enum ClientBuilderError {
    #[error("Address validation: {0}")]
    AddressValidation(#[from] crate::utils::address::AddressValidationError),

    #[error("Missing parameter: {parameter}")]
    MissingParameter { parameter: &'static str },

    // #[error("Failed to serialize/deserialize state for persistence: {source}")]
    // SerializationError { source: serde_json::Error },
    #[error("Required identity was not found in cache.")]
    RequiredIdentityNotFound,

    #[error("Database was configured with a different wallet")]
    StoredIdentityMismatch,

    // #[error("Associating an address to account failed")]
    // AssociationFailed(#[from] AssociationError),
    // #[error("Error Initializing Store")]
    // StoreInitialization(#[from] SE),
    #[error("Error initializing identity: {0}")]
    IdentityInitialization(#[from] IdentityError),

    #[error("Storage Error")]
    StorageError(#[from] StorageError),
}

/// Describes how the legacy v2 identity key was obtained, if applicable.
///
/// XMTP SDK's may embed libxmtp (v3) alongside existing v2 protocol logic
/// for backwards-compatibility purposes. In this case, the client may already
/// have a wallet-signed v2 key. Depending on the source of this key,
/// libxmtp may choose to bootstrap v3 installation keys using the existing
/// legacy key.
///
/// If the client supports v2, then the serialized bytes of the legacy
/// SignedPrivateKey proto for the v2 identity key should be provided.
pub enum LegacyIdentity {
    // A client with no support for v2 messages
    None,
    // A cached v2 key was provided on client initialization
    Static(Vec<u8>),
    // A private bundle exists on the network from which the v2 key will be fetched
    Network(Vec<u8>),
    // A new v2 key was generated on client initialization
    KeyGenerator(Vec<u8>),
}

/// Describes whether the v3 identity should be created
/// If CreateIfNotFound is chosen, the wallet account address and legacy
/// v2 identity should be specified, or set to LegacyIdentity::None if not applicable.
pub enum IdentityStrategy {
    CreateIfNotFound(String, LegacyIdentity),
    CachedOnly,
    #[cfg(test)]
    ExternalIdentity(Identity),
}

impl IdentityStrategy {
    async fn initialize_identity<ApiClient: XmtpMlsClient + XmtpIdentityClient>(
        self,
        api_client: &ApiClientWrapper<ApiClient>,
        store: &EncryptedMessageStore,
    ) -> Result<Identity, ClientBuilderError> {
        info!("Initializing identity");
        let conn = store.conn()?;
        let provider = XmtpOpenMlsProvider::new(&conn);
        let identity_option: Option<Identity> = provider
            .conn()
            .fetch(&())?
            .map(|i: StoredIdentity| i.into());
        debug!("Existing identity in store: {:?}", identity_option);
        match self {
            IdentityStrategy::CachedOnly => {
                identity_option.ok_or(ClientBuilderError::RequiredIdentityNotFound)
            }
            IdentityStrategy::CreateIfNotFound(account_address, legacy_identity) => {
                let account_address = sanitize_evm_addresses(vec![account_address])?[0].clone();
                match identity_option {
                    Some(identity) => {
                        if identity.account_address != account_address {
                            return Err(ClientBuilderError::StoredIdentityMismatch);
                        }
                        Ok(identity)
                    }
                    None => Ok(
                        Self::create_identity(api_client, account_address, legacy_identity).await?,
                    ),
                }
            }
            #[cfg(test)]
            IdentityStrategy::ExternalIdentity(identity) => Ok(identity),
        }
    }

    async fn create_identity<ApiClient: XmtpMlsClient + XmtpIdentityClient>(
        api_client: &ApiClientWrapper<ApiClient>,
        account_address: String,
        legacy_identity: LegacyIdentity,
    ) -> Result<Identity, ClientBuilderError> {
        info!("Creating identity");
        let identity = match legacy_identity {
            // This is a fresh install, and at most one v2 signature (enable_identity)
            // has been requested so far, so it's fine to request another one (grant_messaging_access).
            LegacyIdentity::None | LegacyIdentity::Network(_) => {
                Identity::create_to_be_signed(account_address)?
            }
            // This is a new XMTP user and two v2 signatures (create_identity and enable_identity)
            // have just been requested, don't request a third.
            LegacyIdentity::KeyGenerator(legacy_signed_private_key) => {
                Identity::create_from_legacy(account_address, legacy_signed_private_key)?
            }
            // This is an existing v2 install being upgraded to v3, not a fresh install.
            // Don't request a signature out of the blue if possible.
            LegacyIdentity::Static(legacy_signed_private_key) => {
                if Identity::has_existing_legacy_credential(api_client, &account_address).await? {
                    // Another installation has already derived a v3 key from this v2 key.
                    // Don't reuse the same v2 key - make a new key altogether.
                    Identity::create_to_be_signed(account_address)?
                } else {
                    Identity::create_from_legacy(account_address, legacy_signed_private_key)?
                }
            }
        };
        Ok(identity)
    }
}

// Deprecated
impl<Owner> From<&Owner> for IdentityStrategy
where
    Owner: InboxOwner,
{
    fn from(value: &Owner) -> Self {
        IdentityStrategy::CreateIfNotFound(value.get_address(), LegacyIdentity::None)
    }
}

pub struct ClientBuilder<ApiClient> {
    api_client: Option<ApiClient>,
    network: Network,
    identity: Option<Identity>,
    store: Option<EncryptedMessageStore>,
    identity_strategy: IdentityStrategy,
}

impl<ApiClient> ClientBuilder<ApiClient>
where
    ApiClient: XmtpMlsClient + XmtpIdentityClient,
{
    pub fn new(strat: IdentityStrategy) -> Self {
        Self {
            api_client: None,
            network: Network::Dev,
            identity: None,
            store: None,
            identity_strategy: strat,
        }
    }

    pub fn api_client(mut self, api_client: ApiClient) -> Self {
        self.api_client = Some(api_client);
        self
    }

    pub fn network(mut self, network: Network) -> Self {
        self.network = network;
        self
    }

    pub fn identity(mut self, identity: Identity) -> Self {
        self.identity = Some(identity);
        self
    }

    pub fn store(mut self, store: EncryptedMessageStore) -> Self {
        self.store = Some(store);
        self
    }

    pub async fn build(mut self) -> Result<Client<ApiClient>, ClientBuilderError> {
        debug!("Building client");
        let api_client = self
            .api_client
            .take()
            .ok_or(ClientBuilderError::MissingParameter {
                parameter: "api_client",
            })?;
        let api_client_wrapper = ApiClientWrapper::new(api_client, Retry::default());
        let network = self.network;
        let store = self
            .store
            .take()
            .ok_or(ClientBuilderError::MissingParameter { parameter: "store" })?;
        debug!("Initializing identity");
        let identity = self
            .identity_strategy
            .initialize_identity(&api_client_wrapper, &store)
            .await?;
        Ok(Client::new(api_client_wrapper, network, identity, store))
    }
}

#[cfg(test)]
mod tests {

    use ethers::signers::Signer;

    use xmtp_api_grpc::grpc_api_helper::Client as GrpcClient;
    use xmtp_cryptography::utils::generate_local_wallet;

    use super::{ClientBuilder, IdentityStrategy};
    use crate::{
        storage::{EncryptedMessageStore, StorageOption},
        utils::test::tmp_path,
        Client, InboxOwner,
    };

    async fn get_local_grpc_client() -> GrpcClient {
        GrpcClient::create("http://localhost:5556".to_string(), false)
            .await
            .unwrap()
    }

    impl ClientBuilder<GrpcClient> {
        pub async fn local_grpc(self) -> Self {
            self.api_client(get_local_grpc_client().await)
        }

        fn temp_store(self) -> Self {
            let tmpdb = tmp_path();
            self.store(
                EncryptedMessageStore::new_unencrypted(StorageOption::Persistent(tmpdb)).unwrap(),
            )
        }

        pub async fn new_test_client(owner: &impl InboxOwner) -> Client<GrpcClient> {
            let client = Self::new(owner.into())
                .temp_store()
                .local_grpc()
                .await
                .build()
                .await
                .unwrap();
            let signature: Option<Vec<u8>> = client
                .text_to_sign()
                .map(|text| owner.sign(&text).unwrap().into());
            client.register_identity(signature).await.unwrap();
            client
        }
    }

    #[tokio::test]
    async fn builder_test() {
        let wallet = generate_local_wallet();
        let address = wallet.address();
        let client = ClientBuilder::new_test_client(&wallet).await;
        assert!(client.account_address() == format!("{address:#020x}"));
        assert!(!client.installation_public_key().is_empty());
    }

    #[tokio::test]
    async fn identity_persistence_test() {
        let tmpdb = tmp_path();
        let wallet = &generate_local_wallet();

        // Generate a new Wallet + Store
        let store_a =
            EncryptedMessageStore::new_unencrypted(StorageOption::Persistent(tmpdb.clone()))
                .unwrap();

        let client_a = ClientBuilder::new(wallet.into())
            .local_grpc()
            .await
            .store(store_a)
            .build()
            .await
            .unwrap();
        let signature: Option<Vec<u8>> = client_a
            .text_to_sign()
            .map(|text| wallet.sign(&text).unwrap().into());
        client_a.register_identity(signature).await.unwrap(); // Persists the identity on registration
        let keybytes_a = client_a.installation_public_key();
        drop(client_a);

        // Reload the existing store and wallet
        let store_b =
            EncryptedMessageStore::new_unencrypted(StorageOption::Persistent(tmpdb.clone()))
                .unwrap();

        let client_b = ClientBuilder::new(wallet.into())
            .local_grpc()
            .await
            .store(store_b)
            .build()
            .await
            .unwrap();
        let keybytes_b = client_b.installation_public_key();
        drop(client_b);

        // Ensure the persistence was used to store the generated keys
        assert_eq!(keybytes_a, keybytes_b);

        // Create a new wallet and store
        let store_c =
            EncryptedMessageStore::new_unencrypted(StorageOption::Persistent(tmpdb.clone()))
                .unwrap();

        ClientBuilder::new((&generate_local_wallet()).into())
            .local_grpc()
            .await
            .store(store_c)
            .build()
            .await
            .expect_err("Testing expected mismatch error");

        // Use cached only strategy
        let store_d =
            EncryptedMessageStore::new_unencrypted(StorageOption::Persistent(tmpdb.clone()))
                .unwrap();
        let client_d = ClientBuilder::new(IdentityStrategy::CachedOnly)
            .local_grpc()
            .await
            .store(store_d)
            .build()
            .await
            .unwrap();
        assert_eq!(client_d.installation_public_key(), keybytes_a);
    }
}
