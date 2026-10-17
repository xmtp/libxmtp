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
        let starting_depth =
            <SqliteConnection as Connection>::TransactionManager::transaction_manager_status_mut(
                conn,
            )
            .transaction_depth()?
            .map_or(0, |depth| depth.get());
        let mut commit_started = false;
        let transaction_work = |conn: &mut SqliteConnection| {
            let result = work(conn);
            commit_started = result.is_ok();
            result
        };
        let result = if starting_depth > 0 {
            conn.transaction(transaction_work)
        } else {
            conn.immediate_transaction(transaction_work)
        };
        Ok(recover_failed_commit(
            conn,
            starting_depth,
            commit_started,
            result,
        ))
    })?
}

/// Recover only the transaction level whose COMMIT failed.
/// Keep the original COMMIT cause, including when its rollback also fails.
fn recover_failed_commit<T>(
    conn: &mut SqliteConnection,
    starting_depth: u32,
    commit_started: bool,
    result: Result<T, StorageError>,
) -> Result<T, StorageError> {
    let Err(StorageError::DieselResult(commit_error)) = result else {
        return result;
    };
    if !commit_started {
        return Err(StorageError::DieselResult(commit_error));
    }
    let depth =
        <SqliteConnection as Connection>::TransactionManager::transaction_manager_status_mut(conn)
            .transaction_depth();
    let rollback = match depth {
        Ok(depth) if depth.is_some_and(|depth| depth.get() > starting_depth) => {
            <SqliteConnection as Connection>::TransactionManager::rollback_transaction(conn)
        }
        Ok(_) => Ok(()),
        Err(error) => Err(error),
    };
    if let Err(rollback_error) = rollback {
        return Err(StorageError::DieselResult(
            diesel::result::Error::RollbackErrorOnCommit {
                rollback_error: Box::new(rollback_error),
                commit_error: Box::new(commit_error),
            },
        ));
    }
    Err(StorageError::DieselResult(commit_error))
}

/// A cancelled iterator ACK rolls back its tentative progress update.
/// This path requires the outer transaction, not a nested savepoint.
pub(crate) fn cancellable_ack_transaction<C>(
    connection: &C,
    work: impl FnOnce(&mut SqliteConnection) -> Result<bool, StorageError>,
) -> Result<bool, StorageError>
where
    C: ConnectionExt,
{
    connection.raw_query(|conn| {
        if <SqliteConnection as Connection>::TransactionManager::transaction_manager_status_mut(
            conn,
        )
        .transaction_depth()?
        .is_some()
        {
            return Err(diesel::result::Error::AlreadyInTransaction);
        }
        let mut cancelled = false;
        let mut commit_started = false;
        let result = conn.immediate_transaction(|conn| {
            if work(conn)? {
                commit_started = true;
                Ok(())
            } else {
                cancelled = true;
                Err(StorageError::DieselResult(
                    diesel::result::Error::RollbackTransaction,
                ))
            }
        });
        Ok(
            match recover_failed_commit(conn, 0, commit_started, result) {
                Ok(()) => Ok(true),
                Err(StorageError::DieselResult(diesel::result::Error::RollbackTransaction))
                    if cancelled =>
                {
                    Ok(false)
                }
                Err(error) => Err(error),
            },
        )
    })?
}
