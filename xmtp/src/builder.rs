use crate::{
    account::VmacAccount,
    client::{Client, Network},
    persistence::{NamespacedPersistence, Persistence},
};

#[derive(Default)]
pub struct ClientBuilder<P>
where
    P: Persistence,
{
    network: Network,
    persistence: Option<P>,
    wallet_address: Option<String>,
}

impl<P> ClientBuilder<P>
where
    P: Persistence,
{
    pub fn new() -> Self {
        Self {
            network: Network::Dev,
            persistence: None,
            wallet_address: None,
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

    pub fn wallet_address(mut self, wallet_address: String) -> Self {
        self.wallet_address = Some(wallet_address);
        self
    }

    fn find_or_create_account(
        persistence: &mut NamespacedPersistence<P>,
    ) -> Result<VmacAccount, P::Error> {
        let key = "vmac_account";
        let existing = persistence.read(key);
        match existing {
            Ok(Some(data)) => {
                let data_string = std::str::from_utf8(&data).map_err(|e| format!("{}", e))?;
                let account: VmacAccount =
                    serde_json::from_str(data_string).map_err(|e| format!("{}", e))?;
                Ok(account)
            }
            Ok(None) => {
                let account = VmacAccount::generate();
                let data = serde_json::to_string(&account).map_err(|e| format!("{}", e))?;
                persistence.write(key, data.as_bytes())?;
                Ok(account)
            }
            Err(e) => Err(e),
        }
    }

    pub fn build(&mut self) -> Result<Client<P>, P::Error> {
        let wallet_address = self
            .wallet_address
            .as_ref()
            .ok_or_else(|| "Wallet address must be set before setting the account".to_string())?;
        let persistence = self
            .persistence
            .take()
            .expect("Persistence engine must be set");
        let mut persistence =
            NamespacedPersistence::new(&get_account_namespace(wallet_address), persistence);
        let account = Self::find_or_create_account(&mut persistence)?;

        Ok(Client {
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

    use crate::{client::Network, persistence::in_memory_persistence::InMemoryPersistence};

    use super::ClientBuilder;

    impl ClientBuilder<InMemoryPersistence> {
        pub fn new_test() -> Self {
            Self::new()
                .persistence(InMemoryPersistence::new())
                .wallet_address("unknown".to_string())
        }
    }

    #[test]
    fn builder_test() {
        let client = ClientBuilder::new_test().build().unwrap();
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
            .wallet_address("foo".to_string())
            .build()
            .unwrap();

        let client_b = ClientBuilder::new()
            .persistence(client_a.persistence.persistence)
            .wallet_address("foo".to_string())
            .build()
            .unwrap();

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
            .build()
            .unwrap();
    }
}
