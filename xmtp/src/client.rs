use crate::{account::VmacAccount, persistence::Persistence};

#[derive(Clone, Copy)]
pub enum Network {
    Local(&'static str),
    Dev,
    Prod,
}

impl Default for Network {
    fn default() -> Self {
        Network::Dev
    }
}

pub struct Client<P>
where
    P: Persistence,
{
    pub network: Network,
    pub persistence: P,
    pub account: VmacAccount,
}

impl<P: Persistence> Client<P> {
    pub fn write_to_persistence(&mut self, s: String, b: &[u8]) -> Result<(), String> {
        self.persistence.write(s, b)
    }

    pub fn read_from_persistence(&self, s: String) -> Result<Option<Vec<u8>>, String> {
        self.persistence.read(s)
    }
}
