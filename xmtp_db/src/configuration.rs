use xmtp_common::{NS_IN_DAY, NS_IN_SEC};

#[cfg(not(target_arch = "wasm32"))]
pub const MAX_DB_POOL_SIZE: u32 = 25;

/// Minimum number of connections that will be continually kept idle even during no
/// activity.
#[cfg(not(target_arch = "wasm32"))]
pub const MIN_DB_POOL_SIZE: u32 = 5;

/// This is the maximum amount of time SQLite spends
/// trying to acquire a lock for writing to the database.
/// if the lock fails to acquire, it returns "database is locked".
#[cfg(not(target_arch = "wasm32"))]
pub const BUSY_TIMEOUT: i32 = 5_000;

#[allow(dead_code)]
const KEYS_EXPIRATION_INTERVAL_NS: i64 = NS_IN_DAY; // 1 day

pub const KEY_PACKAGE_QUEUE_INTERVAL_NS: i64 = 5 * NS_IN_SEC; // 5 secs

#[cfg(target_arch = "wasm32")]
pub use wasm::*;

#[cfg(target_arch = "wasm32")]
mod wasm {
    // Changing these values is a breaking change, unless a migration path is specified
    pub const VFS_NAME: &str = "opfs-libxmtp";
    pub const VFS_DIRECTORY: &str = ".opfs-libxmtp-metadata";
}

#[cfg(any(test, feature = "test-utils"))]
pub mod debug_config {
    use xmtp_common::NS_IN_SEC;
    pub(crate) const KEYS_EXPIRATION_INTERVAL_NS: i64 = 3 * NS_IN_SEC; //3 seconds
}

pub fn keys_expiration_interval_ns() -> i64 {
    #[cfg(any(test, feature = "test-utils"))]
    {
        debug_config::KEYS_EXPIRATION_INTERVAL_NS
    }
    #[cfg(not(any(test, feature = "test-utils")))]
    {
        KEYS_EXPIRATION_INTERVAL_NS
    }
}
