//! Shared transaction and error types for durable client streams.

use diesel::{Connection, SqliteConnection, connection::TransactionManager};
use thiserror::Error;
use xmtp_common::{ErrorCode, RetryableError};

use crate::{ConnectionExt, StorageError};

/// The independent capacity check that rejected an otherwise valid admission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetScope {
    /// One fetched or admitted batch.
    Batch,
    /// All pending rows for one topic.
    Topic,
    /// All pending rows of one log kind; dependency kinds keep separate capacity.
    Kind,
}

/// Stable storage failures that preserve receipt, processing, and delivery invariants.
#[derive(Debug, Error, ErrorCode)]
pub enum StreamStorageError {
    /// The batch does not have valid, strictly increasing envelope IDs.
    #[error("The ordered envelope batch is invalid")]
    InvalidBatch,
    /// The source omitted a prefix not yet stored in this database.
    #[error("The source starts at {after}, beyond received position {received}")]
    MissingPrefix { after: u64, received: u64 },
    /// Existing progress was not written by ordered admission.
    #[error("Network progress has no proven received prefix")]
    UninitializedNetworkProgress,
    /// Pending work exceeds its row or byte budget. Retry after processing drains it.
    #[error("The {scope:?} pending budget is full")]
    Capacity { scope: BudgetScope },
    /// Another processor changed the pending head. Reload state before retrying.
    #[error("The pending envelope is no longer the topic head")]
    HeadChanged,
    /// Installing this welcome would rewind or replace processed state.
    #[error("The join anchor does not advance processed progress")]
    StaleJoinAnchor,
    /// Another default consumer holds an unexpired lease.
    #[error("A default message consumer is already active")]
    AlreadyActive,
    /// The lease expired or a new consumer acquired ownership.
    #[error("The default message consumer no longer owns delivery")]
    NotCurrentOwner,
    /// The cursor was issued before restore or by another database.
    #[error("The delivery cursor belongs to another database")]
    ForeignCursor,
    /// The persistent local message counter cannot allocate another number.
    #[error("The delivery sequence allocator is exhausted")]
    DeliveryExhausted,
    /// The next retained message exceeds the local read byte limit. Not retryable.
    #[error("The next local message needs {bytes} bytes, above the {limit} byte limit")]
    LocalReadCapacity { bytes: u64, limit: u64 },
    /// The cursor is ahead of local history or the lease interval is empty.
    #[error("The delivery cursor or lease is invalid")]
    InvalidDeliveryPosition,
}

impl RetryableError for StreamStorageError {
    fn is_retryable(&self) -> bool {
        matches!(self, Self::Capacity { .. } | Self::HeadChanged)
    }
}

/// Acquire the database writer before reading mutable stream state.
/// Nested calls use a savepoint under the writer already held by the caller.
pub(crate) fn stream_transaction<C, T>(
    connection: &C,
    work: impl FnOnce(&mut SqliteConnection) -> Result<T, StorageError>,
) -> Result<T, StorageError>
where
    C: ConnectionExt,
{
    connection.raw_query(|conn| {
        let nested =
            <SqliteConnection as Connection>::TransactionManager::transaction_manager_status_mut(
                conn,
            )
            .transaction_depth()?
            .is_some();
        Ok(if nested {
            conn.transaction(work)
        } else {
            conn.immediate_transaction(work)
        })
    })?
}
