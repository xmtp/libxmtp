//! Pins that a dropped-pool signal survives `GroupError`, so a worker that
//! surfaces it keeps `needs_db_reconnect()` `true`. These fixtures use
//! native pool errors. Browser connection lifecycle tests cover closed
//! persistent handles in the WASM database module.

use crate::groups::GroupError;
use xmtp_db::{ConnectionError, PlatformStorageError, StorageError};
use xmtp_mls_core::mls_store::MlsStoreError;
use xmtp_mls_core::reconnect::NeedsDbReconnect;

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
fn group_error_forwards_disconnect() {
    use crate::subscriptions::barrier::{BarrierCause, BarrierError, BarrierFailure, BarrierTopic};
    use crate::subscriptions::incoming::IncomingError;
    use std::sync::Arc;
    use xmtp_proto::types::{Cursor, Topic};

    assert!(GroupError::Storage(disconnect_storage()).needs_db_reconnect());
    assert!(GroupError::Db(disconnect_connection()).needs_db_reconnect());
    assert!(
        GroupError::MlsStore(MlsStoreError::Connection(disconnect_connection()))
            .needs_db_reconnect()
    );
    // A non-disconnect storage failure inside a GroupError must not stop the worker.
    assert!(!GroupError::Storage(benign_storage()).needs_db_reconnect());
    assert!(!GroupError::InvalidGroupMembership.needs_db_reconnect());

    let barrier = |cause| BarrierError::Incomplete {
        reason: BarrierFailure::Blocked,
        unfinished: vec![BarrierTopic {
            topic: Topic::new_group_message([1; 32]),
            scope_generation: None,
            target: Some(Cursor(3)),
            received: Cursor(2),
            processed: Cursor(1),
            unresolved_welcomes: Vec::new(),
            inactive: false,
            cause: Some(cause),
        }],
    };
    assert!(
        GroupError::StreamBarrier(barrier(BarrierCause::Storage(Arc::new(
            disconnect_storage(),
        ))))
        .needs_db_reconnect()
    );
    assert!(
        GroupError::PublishedButUnconfirmed {
            intent_id: 1,
            cause: Some(Box::new(barrier(BarrierCause::Receiver(Arc::new(
                IncomingError::Processing(
                    crate::groups::mls_sync::GroupMessageProcessingError::Storage(
                        disconnect_storage(),
                    ),
                ),
            ))))),
            rate_limit_delay: None,
        }
        .needs_db_reconnect()
    );
    assert!(
        GroupError::Sync(Box::new(crate::groups::summary::SyncSummary::other(
            GroupError::Storage(disconnect_storage()),
        )))
        .needs_db_reconnect()
    );
    assert!(
        !GroupError::StreamBarrier(barrier(BarrierCause::Blocked("unsupported".into())))
            .needs_db_reconnect()
    );
}
