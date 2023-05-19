use crate::{
    account::VmacAccount,
    networking::XmtpApiClient,
    persistence::{NamespacedPersistence, Persistence},
};

#[derive(Clone, Copy, Default)]
pub enum Network {
    Local(&'static str),
    #[default]
    Dev,
    Prod,
}

pub struct Client<A, P>
where
    A: XmtpApiClient,
    P: Persistence,
{
    pub api_client: A,
    pub network: Network,
    pub persistence: NamespacedPersistence<P>,
    pub account: VmacAccount,
}

impl<A: XmtpApiClient, P: Persistence> Client<A, P> {
    pub fn write_to_persistence(&mut self, s: &str, b: &[u8]) -> Result<(), P::Error> {
        self.persistence.write(s, b)
    }

    pub fn read_from_persistence(&self, s: &str) -> Result<Option<Vec<u8>>, P::Error> {
        self.persistence.read(s)
    }
}
