use thiserror::Error;

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

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("unknown client error")]
    Unknown,
}

pub struct Client<P, S>
where
    P: Persistence,
{
    pub network: Network,
    pub persistence: NamespacedPersistence<P>,
    pub account: VmacAccount,
    pub(super) _store: S,
}

impl<P, S> Client<P, S> where P: Persistence {}

impl<P, S> Client<P, S>
where
    P: Persistence,
{
    pub fn write_to_persistence(&mut self, s: &str, b: &[u8]) -> Result<(), P::Error> {
        self.persistence.write(s, b)
    }

    pub fn read_from_persistence(&self, s: &str) -> Result<Option<Vec<u8>>, P::Error> {
        self.persistence.read(s)
    }
}
