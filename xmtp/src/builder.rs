use crate::{
    account::{Account, AccountError},
    association::{Association, AssociationError, AssociationText},
    client::{Client, Network},
    networking::XmtpApiClient,
    persistence::{NamespacedPersistence, Persistence},
    storage::EncryptedMessageStore,
    types::Address,
    InboxOwner,
};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ClientBuilderError<PE> {
    #[error("Missing parameter: {parameter}")]
    MissingParameterError { parameter: &'static str },

    #[error("Failed to serialize/deserialize state for persistence: {source}")]
    SerializationError { source: serde_json::Error },

    #[error("Failed to read/write state to persistence: {source}")]
    PersistenceError { source: PE },

    #[error("Required account was not found in cache.")]
    RequiredAccountNotFound,

    #[error("Associating an address to account failed")]
    AssociationFailed(#[from] AssociationError),
    // #[error("Error Initalizing Store")]
    // StoreInitialization(#[from] SE),
    #[error("Error Initalizing Account")]
    AccountInitialization(#[from] AccountError),
}

pub enum AccountStrategy<O: InboxOwner> {
    CreateIfNotFound(O),
    CachedOnly(Address),
    #[cfg(test)]
    ExternalAccount(Account),
}

impl<O> From<String> for AccountStrategy<O>
where
    O: InboxOwner,
{
    fn from(value: String) -> Self {
        AccountStrategy::CachedOnly(value)
    }
}

impl<O> From<O> for AccountStrategy<O>
where
    O: InboxOwner,
{
    fn from(value: O) -> Self {
        AccountStrategy::CreateIfNotFound(value)
    }
}

pub struct ClientBuilder<A, P, O>
where
    A: XmtpApiClient + Default,
    P: Persistence + Default,
    O: InboxOwner,
{
    api_client: Option<A>,
    network: Network,
    persistence: Option<P>,
    account: Option<Account>,
    store: Option<EncryptedMessageStore>,
    account_strategy: AccountStrategy<O>,
}

impl<A, P, O> ClientBuilder<A, P, O>
where
    A: XmtpApiClient + Default,
    P: Persistence + Default,
    O: InboxOwner,
{
    const ACCOUNT_KEY: &str = "xmtp_account";

    pub fn new(strat: AccountStrategy<O>) -> Self {
        Self {
            api_client: None,
            network: Network::Dev,
            persistence: None,

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

    pub fn persistence(mut self, persistence: P) -> Self {
        self.persistence = Some(persistence);
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

    fn get_address(&self) -> Address {
        match &self.account_strategy {
            AccountStrategy::CachedOnly(a) => a.clone(),
            AccountStrategy::CreateIfNotFound(o) => o.get_address(),
            #[cfg(test)]
            AccountStrategy::ExternalAccount(e) => e.addr(),
        }
    }

    /// Fetch account from peristence or generate and sign a new one
    fn find_or_create_account(
        owner: &O,
        persistence: &mut NamespacedPersistence<P>,
    ) -> Result<Account, ClientBuilderError<P::Error>> {
        let account = Self::retrieve_peristed_account(Self::ACCOUNT_KEY, persistence)?;

        match account {
            Some(a) => Ok(a),
            None => {
                let new_account = Self::sign_new_account(owner)?;
                Self::persist_account(&new_account, persistence)?;
                Ok(new_account)
            }
        }
    }

    /// Save Account to persistence
    fn persist_account(
        account: &Account,
        persistence: &mut NamespacedPersistence<P>,
    ) -> Result<(), ClientBuilderError<P::Error>> {
        // TODO: use proto bytes instead of string here (or use base64 instead of utf8)
        let data = serde_json::to_string(account)
            .map_err(|source| ClientBuilderError::SerializationError { source })?;
        persistence
            .write(Self::ACCOUNT_KEY, data.as_bytes())
            .map_err(|source| ClientBuilderError::PersistenceError { source })?;
        Ok(())
    }

    /// Fetch Account from persistence
    fn retrieve_peristed_account(
        key: &str,
        persistence: &mut NamespacedPersistence<P>,
    ) -> Result<Option<Account>, ClientBuilderError<P::Error>> {
        let res = persistence
            .read(key)
            .map_err(|source| ClientBuilderError::PersistenceError { source })?;

        match res {
            None => Ok(None),
            Some(v) => Ok(Some(Self::load_account(&v)?)),
        }
    }

    fn load_account(data: &[u8]) -> Result<Account, ClientBuilderError<P::Error>> {
        // TODO: use proto bytes instead of string here (or use base64 instead of utf8)
        // Remove expect() afterwards
        let data_string =
            std::str::from_utf8(data).expect("Data read from persistence is not valid UTF-8");
        let account: Account = serde_json::from_str(data_string)
            .map_err(|source| ClientBuilderError::SerializationError { source })?;
        Ok(account)
    }

    fn sign_new_account(owner: &O) -> Result<Account, ClientBuilderError<P::Error>> {
        let sign = |public_key_bytes: Vec<u8>| -> Result<Association, AssociationError> {
            let assoc_text = AssociationText::Static {
                addr: owner.get_address(),
                account_public_key: public_key_bytes.clone(),
            };

            let signature = owner.sign(assoc_text.clone())?;

            Association::new(public_key_bytes.as_slice(), assoc_text, signature)
        };

        Account::generate(sign).map_err(ClientBuilderError::AccountInitialization)
    }
    pub fn build(mut self) -> Result<Client<A, P>, ClientBuilderError<P::Error>> {
        let api_client = self.api_client.take().unwrap_or_default();
        let wallet_address = self.get_address();
        let persistence = self.persistence.take().unwrap_or_default();
        let mut persistence =
            NamespacedPersistence::new(&get_account_namespace(&wallet_address), persistence);

        // Fetch the Account based upon the account strategy.
        let account = match self.account_strategy {
            AccountStrategy::CachedOnly(_) => {
                let account = Self::retrieve_peristed_account(Self::ACCOUNT_KEY, &mut persistence)?;
                account.ok_or(ClientBuilderError::RequiredAccountNotFound)?
            }
            AccountStrategy::CreateIfNotFound(owner) => {
                Self::find_or_create_account(&owner, &mut persistence)?
            }
            #[cfg(test)]
            AccountStrategy::ExternalAccount(a) => a,
        };

        let store = self.store.take().unwrap_or_default();

        Ok(Client::new(
            api_client,
            self.network,
            persistence,
            account,
            store,
        ))
    }
}

fn get_account_namespace(wallet_address: &str) -> String {
    format!("xmtp/account_{}", wallet_address)
}

#[cfg(test)]
mod tests {

    use ethers::signers::LocalWallet;
    use xmtp_cryptography::utils::generate_local_wallet;

    use crate::{
        networking::MockXmtpApiClient, persistence::in_memory_persistence::InMemoryPersistence,
        Client,
    };

    use super::ClientBuilder;

    impl ClientBuilder<MockXmtpApiClient, InMemoryPersistence, LocalWallet> {
        pub fn new_test() -> Self {
            let wallet = generate_local_wallet();

            Self::new(wallet.into())
        }
    }

    #[test]
    fn builder_test() {
        let client = ClientBuilder::new_test().build().unwrap();
        assert!(!client.account.get_keys().curve25519.to_bytes().is_empty())
    }

    #[test]
    fn persistence_test() {
        let persistence = InMemoryPersistence::new();

        let wallet = generate_local_wallet();

        let client_a: Client<MockXmtpApiClient, InMemoryPersistence> =
            ClientBuilder::new(wallet.clone().into())
                .persistence(persistence)
                .build()
                .unwrap();

        let client_b: Client<MockXmtpApiClient, InMemoryPersistence> =
            ClientBuilder::new(wallet.into())
                .persistence(client_a.persistence.persistence)
                .build()
                .unwrap();
        // Ensure the persistence was used to store the generated keys
        assert_eq!(
            client_a.account.get_keys().curve25519.to_bytes(),
            client_b.account.get_keys().curve25519.to_bytes()
        )
    }
}
