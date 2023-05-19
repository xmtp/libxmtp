use thiserror::Error;

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

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("unknown client error")]
    Unknown,
}

pub struct Client<A, P, S>
where
    A: XmtpApiClient,
    P: Persistence,
{
    pub api_client: A,
    pub network: Network,
    pub persistence: NamespacedPersistence<P>,
    pub account: VmacAccount,
    pub(super) _store: S,
}

impl<A, P, S> Client<A, P, S>
where
    A: XmtpApiClient,
    P: Persistence,
{
}

impl<A, P, S> Client<A, P, S>
where
    A: XmtpApiClient,
    P: Persistence,
{
    pub fn write_to_persistence(&mut self, s: &str, b: &[u8]) -> Result<(), P::Error> {
        self.persistence.write(s, b)
    }

    pub fn read_from_persistence(&self, s: &str) -> Result<Option<Vec<u8>>, P::Error> {
        self.persistence.read(s)
    }
}
