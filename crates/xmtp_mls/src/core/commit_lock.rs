//! The per-group commit lock that tests hold to stall a group's commits.

use parking_lot::Mutex;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex as TokioMutex;
use xmtp_proto::types::GroupId;

/// Another caller holds the group's commit lock, so a synchronous acquire
/// fails instead of waiting.
#[derive(Debug, thiserror::Error)]
#[error("the group commit lock is held")]
pub struct CommitLockUnavailable;

/// A manager for group-specific semaphores
#[derive(Debug)]
pub struct GroupCommitLock {
    // Storage for group-specific semaphores
    locks: Mutex<HashMap<GroupId, Arc<TokioMutex<()>>>>,
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
    pub fn get_lock_sync(&self, group_id: GroupId) -> Result<MlsGroupGuard, CommitLockUnavailable> {
        let lock = {
            let mut locks = self.locks.lock();
            locks
                .entry(group_id)
                .or_insert_with(|| Arc::new(TokioMutex::new(())))
                .clone()
        };

        // Synchronously acquire the permit
        let permit = lock.try_lock_owned().map_err(|_| CommitLockUnavailable)?;
        Ok(MlsGroupGuard { _permit: permit })
    }
}
/// A guard that releases the semaphore when dropped
pub struct MlsGroupGuard {
    _permit: tokio::sync::OwnedMutexGuard<()>,
}
