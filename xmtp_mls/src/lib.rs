pub mod api_client_wrapper;
pub mod association;
pub mod builder;
pub mod client;
mod configuration;
pub mod identity;
pub mod mock_xmtp_api_client;
pub mod owner;
mod proto_wrapper;
pub mod storage;
pub mod types;
mod xmtp_openmls_provider;

pub use client::{Client, Network};
use storage::StorageError;
use xmtp_cryptography::signature::{RecoverableSignature, SignatureError};

pub trait InboxOwner {
    fn get_address(&self) -> String;
    fn sign(&self, text: &str) -> Result<RecoverableSignature, SignatureError>;
}

// Inserts a model to the underlying data store
pub trait Store<StorageConnection> {
    fn store(&self, into: &mut StorageConnection) -> Result<(), StorageError>;
}

pub trait Fetch<Model> {
    type Key;
    fn fetch(&mut self, key: Self::Key) -> Result<Option<Model>, StorageError>;
}

pub trait Delete<Model> {
    type Key;
    fn delete(&mut self, key: Self::Key) -> Result<usize, StorageError>;
}

#[cfg(test)]
mod tests {}
