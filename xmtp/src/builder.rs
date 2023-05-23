use crate::{
    account::{Account, AccountCreator, AccountError},
    client::{Client, Network},
    networking::XmtpApiClient,
    persistence::{NamespacedPersistence, Persistence},
    Errorer,
};
use ethers::signers::{LocalWallet, Signer};
use ethers_core::utils::hash_message;
use thiserror::Error;
use xmtp_cryptography::{signature::h160addr_to_string, utils::rng};

#[derive(Error, Debug)]
pub enum ClientBuilderError<PE, SE> {
    #[error("Missing parameter: {parameter}")]
    MissingParameterError { parameter: &'static str },

    #[error("Failed to serialize/deserialize state for persistence: {source}")]
    SerializationError { source: serde_json::Error },

    #[error("Failed to read/write state to persistence: {source}")]
    PersistenceError { source: PE },

    #[error("Error Initalizing Store")]
    StoreInitialization(#[from] SE),
}

#[derive(Default)]
pub struct ClientBuilder<A, P, S>
where
    A: XmtpApiClient,
    P: Persistence,
    S: Default + Errorer,
{
    api_client: Option<A>,
    network: Network,
    persistence: Option<P>,
    wallet_address: Option<String>,
    account: Option<Account>,
    store: Option<S>,
}

impl<A, P, S> ClientBuilder<A, P, S>
where
    A: XmtpApiClient,
    P: Persistence,
    S: Default + Errorer,
{
    pub fn new() -> Self {
        Self {
            api_client: None,
            network: Network::Dev,
            persistence: None,
            wallet_address: None,
            account: None,
            store: None,
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

    pub fn wallet_address(mut self, wallet_address: &str) -> Self {
        self.wallet_address = Some(wallet_address.to_string());
        self
    }

    // Temp function to generate a full account, using a random local wallet
    fn generate_account() -> Result<Account, AccountError> {
        // TODO: Replace with real wallet signature
        let wallet = LocalWallet::new(&mut rng());
        let addr = h160addr_to_string(wallet.address());

        let ac = AccountCreator::new(addr);
        let msg = ac.text_to_sign();
        let hash = hash_message(msg);
        let sig = wallet
            .sign_hash(hash)
            .expect("Bad Signature with fake wallet");
        let account = ac.finalize(sig.to_vec())?;

        Ok(account)
    }

    pub fn store(mut self, store: S) -> Self {
        self.store = Some(store);
        self
    }

    fn find_or_create_account(
        persistence: &mut NamespacedPersistence<P>,
    ) -> Result<Account, ClientBuilderError<P::Error, S::Error>> {
        let key = "vmac_account";
        let existing = persistence
            .read(key)
            .map_err(|source| ClientBuilderError::PersistenceError { source })?;
        match existing {
            Some(data) => {
                // TODO: use proto bytes instead of string here (or use base64 instead of utf8)
                // Remove expect() afterwards
                let data_string = std::str::from_utf8(&data)
                    .expect("Data read from persistence is not valid UTF-8");
                let account: Account = serde_json::from_str(data_string)
                    .map_err(|source| ClientBuilderError::SerializationError { source })?;
                Ok(account)
            }
            None => {
                // TODO: Stop using unwrap and convert error to an appropriate type
                // Will do once we are getting real signatures
                let account = Self::generate_account().unwrap();
                // TODO: use proto bytes instead of string here (or use base64 instead of utf8)
                let data = serde_json::to_string(&account)
                    .map_err(|source| ClientBuilderError::SerializationError { source })?;
                persistence
                    .write(key, data.as_bytes())
                    .map_err(|source| ClientBuilderError::PersistenceError { source })?;
                Ok(account)
            }
        }
    }

    #[allow(clippy::type_complexity)] // TODO: Simplify return types via associated types
    pub fn build(mut self) -> Result<Client<A, P, S>, ClientBuilderError<P::Error, S::Error>> {
        let api_client =
            self.api_client
                .take()
                .ok_or(ClientBuilderError::MissingParameterError {
                    parameter: "api_client",
                })?;
        let wallet_address =
            self.wallet_address
                .as_ref()
                .ok_or(ClientBuilderError::MissingParameterError {
                    parameter: "wallet_address",
                })?;
        let persistence =
            self.persistence
                .take()
                .ok_or(ClientBuilderError::MissingParameterError {
                    parameter: "persistence",
                })?;
        let mut persistence =
            NamespacedPersistence::new(&get_account_namespace(wallet_address), persistence);
        let account = Self::find_or_create_account(&mut persistence)?;

        let store = self.store.take().unwrap_or_default();

        Ok(Client {
            api_client,
            network: self.network,
            persistence,
            account,
            _store: store,
        })
    }
}

fn get_account_namespace(wallet_address: &str) -> String {
    format!("xmtp/account_{}", wallet_address)
}

#[cfg(test)]
mod tests {

    use crate::{
        builder::ClientBuilderError,
        client::Network,
        networking::MockXmtpApiClient,
        persistence::in_memory_persistence::InMemoryPersistence,
        storage::{StorageOption, UnencryptedMessageStore},
    };

    use super::ClientBuilder;

    impl ClientBuilder<MockXmtpApiClient, InMemoryPersistence, UnencryptedMessageStore> {
        pub fn new_test() -> Self {
            Self::new()
                .api_client(MockXmtpApiClient::new())
                .persistence(InMemoryPersistence::new())
                .wallet_address("unknown")
        }
    }

    #[test]
    fn builder_test() {
        let client = ClientBuilder::new_test().build().unwrap();
        assert!(!client
            .account
            .keys
            .get()
            .identity_keys()
            .curve25519
            .to_bytes()
            .is_empty())
    }

    #[test]
    fn persistence_test() {
        let persistence = InMemoryPersistence::new();
        let client_a = ClientBuilder::new()
            .api_client(MockXmtpApiClient::new())
            .persistence(persistence)
            .store(UnencryptedMessageStore::new(StorageOption::Ephemeral).unwrap())
            .wallet_address("foo")
            .build()
            .unwrap();

        let client_b = ClientBuilder::new()
            .api_client(MockXmtpApiClient::new())
            .persistence(client_a.persistence.persistence)
            .store(UnencryptedMessageStore::new(StorageOption::Ephemeral).unwrap())
            .wallet_address("foo")
            .build()
            .unwrap();

        // Ensure the persistence was used to store the generated keys
        assert_eq!(
            client_a.account.keys.get().curve25519_key().to_bytes(),
            client_b.account.keys.get().curve25519_key().to_bytes()
        )
    }

    #[test]
    fn test_error_result() {
        let e =
            ClientBuilder::<MockXmtpApiClient, InMemoryPersistence, UnencryptedMessageStore>::new()
                .api_client(MockXmtpApiClient::new())
                .network(Network::Dev)
                .build();
        match e {
            Err(ClientBuilderError::MissingParameterError { parameter: _ }) => {}
            _ => panic!("Should error with MissingParameterError type"),
        }
    }
}
