pub mod api_client_wrapper;
pub mod association;
pub mod builder;
pub mod client;
pub mod identity;
pub mod mock_xmtp_api_client;
pub mod owner;
pub mod storage;
pub mod types;

pub use client::{Client, Network};
use storage::StorageError;
use xmtp_cryptography::signature::{RecoverableSignature, SignatureError};

pub trait InboxOwner {
    fn get_address(&self) -> String;
    fn sign(&self, text: &str) -> Result<RecoverableSignature, SignatureError>;
}

// Inserts a model to the underlying data store
pub trait Store<I> {
    fn store(&self, into: &mut I) -> Result<(), StorageError>;
}

pub trait Fetch<T> {
    type Key<'a>;
    // Fetches all instances of a model from the underlying data store
    fn fetch_all(&mut self) -> Result<Vec<T>, StorageError>;

    // Fetches a single instance by key of a model from the underlying data store
    #[allow(clippy::needless_lifetimes)]
    fn fetch_one<'a>(&mut self, key: Self::Key<'a>) -> Result<Option<T>, StorageError>;
}
