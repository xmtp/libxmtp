use thiserror::Error;

use crate::{
    identity::Identity,
    storage::{EncryptedMessageStore, StorageError},
};
use xmtp_proto::api_client::XmtpApiClient;

#[derive(Clone, Copy, Default, Debug)]
pub enum Network {
    Local(&'static str),
    #[default]
    Dev,
    Prod,
}

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("could not publish: {0}")]
    PublishError(String),
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("dieselError: {0}")]
    Ddd(#[from] diesel::result::Error),
    #[error("Query failed: {0}")]
    QueryError(#[from] xmtp_proto::api_client::Error),
    #[error("generic:{0}")]
    Generic(String),
}

impl From<String> for ClientError {
    fn from(value: String) -> Self {
        Self::Generic(value)
    }
}

impl From<&str> for ClientError {
    fn from(value: &str) -> Self {
        Self::Generic(value.to_string())
    }
}

pub struct Client<A>
where
    A: XmtpApiClient,
{
    pub api_client: A,
    pub(crate) _network: Network,
    pub(crate) _identity: Identity,
    pub store: EncryptedMessageStore, // Temporarily exposed outside crate for CLI client
}

impl<A> Client<A>
where
    A: XmtpApiClient,
{
    pub fn new(
        api_client: A,
        network: Network,
        identity: Identity,
        store: EncryptedMessageStore,
    ) -> Self {
        Self {
            api_client,
            _network: network,
            _identity: identity,
            store,
        }
    }
}
