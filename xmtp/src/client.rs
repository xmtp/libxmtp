use thiserror::Error;

use crate::{
    account::VmacAccount,
    persistence::{NamespacedPersistence, Persistence},
    types::Message,
    MessageReceivedHookType,
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

pub struct Client<'c, P>
where
    P: Persistence,
{
    pub network: Network,
    pub persistence: NamespacedPersistence<P>,
    pub account: VmacAccount,
    pub(super) message_hook: Box<MessageReceivedHookType!(dyn, 'c)>,
}

impl<'c, P> Client<'c, P>
where
    P: Persistence,
{
    pub fn set_message_hook(&mut self, hook: MessageReceivedHookType!(impl,'c)) {
        self.message_hook = Box::new(hook);
    }

    pub(super) fn fire_message_hook(&mut self, msg: Message) -> Result<(), ClientError> {
        (self.message_hook)(msg);
        Ok(())
    }
}

impl<'c, P: Persistence> Client<'c, P> {
    pub fn write_to_persistence(&mut self, s: &str, b: &[u8]) -> Result<(), P::Error> {
        self.persistence.write(s, b)
    }

    pub fn read_from_persistence(&self, s: &str) -> Result<Option<Vec<u8>>, P::Error> {
        self.persistence.read(s)
    }
}
