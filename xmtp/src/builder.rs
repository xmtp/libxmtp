use crate::storage::EncryptedMessageStore;
use crate::{
    account::{Account, AccountError},
    association::{Association, AssociationError, AssociationText},
    client::{Client, Network},
    networking::XmtpApiClient,
    types::Address,
    Errorer, InboxOwner, KeyStore,
};
use crate::{Fetch, StorageError, Store};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ClientBuilderError {
    #[error("Missing parameter: {parameter}")]
    MissingParameterError { parameter: &'static str },

    #[error("Failed to serialize/deserialize state for persistence: {source}")]
    SerializationError { source: serde_json::Error },

    #[error("Required account was not found in cache.")]
    RequiredAccountNotFound,

    #[error("Associating an address to account failed")]
    AssociationFailed(#[from] AssociationError),
    // #[error("Error Initalizing Store")]
    // StoreInitialization(#[from] SE),
    #[error("Error Initalizing Account")]
    AccountInitialization(#[from] AccountError),

    #[error("Storage Error")]
    StorageError(#[from] StorageError),
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

pub struct ClientBuilder<A, S, O>
where
    A: XmtpApiClient + Default,
    S: KeyStore,
    O: InboxOwner,
{
    api_client: Option<A>,
    network: Network,
    account: Option<Account>,
    store: Option<S>,
    account_strategy: AccountStrategy<O>,
}

impl<A, S, O> ClientBuilder<A, S, O>
where
    A: XmtpApiClient + Default,
    S: KeyStore,
    O: InboxOwner,
    S: Fetch<Account>,
{
    const ACCOUNT_KEY: &str = "xmtp_account";

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

    pub fn store(mut self, store: S) -> Self {
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
    fn find_or_create_account(owner: &O, store: &mut S) -> Result<Account, ClientBuilderError> {
        let account = Self::retrieve_peristed_account(Self::ACCOUNT_KEY, store)?;

        match account {
            Some(a) => Ok(a),
            None => {
                let new_account = Self::sign_new_account(owner)?;
                store.set_account(&new_account);
                Ok(new_account)
            }
        }
    }

    /// Fetch Account from persistence
    fn retrieve_peristed_account(
        key: &str,
        store: &mut S,
    ) -> Result<Option<Account>, ClientBuilderError> {
        let mut accounts = store.fetch()?;
        Ok(accounts.pop())
    }

    fn load_account(data: &[u8]) -> Result<Account, ClientBuilderError> {
        // TODO: use proto bytes instead of string here (or use base64 instead of utf8)
        // Remove expect() afterwards
        let data_string =
            std::str::from_utf8(data).expect("Data read from persistence is not valid UTF-8");
        let account: Account = serde_json::from_str(data_string)
            .map_err(|source| ClientBuilderError::SerializationError { source })?;
        Ok(account)
    }

    fn sign_new_account(owner: &O) -> Result<Account, ClientBuilderError> {
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
    pub fn build(mut self) -> Result<Client<A, S>, ClientBuilderError> {
        let api_client = self.api_client.take().unwrap_or_default();
        let mut store = self.store.take().unwrap_or_default();
        // Fetch the Account based upon the account strategy.
        let account = match self.account_strategy {
            AccountStrategy::CachedOnly(_) => {
                let account = Self::retrieve_peristed_account(Self::ACCOUNT_KEY, &mut store)?;
                account.ok_or(ClientBuilderError::RequiredAccountNotFound)?
            }
            AccountStrategy::CreateIfNotFound(owner) => {
                Self::find_or_create_account(&owner, &mut store)?
            }
            #[cfg(test)]
            AccountStrategy::ExternalAccount(a) => a,
        };

        Ok(Client::new(api_client, self.network, account, store))
    }
}

fn get_account_namespace(wallet_address: &str) -> String {
    format!("xmtp/account_{}", wallet_address)
}

#[cfg(test)]
mod tests {

    use ethers::signers::LocalWallet;
    use tempfile::TempPath;
    use xmtp_cryptography::utils::generate_local_wallet;

    use crate::{
        networking::MockXmtpApiClient,
        persistence::in_memory_persistence::InMemoryPersistence,
        storage::{EncryptedMessageStore, StorageOption},
        Client,
    };

    use super::ClientBuilder;

    impl ClientBuilder<MockXmtpApiClient, EncryptedMessageStore, LocalWallet> {
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
        let tmpdb = TempPath::from_path("./db.db3");
        let enckey = EncryptedMessageStore::generate_enc_key();
        let wallet = generate_local_wallet();

        let store_a = EncryptedMessageStore::new_unencrypted(StorageOption::Peristent(
            tmpdb.to_str().unwrap().into(),
        ))
        .unwrap();

        let client_a: Client<MockXmtpApiClient, EncryptedMessageStore> =
            ClientBuilder::new(wallet.clone().into())
                .store(store_a)
                .build()
                .unwrap();
        let keybytes_a = client_a.account.get_keys().curve25519.to_bytes();
        drop(client_a);

        let store_b = EncryptedMessageStore::new_unencrypted(StorageOption::Peristent(
            tmpdb.to_str().unwrap().into(),
        ))
        .unwrap();

        let client_b: Client<MockXmtpApiClient, EncryptedMessageStore> =
            ClientBuilder::new(wallet.into())
                .store(store_b)
                .build()
                .unwrap();
        let keybytes_b = client_b.account.get_keys().curve25519.to_bytes();

        // Ensure the persistence was used to store the generated keys
        assert_eq!(keybytes_a, keybytes_b);
        tmpdb.keep().unwrap();
    }
}
