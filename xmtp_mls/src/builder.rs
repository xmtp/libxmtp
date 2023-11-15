#[cfg(test)]
use std::println as debug;

#[cfg(not(test))]
use log::debug;
use thiserror::Error;
use xmtp_proto::api_client::{XmtpApiClient, XmtpMlsClient};

use crate::{
    client::{Client, Network},
    identity::{Identity, IdentityError},
    storage::{identity::StoredIdentity, EncryptedMessageStore},
    xmtp_openmls_provider::XmtpOpenMlsProvider,
    Fetch, InboxOwner, StorageError,
};

#[derive(Error, Debug)]
pub enum ClientBuilderError {
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
    #[error("Error Initalizing Identity")]
    IdentityInitialization(#[from] IdentityError),

    #[error("Storage Error")]
    StorageError(#[from] StorageError),
}

pub enum IdentityStrategy<Owner> {
    CreateIfNotFound(Owner),
    CachedOnly,
    #[cfg(test)]
    ExternalIdentity(Identity),
}

impl<'a, Owner> IdentityStrategy<Owner>
where
    Owner: InboxOwner,
{
    fn initialize_identity(
        self,
        store: &EncryptedMessageStore,
        provider: &'a XmtpOpenMlsProvider,
    ) -> Result<Identity, ClientBuilderError> {
        let identity_option: Option<Identity> =
            store.conn()?.fetch(&())?.map(|i: StoredIdentity| i.into());
        debug!("Existing identity in store: {:?}", identity_option);
        match self {
            IdentityStrategy::CachedOnly => {
                identity_option.ok_or(ClientBuilderError::RequiredIdentityNotFound)
            }
            IdentityStrategy::CreateIfNotFound(owner) => match identity_option {
                Some(identity) => {
                    if identity.account_address != owner.get_address() {
                        return Err(ClientBuilderError::StoredIdentityMismatch);
                    }
                    Ok(identity)
                }
                None => Ok(Identity::new(store, provider, &owner)?),
            },
            #[cfg(test)]
            IdentityStrategy::ExternalIdentity(identity) => Ok(identity),
        }
    }
}

impl<Owner> From<Owner> for IdentityStrategy<Owner>
where
    Owner: InboxOwner,
{
    fn from(value: Owner) -> Self {
        IdentityStrategy::CreateIfNotFound(value)
    }
}

pub struct ClientBuilder<ApiClient, Owner> {
    api_client: Option<ApiClient>,
    network: Network,
    identity: Option<Identity>,
    store: Option<EncryptedMessageStore>,
    identity_strategy: IdentityStrategy<Owner>,
}

impl<ApiClient, Owner> ClientBuilder<ApiClient, Owner>
where
    ApiClient: XmtpApiClient + XmtpMlsClient,
    Owner: InboxOwner,
{
    pub fn new(strat: IdentityStrategy<Owner>) -> Self {
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

    pub fn build(mut self) -> Result<Client<ApiClient>, ClientBuilderError> {
        let api_client = self
            .api_client
            .take()
            .ok_or(ClientBuilderError::MissingParameter {
                parameter: "api_client",
            })?;
        let network = self.network;
        let store = self
            .store
            .take()
            .ok_or(ClientBuilderError::MissingParameter { parameter: "store" })?;
        let mut conn = store.conn()?;
        let provider = XmtpOpenMlsProvider::new(&mut conn);
        let identity = self
            .identity_strategy
            .initialize_identity(&store, &provider)?;
        Ok(Client::new(api_client, network, identity, store))
    }
}

#[cfg(test)]
mod tests {

    use ethers::signers::{LocalWallet, Signer, Wallet};
    use ethers_core::k256::ecdsa::SigningKey;
    use xmtp_api_grpc::grpc_api_helper::Client as GrpcClient;
    use xmtp_cryptography::utils::generate_local_wallet;

    use super::{ClientBuilder, IdentityStrategy};
    use crate::{
        storage::{EncryptedMessageStore, StorageOption},
        utils::test::tmp_path,
        Client,
    };

    async fn get_local_grpc_client() -> GrpcClient {
        GrpcClient::create("http://localhost:5556".to_string(), false)
            .await
            .unwrap()
    }

    impl ClientBuilder<GrpcClient, LocalWallet> {
        pub async fn local_grpc(self) -> Self {
            self.api_client(get_local_grpc_client().await)
        }

        fn temp_store(self) -> Self {
            let tmpdb = tmp_path();
            self.store(
                EncryptedMessageStore::new_unencrypted(StorageOption::Persistent(tmpdb)).unwrap(),
            )
        }

        pub async fn new_test_client(
            strat: IdentityStrategy<Wallet<SigningKey>>,
        ) -> Client<GrpcClient> {
            Self::new(strat)
                .temp_store()
                .local_grpc()
                .await
                .build()
                .unwrap()
        }
    }

    #[tokio::test]
    async fn builder_test() {
        let wallet = generate_local_wallet();
        let address = wallet.address();
        let client = ClientBuilder::new_test_client(wallet.into()).await;
        assert!(client.account_address() == format!("{address:#020x}"));
        assert!(!client.installation_public_key().is_empty());
    }

    #[tokio::test]
    async fn identity_persistence_test() {
        let tmpdb = tmp_path();
        let wallet = generate_local_wallet();

        // Generate a new Wallet + Store
        let store_a =
            EncryptedMessageStore::new_unencrypted(StorageOption::Persistent(tmpdb.clone()))
                .unwrap();

        let client_a = ClientBuilder::new(wallet.clone().into())
            .local_grpc()
            .await
            .store(store_a)
            .build()
            .unwrap();
        let keybytes_a = client_a.installation_public_key();
        drop(client_a);

        // Reload the existing store and wallet
        let store_b =
            EncryptedMessageStore::new_unencrypted(StorageOption::Persistent(tmpdb.clone()))
                .unwrap();

        let client_b = ClientBuilder::new(wallet.clone().into())
            .local_grpc()
            .await
            .store(store_b)
            .build()
            .unwrap();
        let keybytes_b = client_b.installation_public_key();
        drop(client_b);

        // Ensure the persistence was used to store the generated keys
        assert_eq!(keybytes_a, keybytes_b);

        // Create a new wallet and store
        let store_c =
            EncryptedMessageStore::new_unencrypted(StorageOption::Persistent(tmpdb.clone()))
                .unwrap();

        ClientBuilder::new(generate_local_wallet().into())
            .local_grpc()
            .await
            .store(store_c)
            .build()
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
            .unwrap();
        assert_eq!(client_d.installation_public_key(), keybytes_a);
    }
}
