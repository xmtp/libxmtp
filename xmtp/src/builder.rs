use crate::{
    account::VmacAccount,
    client::{Client, Network},
    persistence::Persistence,
};

#[derive(Default)]
pub struct ClientBuilder<P>
where
    P: Persistence,
{
    network: Network,
    persistence: Option<P>,
    account: Option<VmacAccount>,
}

impl<P> ClientBuilder<P>
where
    P: Persistence,
{
    pub fn new() -> Self {
        Self {
            network: Network::Dev,
            persistence: None,
            account: None,
        }
    }

    pub fn network(mut self, network: Network) -> Self {
        self.network = network;
        self
    }

    pub fn persistence(mut self, persistence: P) -> Self {
        self.persistence = Some(persistence);
        self
    }

    pub fn account(mut self, wallet_address: String) -> Result<Self, String> {
        let key = get_account_storage_key(wallet_address);
        let persistence = self.persistence.as_mut().ok_or_else(|| {
            "Persistence engine must be set before setting the account".to_string()
        })?;

        let existing = persistence.read(key.clone());
        match existing {
            Ok(Some(data)) => {
                println!("Found data in key {}", key.clone());
                let data_string = std::str::from_utf8(&data).map_err(|e| format!("{}", e))?;
                let account: VmacAccount =
                    serde_json::from_str(data_string).map_err(|e| format!("{}", e))?;
                self.account = Some(account)
            }
            Ok(None) => {
                let account = VmacAccount::generate();
                let data = serde_json::to_string(&account).map_err(|e| format!("{}", e))?;

                persistence.write(key, data.as_bytes())?;

                self.account = Some(account)
            }
            Err(e) => return Err(format!("Failed to read from persistence: {}", e)),
        }

        Ok(self)
    }

    pub fn build(self) -> Client<P> {
        Client {
            network: self.network,
            persistence: self.persistence.expect("A persistence engine must be set"),
            account: self.account.expect("An account must be set"),
        }
    }
}

pub fn get_account_storage_key(wallet_address: String) -> String {
    format!("account_{}", wallet_address)
}

#[cfg(test)]
mod tests {

    use crate::{client::Network, persistence::InMemoryPersistence};

    use super::ClientBuilder;

    impl ClientBuilder<InMemoryPersistence> {
        pub fn new_test() -> Self {
            Self::new()
                .persistence(InMemoryPersistence::new())
                .account("unknown".to_string())
                .unwrap()
        }
    }

    #[test]
    fn builder_test() {
        let client = ClientBuilder::new_test().build();
        assert!(!client
            .account
            .account
            .identity_keys()
            .curve25519
            .to_bytes()
            .is_empty())
    }

    #[test]
    fn persistence_test() {
        let persistence = InMemoryPersistence::new();
        let client_a = ClientBuilder::new()
            .persistence(persistence)
            .account("foo".to_string())
            .unwrap()
            .build();

        let client_b = ClientBuilder::new()
            .persistence(client_a.persistence)
            .account("foo".to_string())
            .unwrap()
            .build();

        // Ensure the persistence was used to store the generated keys
        assert_eq!(
            client_a.account.account.curve25519_key().to_bytes(),
            client_b.account.account.curve25519_key().to_bytes()
        )
    }

    #[test]
    #[should_panic]
    fn test_runtime_panic() {
        ClientBuilder::<InMemoryPersistence>::new()
            .network(Network::Dev)
            .build();
    }
}
