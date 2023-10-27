use crate::configuration::CIPHERSUITE;
use crate::storage::StoredIdentity;
use crate::xmtp_openmls_provider::XmtpOpenMlsProvider;
use crate::{
    client::{Client, Network},
    identity::{Identity, IdentityError},
    storage::EncryptedMessageStore,
    InboxOwner,
};
use crate::{Fetch, StorageError};
use thiserror::Error;
use xmtp_proto::api_client::{XmtpApiClient, XmtpMlsClient};

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
        let store = self.store.take().unwrap_or_default();
        let provider = XmtpOpenMlsProvider::new(&store);
        let identity = self.initialize_identity(&store, &provider)?;
        Ok(Client::new(api_client, network, identity, store))
    }

    fn initialize_identity(
        self,
        store: &EncryptedMessageStore,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<Identity, ClientBuilderError> {
        let conn = &mut store.conn()?;
        let identity_option: Option<Identity> = conn.fetch(())?.map(|i: StoredIdentity| i.into());
        match self.identity_strategy {
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
                None => Ok(Identity::new(CIPHERSUITE, &provider, &owner)?),
            },
            #[cfg(test)]
            IdentityStrategy::ExternalIdentity(identity) => Ok(identity),
        }
    }
}

#[cfg(test)]
mod tests {

    use ethers::signers::LocalWallet;
    use xmtp_api_grpc::grpc_api_helper::Client as GrpcClient;
    use xmtp_cryptography::utils::generate_local_wallet;

    use super::ClientBuilder;

    impl ClientBuilder<GrpcClient, LocalWallet> {
        pub async fn new_test() -> Self {
            let wallet = generate_local_wallet();
            let grpc_client = GrpcClient::create("http://localhost:5556".to_string(), false)
                .await
                .unwrap();

            Self::new(wallet.into()).api_client(grpc_client)
        }
    }

    #[tokio::test]
    async fn test_mls() {
        let client = ClientBuilder::new_test().await.build().unwrap();

        let result = client.api_client.register_installation(&[1, 2, 3]).await;

        assert!(result.is_err());
        let error_string = result.err().unwrap().to_string();
        assert!(error_string.contains("invalid identity"));
    }
}
