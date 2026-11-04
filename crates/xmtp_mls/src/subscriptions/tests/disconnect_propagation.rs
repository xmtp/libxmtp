//! Pins that a dropped-pool signal survives `SubscribeError`, so a worker
//! that surfaces it keeps `needs_db_reconnect()` `true`. These fixtures use
//! native pool errors. Browser connection lifecycle tests cover closed
//! persistent handles in the WASM database module.

use crate::subscriptions::SubscribeError;
use xmtp_db::{ConnectionError, PlatformStorageError, StorageError};
use xmtp_mls_core::reconnect::NeedsDbReconnect;
use xmtp_mls_engine::groups::GroupError;

/// A `StorageError` that signals the connection pool was dropped.
fn disconnect_storage() -> StorageError {
    StorageError::Platform(PlatformStorageError::PoolNeedsConnection)
}

/// A `ConnectionError` that signals the connection pool was dropped.
fn disconnect_connection() -> ConnectionError {
    ConnectionError::Platform(PlatformStorageError::PoolNeedsConnection)
}

/// A storage error that is NOT a disconnect — must never trip the contract.
fn benign_storage() -> StorageError {
    StorageError::InvalidHmacLength
}

#[xmtp_common::test]
fn subscribe_error_forwards_disconnect() {
    assert!(SubscribeError::Storage(disconnect_storage()).needs_db_reconnect());
    assert!(SubscribeError::Db(disconnect_connection()).needs_db_reconnect());
    assert!(SubscribeError::from(GroupError::Storage(disconnect_storage())).needs_db_reconnect());
    assert!(!SubscribeError::Storage(benign_storage()).needs_db_reconnect());
}
