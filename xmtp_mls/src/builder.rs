use crate::StorageError;
use crate::{
    client::{Client, Network},
    identity::{Identity, IdentityError},
    storage::EncryptedMessageStore,
    types::Address,
    InboxOwner,
};
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
    CachedOnly(Address),
    #[cfg(test)]
    ExternalIdentity(Identity),
}

impl<Owner> From<String> for IdentityStrategy<Owner> {
    fn from(value: String) -> Self {
        IdentityStrategy::CachedOnly(value)
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
        let store = self.store.take().unwrap_or_default();
        // Fetch the Identity based upon the identity strategy.
        let identity = match self.identity_strategy {
            IdentityStrategy::CachedOnly(_) => {
                // TODO
                Identity {}
            }
            IdentityStrategy::CreateIfNotFound(_owner) => {
                // TODO
                Identity {}
            }
            #[cfg(test)]
            IdentityStrategy::ExternalIdentity(a) => a,
        };
        Ok(Client::new(api_client, self.network, identity, store))
    }
}

#[cfg(test)]
mod tests {

    use ethers::signers::LocalWallet;
    use xmtp_api_grpc::grpc_api_helper::Client as GrpcClient;
    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_proto::xmtp::message_api::v3::{KeyPackageUpload, RegisterInstallationRequest};

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
