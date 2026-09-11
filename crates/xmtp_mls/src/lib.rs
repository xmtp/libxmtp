#![recursion_limit = "256"]
#![warn(clippy::unwrap_used)]

pub mod builder;
pub mod client;
pub mod context;
mod definitions;
pub mod groups;
pub mod identity;
pub mod identity_updates;
mod intents;
pub mod messages;
pub mod mls_store;
mod mutex_registry;
mod state_tx;
pub use client::VisibilityConfirmationOptions;
pub mod subscriptions;
pub mod utils;
pub mod worker;
pub use definitions::*;

#[cfg(any(test, feature = "test-utils"))]
pub mod test;
mod traits;

#[cfg(test)]
use crate::groups::GroupError;
pub use client::{Client, Network};
#[cfg(test)]
use parking_lot::Mutex;
#[cfg(test)]
use std::collections::HashMap;
#[cfg(test)]
use std::sync::Arc;
#[cfg(test)]
use tokio::sync::Mutex as TokioMutex;
pub use xmtp_common as common;
pub use xmtp_db as db;
#[cfg(test)]
use xmtp_db::DuplicateItem;
use xmtp_db::StorageError;
pub use xmtp_id::InboxOwner;
pub use xmtp_mls_common as mls_common;
pub use xmtp_proto::api_client::*;
#[cfg(test)]
use xmtp_proto::types::GroupId;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// A manager for group-specific semaphores
#[cfg(test)]
#[derive(Debug)]
pub struct GroupCommitLock {
    // Storage for group-specific semaphores
    locks: Mutex<HashMap<GroupId, Arc<TokioMutex<()>>>>,
}

#[cfg(test)]
impl Default for GroupCommitLock {
    fn default() -> Self {
        Self::new()
    }
}
#[cfg(test)]
impl GroupCommitLock {
    /// Create a new `GroupCommitLock`
    pub fn new() -> Self {
        Self {
            locks: Mutex::new(HashMap::new()),
        }
    }

    /// Get or create a semaphore for a specific group and acquire it, returning a guard
    pub async fn get_lock_async(&self, group_id: GroupId) -> MlsGroupGuard {
        let lock = {
            let mut locks = self.locks.lock();
            locks
                .entry(group_id)
                .or_insert_with(|| Arc::new(TokioMutex::new(())))
                .clone()
        };

        MlsGroupGuard {
            _permit: lock.lock_owned().await,
        }
    }

    /// Get or create a semaphore for a specific group and acquire it synchronously
    pub fn get_lock_sync(&self, group_id: GroupId) -> Result<MlsGroupGuard, GroupError> {
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
#[cfg(test)]
pub struct MlsGroupGuard {
    _permit: tokio::sync::OwnedMutexGuard<()>,
}

#[cfg_attr(not(target_arch = "wasm32"), ctor::ctor(unsafe))]
#[cfg(all(test, not(target_arch = "wasm32")))]
fn test_setup() {
    xmtp_common::logger();
    let _ = fdlimit::raise_fd_limit();
}
