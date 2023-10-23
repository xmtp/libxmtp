use crate::StorageError;
use crate::{
    account::{Account, AccountError},
    client::{Client, Network},
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
    #[error("Required account was not found in cache.")]
    RequiredAccountNotFound,

    #[error("Database was configured with a different wallet")]
    StoredAccountMismatch,

    // #[error("Associating an address to account failed")]
    // AssociationFailed(#[from] AssociationError),
    // #[error("Error Initalizing Store")]
    // StoreInitialization(#[from] SE),
    #[error("Error Initalizing Account")]
    AccountInitialization(#[from] AccountError),

    #[error("Storage Error")]
    StorageError(#[from] StorageError),
}

pub enum AccountStrategy<Owner> {
    CreateIfNotFound(Owner),
    CachedOnly(Address),
    #[cfg(test)]
    ExternalAccount(Account),
}

impl<Owner> From<String> for AccountStrategy<Owner> {
    fn from(value: String) -> Self {
        AccountStrategy::CachedOnly(value)
    }
}

impl<Owner> From<Owner> for AccountStrategy<Owner> 
where
    Owner: InboxOwner,
{
    fn from(value: Owner) -> Self {
        AccountStrategy::CreateIfNotFound(value)
    }
}

pub struct ClientBuilder<ApiClient, Owner> {
    api_client: Option<ApiClient>,
    network: Network,
    account: Option<Account>,
    store: Option<EncryptedMessageStore>,
    account_strategy: AccountStrategy<Owner>,
}

impl<A, O> ClientBuilder<A, O>
where
    A: XmtpApiClient + Default,
    O: InboxOwner,
{
    pub fn new(strat: AccountStrategy<O>) -> Self {
        Self {
            api_client: None,
            network: Network::Dev,
            account: None,
            store: None,
            account_strategy: strat,
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

    pub fn account(mut self, account: Account) -> Self {
        self.account = Some(account);
        self
    }

    pub fn store(mut self, store: EncryptedMessageStore) -> Self {
        self.store = Some(store);
        self
    }

    pub fn build(mut self) -> Result<Client<A>, ClientBuilderError> {
        let api_client = self.api_client.take().unwrap_or_default();
        let store = self.store.take().unwrap_or_default();
        // Fetch the Account based upon the account strategy.
        let account = match self.account_strategy {
            AccountStrategy::CachedOnly(_) => {
                // TODO
                Account {}
            }
            AccountStrategy::CreateIfNotFound(_owner) => {
                // TODO
                Account {}
            }
            #[cfg(test)]
            AccountStrategy::ExternalAccount(a) => a,
        };
        Ok(Client::new(api_client, self.network, account, store))
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
