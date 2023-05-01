use crate::{
    account::VmacAccount,
    persistence::{NamespacedPersistence, Persistence},
};

#[derive(Clone, Copy, Default)]
pub enum Network {
    Local(&'static str),
    #[default]
    Dev,
    Prod,
}

pub struct Client<P>
where
    P: Persistence,
{
    pub network: Network,
    pub persistence: NamespacedPersistence<P>,
    pub account: VmacAccount,
}

impl<P: Persistence> Client<P> {
    pub fn write_to_persistence(&mut self, s: &str, b: &[u8]) -> Result<(), String> {
        self.persistence.write(s, b)
    }

    pub fn read_from_persistence(&self, s: &str) -> Result<Option<Vec<u8>>, String> {
        self.persistence.read(s)
    }
}
