use crate::{
    account::{Account, AccountError},
    association::{Association, AssociationError, AssociationText},
    client::{Client, Network},
    networking::XmtpApiClient,
    storage::EncryptedMessageStore,
    types::Address,
    InboxOwner, Store,
};
use crate::{Fetch, StorageError};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ClientBuilderError {
    #[error("Missing parameter: {parameter}")]
    MissingParameterError { parameter: &'static str },

    #[error("Failed to serialize/deserialize state for persistence: {source}")]
    SerializationError { source: serde_json::Error },

    #[error("Required account was not found in cache.")]
    RequiredAccountNotFound,

    #[error("Database was configured with a different wallet")]
    StoredAccountMismatch,

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

pub struct ClientBuilder<A, O>
where
    A: XmtpApiClient + Default,
    O: InboxOwner,
{
    api_client: Option<A>,
    network: Network,
    account: Option<Account>,
    store: Option<EncryptedMessageStore>,
    account_strategy: AccountStrategy<O>,
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

    /// Fetch account from peristence or generate and sign a new one
    fn find_or_create_account(
        owner: &O,
        store: &mut EncryptedMessageStore,
    ) -> Result<Account, ClientBuilderError> {
        let account = Self::retrieve_persisted_account(store)?;

        match account {
            Some(a) => {
                if owner.get_address() == a.addr() {
                    return Ok(a);
                }
                Err(ClientBuilderError::StoredAccountMismatch)
            }
            None => {
                let new_account = Self::sign_new_account(owner)?;
                new_account.store(store)?;
                Ok(new_account)
            }
        }
    }

    /// Fetch Account from persistence
    fn retrieve_persisted_account(
        store: &mut EncryptedMessageStore,
    ) -> Result<Option<Account>, ClientBuilderError> {
        let mut accounts = store.fetch()?;
        Ok(accounts.pop())
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
    pub fn build(mut self) -> Result<Client<A>, ClientBuilderError> {
        let api_client = self.api_client.take().unwrap_or_default();
        let mut store = self.store.take().unwrap_or_default();
        // Fetch the Account based upon the account strategy.
        let account = match self.account_strategy {
            AccountStrategy::CachedOnly(_) => {
                let account = Self::retrieve_persisted_account(&mut store)?;
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

#[cfg(test)]
mod tests {

    use ethers::signers::LocalWallet;
    use tempfile::TempPath;
    use xmtp_cryptography::utils::generate_local_wallet;

    use crate::{
        networking::MockXmtpApiClient,
        storage::{EncryptedMessageStore, StorageOption},
        Client,
    };

    use super::ClientBuilder;

    impl ClientBuilder<MockXmtpApiClient, LocalWallet> {
        pub fn new_test() -> Self {
            let wallet = generate_local_wallet();

            Self::new(wallet.into())
        }
    }

    #[test]
    fn builder_test() {
        let client = ClientBuilder::new_test().build().unwrap();
        assert!(!client
            .account
            .olm_account()
            .unwrap()
            .get()
            .identity_keys()
            .curve25519
            .to_bytes()
            .is_empty())
    }

    #[test]
    fn persistence_test() {
        let tmpdb = TempPath::from_path("./db.db3");
        let wallet = generate_local_wallet();

        // Generate a new Wallet + Store
        let store_a = EncryptedMessageStore::new_unencrypted(StorageOption::Persistent(
            tmpdb.to_str().unwrap().into(),
        ))
        .unwrap();

        let client_a: Client<MockXmtpApiClient> = ClientBuilder::new(wallet.clone().into())
            .store(store_a)
            .build()
            .unwrap();
        let keybytes_a = client_a
            .account
            .olm_account()
            .unwrap()
            .get()
            .identity_keys()
            .curve25519
            .to_bytes();
        drop(client_a);

        // Reload the existing store and wallet
        let store_b = EncryptedMessageStore::new_unencrypted(StorageOption::Persistent(
            tmpdb.to_str().unwrap().into(),
        ))
        .unwrap();

        let client_b: Client<MockXmtpApiClient> = ClientBuilder::new(wallet.into())
            .store(store_b)
            .build()
            .unwrap();
        let keybytes_b = client_b
            .account
            .olm_account()
            .unwrap()
            .get()
            .identity_keys()
            .curve25519
            .to_bytes();
        drop(client_b);

        // Create a new wallet and store
        let store_c = EncryptedMessageStore::new_unencrypted(StorageOption::Persistent(
            tmpdb.to_str().unwrap().into(),
        ))
        .unwrap();

        ClientBuilder::<MockXmtpApiClient, LocalWallet>::new(generate_local_wallet().into())
            .store(store_c)
            .build()
            .expect_err("Testing expected mismatch error");

        // Ensure the persistence was used to store the generated keys
        assert_eq!(keybytes_a, keybytes_b);
    }
}
