use crate::StorageError;
use crate::{
    client::{Client, Network},
    identity::{Identity, IdentityError},
    storage::EncryptedMessageStore,
    types::Address,
    InboxOwner,
};
use thiserror::Error;
use xmtp_proto::api_client::XmtpApiClient;

#[derive(Error, Debug)]
pub enum ClientBuilderError {
    #[error("Missing parameter: {parameter}")]
    MissingParameterError { parameter: &'static str },

    // #[error("Failed to serialize/deserialize state for persistence: {source}")]
    // SerializationError { source: serde_json::Error },
    #[error("Required identity was not found in cache.")]
    RequiredIdentityNotFound,

    #[error("Database was configured with a different wallet")]
    StoredIdentityMismatch,

    // #[error("Associating an address to account failed")]
    // AssociationFailed(#[from] AssociationError),
    // #[error("Error Initalizing Store")]
    // StoreInitialization(#[from] SE),
    #[error("Error Initalizing Identity")]
    IdentityInitialization(#[from] IdentityError),

    #[error("Storage Error")]
    StorageError(#[from] StorageError),
}

pub enum IdentityStrategy<O: InboxOwner> {
    CreateIfNotFound(O),
    CachedOnly(Address),
    #[cfg(test)]
    ExternalIdentity(Identity),
}

impl<O> From<String> for IdentityStrategy<O>
where
    O: InboxOwner,
{
    fn from(value: String) -> Self {
        IdentityStrategy::CachedOnly(value)
    }
}

impl<O> From<O> for IdentityStrategy<O>
where
    O: InboxOwner,
{
    fn from(value: O) -> Self {
        IdentityStrategy::CreateIfNotFound(value)
    }
}

pub struct ClientBuilder<A, O>
where
    A: XmtpApiClient + Default,
    O: InboxOwner,
{
    api_client: Option<A>,
    network: Network,
    identity: Option<Identity>,
    store: Option<EncryptedMessageStore>,
    identity_strategy: IdentityStrategy<O>,
}

impl<A, O> ClientBuilder<A, O>
where
    A: XmtpApiClient + Default,
    O: InboxOwner,
{
    pub fn new(strat: IdentityStrategy<O>) -> Self {
        Self {
            api_client: None,
            network: Network::Dev,
            identity: None,
            store: None,
            identity_strategy: strat,
        }
    }

    pub fn api_client(mut self, api_client: A) -> Self {
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

    pub fn build(mut self) -> Result<Client<A>, ClientBuilderError> {
        let api_client = self.api_client.take().unwrap_or_default();
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
    use xmtp_cryptography::utils::generate_local_wallet;

    use crate::mock_xmtp_api_client::MockXmtpApiClient;

    use super::ClientBuilder;

    impl ClientBuilder<MockXmtpApiClient, LocalWallet> {
        pub fn new_test() -> Self {
            let wallet = generate_local_wallet();

            Self::new(wallet.into())
        }
    }
}
