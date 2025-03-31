#![recursion_limit = "256"]
#![warn(clippy::unwrap_used)]

pub mod builder;
pub mod client;
pub mod configuration;
pub mod groups;
mod hpke;
pub mod identity;
pub mod identity_updates;
mod intents;
mod mutex_registry;
pub mod storage;
pub mod subscriptions;
pub mod types;
pub mod utils;
pub mod verified_key_package_v2;

pub use client::{Client, Network};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use storage::{xmtp_openmls_provider::XmtpOpenMlsProvider, DuplicateItem, StorageError};
use tokio::sync::Mutex as TokioMutex;

pub use xmtp_id::InboxOwner;
pub use xmtp_proto::api_client::trait_impls::*;

/// A manager for group-specific semaphores
#[derive(Debug)]
pub struct GroupCommitLock {
    // Storage for group-specific semaphores
    locks: Mutex<HashMap<Vec<u8>, Arc<TokioMutex<()>>>>,
}

impl Default for GroupCommitLock {
    fn default() -> Self {
        Self::new()
    }
}
impl GroupCommitLock {
    /// Create a new `GroupCommitLock`
    pub fn new() -> Self {
        Self {
            locks: Mutex::new(HashMap::new()),
        }
    }

    /// Get or create a semaphore for a specific group and acquire it, returning a guard
    pub async fn get_lock_async(&self, group_id: Vec<u8>) -> Result<MlsGroupGuard, GroupError> {
        let lock = {
            let mut locks = self.locks.lock();
            locks
                .entry(group_id)
                .or_insert_with(|| Arc::new(TokioMutex::new(())))
                .clone()
        };

        Ok(MlsGroupGuard {
            _permit: lock.lock_owned().await,
        })
    }

    /// Get or create a semaphore for a specific group and acquire it synchronously
    pub fn get_lock_sync(&self, group_id: Vec<u8>) -> Result<MlsGroupGuard, GroupError> {
        let lock = {
            let mut locks = self.locks.lock();
            locks
                .entry(group_id)
                .or_insert_with(|| Arc::new(TokioMutex::new(())))
                .clone()
        };

        // Synchronously acquire the permit
        let permit = lock
            .try_lock_owned()
            .map_err(|_| GroupError::LockUnavailable)?;
        Ok(MlsGroupGuard { _permit: permit })
    }
}

/// A guard that releases the semaphore when dropped
pub struct MlsGroupGuard {
    _permit: tokio::sync::OwnedMutexGuard<()>,
}

/// Inserts a model to the underlying data store, erroring if it already exists
pub trait Store<StorageConnection> {
    type Output;
    fn store(&self, into: &StorageConnection) -> Result<Self::Output, StorageError>;
}

/// Inserts a model to the underlying data store, silent no-op on unique constraint violations
pub trait StoreOrIgnore<StorageConnection> {
    type Output;
    fn store_or_ignore(&self, into: &StorageConnection) -> Result<Self::Output, StorageError>;
}

/// Fetches a model from the underlying data store, returning None if it does not exist
pub trait Fetch<Model> {
    type Key;
    fn fetch(&self, key: &Self::Key) -> Result<Option<Model>, StorageError>;
}

/// Fetches all instances of `Model` from the data store.
/// Returns an empty list if no items are found or an error if the fetch fails.
pub trait FetchList<Model> {
    fn fetch_list(&self) -> Result<Vec<Model>, StorageError>;
}

/// Fetches a filtered list of `Model` instances matching the specified key.
/// Logs an error and returns an empty list if no items are found or if an error occurs.
///
/// # Parameters
/// - `key`: The key used to filter the items in the data store.
pub trait FetchListWithKey<Model> {
    type Key;
    fn fetch_list_with_key(&self, keys: &[Self::Key]) -> Result<Vec<Model>, StorageError>;
}

/// Deletes a model from the underlying data store
pub trait Delete<Model> {
    type Key;
    fn delete(&self, key: Self::Key) -> Result<usize, StorageError>;
}

use crate::groups::GroupError;
/*
pub use stream_handles::{
    spawn, AbortHandle, GenericStreamHandle, StreamHandle, StreamHandleError,
};
*/
#[cfg(test)]
pub(crate) mod tests {
    // Execute once before any tests are run
    #[cfg_attr(not(target_arch = "wasm32"), ctor::ctor)]
    #[cfg(not(target_arch = "wasm32"))]
    fn _setup() {
        xmtp_common::logger();
        let _ = fdlimit::raise_fd_limit();
    }
}
