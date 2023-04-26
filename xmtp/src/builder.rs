use crate::{
    client::{Client, Network},
    persistence::{InMemoryPersistence, Persistence},
};

#[derive(Default)]
pub struct ClientBuilder<P>
where
    P: Persistence,
{
    network: Network,
    persistence: Option<P>,
}

impl<P> ClientBuilder<P>
where
    P: Persistence,
{
    pub fn new() -> Self {
        Self {
            network: Network::Dev,
            persistence: None,
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

    pub fn build(self) -> Client<P> {
        Client {
            network: self.network,
            persistence: self.persistence.expect("A persistence engine must be set"),
        }
    }
}

#[cfg(test)]
mod tests {

    use crate::{client::Network, persistence::InMemoryPersistence};

    use super::ClientBuilder;

    impl ClientBuilder<InMemoryPersistence> {
        pub fn new_test() -> Self {
            Self::new().persistence(InMemoryPersistence::new())
        }
    }

    #[test]
    fn builder_test() {
        ClientBuilder::new_test().build();
    }

    #[test]
    #[should_panic]
    fn test_runtime_panic() {
        ClientBuilder::<InMemoryPersistence>::new()
            .network(Network::Dev)
            .build();
    }
}
