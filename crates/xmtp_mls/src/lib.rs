#![recursion_limit = "256"]
#![warn(clippy::unwrap_used)]
// Async-track only: naming an async closure's `CallOnceFuture` to bound it `Send`
// for `generate_commit_with_rollback`'s operation closure needs these unstable
// features (built with a nightly / `RUSTC_BOOTSTRAP=1` toolchain). The stable sync
// track never enables them (its `generate_commit_with_rollback` omits that bound).
#![cfg_attr(
    not(feature = "blocking"),
    feature(async_fn_traits, unboxed_closures)
)]

/// `expr.await` on the async (Postgres) track, `expr` on the sync (SQLite) track.
///
/// openmls is compiled maybe_async: a given method is `async fn` on the async
/// track and blocking on the sync track. This crate's own functions are `async fn`
/// on both tracks, so a call to such an openmls method must be awaited on async and
/// used directly on sync. This wraps that single difference so a shared call site
/// stays single-source. (openmls-async ⟺ this crate's `async` feature without `sync`.)
macro_rules! maybe_await {
    ($e:expr) => {{
        #[cfg(not(feature = "blocking"))]
        {
            $e.await
        }
        #[cfg(feature = "blocking")]
        {
            $e
        }
    }};
}

pub mod builder;
pub mod client;
pub mod context;
pub mod cursor_store;
mod definitions;
pub mod groups;
pub mod identity;
pub mod identity_updates;
mod intents;
pub mod messages;
pub mod mls_store;
mod mutex_registry;
pub mod registration_visible;
pub mod subscriptions;
pub mod utils;
pub mod worker;
pub use definitions::*;

#[cfg(all(test, not(target_arch = "wasm32"), feature = "d14n"))]
mod migration_tests;
#[cfg(any(test, feature = "test-utils"))]
pub mod test;
#[cfg(test)]
mod tests;
mod traits;

pub use client::{Client, Network};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;
pub use xmtp_common as common;
pub use xmtp_db as db;
use xmtp_db::{DuplicateItem, StorageError};
pub use xmtp_id::InboxOwner;
pub use xmtp_mls_common as mls_common;
pub use xmtp_proto::api_client::*;
use xmtp_proto::types::GroupId;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

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
}
/// A guard that releases the semaphore when dropped
pub struct MlsGroupGuard {
    _permit: tokio::sync::OwnedMutexGuard<()>,
}

#[cfg_attr(not(target_arch = "wasm32"), ctor::ctor(unsafe))]
#[cfg(all(test, not(target_arch = "wasm32")))]
fn test_setup() {
    xmtp_common::logger();
    let _ = fdlimit::raise_fd_limit();
}
