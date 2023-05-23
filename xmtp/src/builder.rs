use crate::{
    account::{Account, AccountError},
    association::{Association, AssociationError, AssociationText},
    client::{Client, Network},
    networking::XmtpApiClient,
    persistence::{NamespacedPersistence, Persistence},
    Errorer, InboxOwner,
};
use ethers::signers::{LocalWallet, Signer};
use ethers_core::utils::hash_message;
use thiserror::Error;
use xmtp_cryptography::{signature::h160addr_to_string, utils::rng};

#[derive(Error, Debug)]
pub enum ClientBuilderError<PE> {
    #[error("Missing parameter: {parameter}")]
    MissingParameterError { parameter: &'static str },

    #[error("Failed to serialize/deserialize state for persistence: {source}")]
    SerializationError { source: serde_json::Error },

    #[error("Failed to read/write state to persistence: {source}")]
    PersistenceError { source: PE },

    // #[error("Error Initalizing Store")]
    // StoreInitialization(#[from] SE),
    #[error("Error Initalizing Account")]
    AccountInitialization(#[from] AccountError),
}

pub struct ClientBuilder<A, P, S, O>
where
    A: XmtpApiClient + Default,
    P: Persistence + Default,
    S: Default + Errorer,
    O: InboxOwner,
{
    api_client: Option<A>,
    network: Network,
    persistence: Option<P>,
    wallet_address: String,
    account: Option<Account>,
    store: Option<S>,
    owner: O,
}

impl<A, P, S, O> ClientBuilder<A, P, S, O>
where
    A: XmtpApiClient + Default,
    P: Persistence + Default,
    S: Default + Errorer,
    O: InboxOwner,
{
    pub fn new(owner: O) -> Self {
        let wallet_address = owner.get_address();
        Self {
            api_client: None,
            network: Network::Dev,
            persistence: None,
            wallet_address,
            account: None,
            store: None,
            owner,
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
        &self,
        persistence: &mut NamespacedPersistence<P>,
    ) -> Result<Account, ClientBuilderError<P::Error>> {
        let key = "xmtp_account";
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
                let account = self.create_new_account()?;
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

    fn create_new_account(&self) -> Result<Account, AccountError> {
        let sign = |public_key_bytes: Vec<u8>| -> Result<Association, AssociationError> {
            let assoc_text = AssociationText::Static {
                addr: self.wallet_address.clone(),
                account_public_key: public_key_bytes.clone(),
            };

            let signature = self.owner.sign(assoc_text.clone())?;

            Association::new(public_key_bytes.as_slice(), assoc_text, signature)
        };

        Account::generate(sign)
    }
    pub fn build(mut self) -> Result<Client<A, P, S>, ClientBuilderError<P::Error>> {
        let api_client = self.api_client.take().unwrap_or_default();
        let wallet_address = self.owner.get_address();
        let persistence = self.persistence.take().unwrap_or_default();
        let mut persistence =
            NamespacedPersistence::new(&get_account_namespace(&wallet_address), persistence);

        let account = self.find_or_create_account(&mut persistence)?;

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

    use ethers::signers::LocalWallet;
    use xmtp_cryptography::utils::generate_local_wallet;

    use crate::{
        networking::MockXmtpApiClient, persistence::in_memory_persistence::InMemoryPersistence,
        storage::UnencryptedMessageStore, Client,
    };

    use super::ClientBuilder;

    impl ClientBuilder<MockXmtpApiClient, InMemoryPersistence, UnencryptedMessageStore, LocalWallet> {
        pub fn new_test() -> Self {
            let wallet = generate_local_wallet();

            Self::new(wallet)
                .api_client(MockXmtpApiClient::new())
                .persistence(InMemoryPersistence::new())
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

        let client_a: Client<MockXmtpApiClient, InMemoryPersistence, UnencryptedMessageStore> =
            ClientBuilder::new(wallet.clone())
                .api_client(MockXmtpApiClient::new())
                .persistence(persistence)
                .build()
                .unwrap();

        let client_b: Client<MockXmtpApiClient, InMemoryPersistence, UnencryptedMessageStore> =
            ClientBuilder::new(wallet)
                .api_client(MockXmtpApiClient::new())
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
