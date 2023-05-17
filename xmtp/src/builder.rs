use crate::{
    account::{Account, VmacAccount},
    client::{Client, Network},
    networking::XmtpApiClient,
    persistence::{NamespacedPersistence, Persistence},
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
}

#[derive(Default)]
pub struct ClientBuilder<A, P>
where
    A: XmtpApiClient,
    P: Persistence,
{
    api_client: Option<A>,
    network: Network,
    persistence: Option<P>,
    wallet_address: Option<String>,
    account: Option<Account>,
}

impl<A, P> ClientBuilder<A, P>
where
    A: XmtpApiClient,
    P: Persistence,
{
    pub fn new() -> Self {
        Self {
            api_client: None,
            network: Network::Dev,
            persistence: None,
            wallet_address: None,
            account: None,
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

    pub fn wallet_address(mut self, wallet_address: String) -> Self {
        self.wallet_address = Some(wallet_address);
        self
    }

    fn find_or_create_account(
        persistence: &mut NamespacedPersistence<P>,
    ) -> Result<VmacAccount, ClientBuilderError<P::Error>> {
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
                let account: VmacAccount = serde_json::from_str(data_string)
                    .map_err(|source| ClientBuilderError::SerializationError { source })?;
                Ok(account)
            }
            None => {
                let account = VmacAccount::generate();
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

    pub fn build(&mut self) -> Result<Client<A, P>, ClientBuilderError<P::Error>> {
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

        Ok(Client {
            api_client,
            network: self.network,
            persistence,
            account,
        })
    }
}

fn get_account_namespace(wallet_address: &str) -> String {
    format!("xmtp/account_{}", wallet_address)
}

#[cfg(test)]
mod tests {

    use crate::{
        builder::ClientBuilderError, client::Network, networking::MockXmtpApiClient,
        persistence::in_memory_persistence::InMemoryPersistence,
    };

    use super::ClientBuilder;

    impl ClientBuilder<MockXmtpApiClient, InMemoryPersistence> {
        pub fn new_test() -> Self {
            Self::new()
                .api_client(MockXmtpApiClient::new())
                .persistence(InMemoryPersistence::new())
                .wallet_address("unknown".to_string())
        }
    }

    #[test]
    fn builder_test() {
        let client = ClientBuilder::new_test().build().unwrap();
        assert!(!client
            .account
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
            .wallet_address("foo".to_string())
            .build()
            .unwrap();

        let client_b = ClientBuilder::new()
            .api_client(MockXmtpApiClient::new())
            .persistence(client_a.persistence.persistence)
            .wallet_address("foo".to_string())
            .build()
            .unwrap();

        // Ensure the persistence was used to store the generated keys
        assert_eq!(
            client_a.account.get().curve25519_key().to_bytes(),
            client_b.account.get().curve25519_key().to_bytes()
        )
    }

    #[test]
    fn test_error_result() {
        let e = ClientBuilder::<MockXmtpApiClient, InMemoryPersistence>::new()
            .api_client(MockXmtpApiClient::new())
            .network(Network::Dev)
            .build();
        match e {
            Err(ClientBuilderError::MissingParameterError { parameter: _ }) => {}
            _ => panic!("Should error with MissingParameterError type"),
        }
    }
}
