//! Exclusive OPFS file changes and whole-database restore.

use super::{
    ConnectionState, POOL_TRANSITION, PlatformStorageError, VfsTransitionFailure, WasmDb,
    check_pool_usable, resume_sqlite,
};
use crate::{ConnectionExt, StorageError, StorageOption, XmtpDb, prelude::QueryDelivery};
use diesel::RunQueryDsl;
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::{Rc, Weak},
};

thread_local! {
    static CONNECTIONS: RefCell<HashMap<String, Vec<Weak<RefCell<ConnectionState>>>>> =
        RefCell::new(HashMap::new());
    static POOL_CHANGE: Cell<bool> = const { Cell::new(false) };
    static ACTIVE_UTILITIES: Cell<usize> = const { Cell::new(0) };
    static PENDING_OPENS: Cell<usize> = const { Cell::new(0) };
}

/// Exclude file changes while a persistent open can await a pool resize.
pub(super) struct PendingOpen;

impl PendingOpen {
    pub(super) fn acquire() -> Result<Self, PlatformStorageError> {
        Self::acquire_for(true)
    }

    /// Only a strict open needs a usable pool. A default open falls back to
    /// SQLite's default VFS, so only an active file change excludes it.
    pub(super) fn acquire_for(strict: bool) -> Result<Self, PlatformStorageError> {
        if strict {
            check_open_allowed()?;
        } else if POOL_CHANGE.with(Cell::get) {
            return Err(PlatformStorageError::DatabaseInUse);
        }
        PENDING_OPENS.with(|count| count.set(count.get() + 1));
        Ok(Self)
    }
}

impl Drop for PendingOpen {
    fn drop(&mut self) {
        PENDING_OPENS.with(|count| count.set(count.get() - 1));
    }
}

/// True while admitted work can touch the VFS or wait for its transition guard.
pub(super) fn operation_pending() -> bool {
    PENDING_OPENS.with(Cell::get) != 0
        || ACTIVE_UTILITIES.with(Cell::get) != 0
        || POOL_CHANGE.with(Cell::get)
}

/// Reserve a utility operation before its first await.
pub(super) struct ActiveUtility;

impl ActiveUtility {
    pub(super) fn acquire() -> Result<Self, PlatformStorageError> {
        check_open_allowed()?;
        ACTIVE_UTILITIES.with(|count| count.set(count.get() + 1));
        Ok(Self)
    }
}

impl Drop for ActiveUtility {
    fn drop(&mut self) {
        ACTIVE_UTILITIES.with(|count| count.set(count.get() - 1));
    }
}

/// Exclude persistent opens for the full duration of an OPFS file change.
pub(super) struct PoolChange;

impl PoolChange {
    pub(super) fn acquire() -> Result<Self, PlatformStorageError> {
        check_pool_usable()?;
        if operation_pending() {
            return Err(PlatformStorageError::DatabaseInUse);
        }
        POOL_CHANGE.with(|active| {
            if active.replace(true) {
                return Err(PlatformStorageError::DatabaseInUse);
            }
            Ok(Self)
        })
    }
}

impl Drop for PoolChange {
    fn drop(&mut self) {
        POOL_CHANGE.with(|active| active.set(false));
    }
}

pub(super) fn check_open_allowed() -> Result<(), PlatformStorageError> {
    check_pool_usable()?;
    if POOL_CHANGE.with(Cell::get) {
        return Err(PlatformStorageError::DatabaseInUse);
    }
    Ok(())
}

/// Track the VFS filename so all surviving objects for that file can be fenced.
pub(super) fn register_connection(path: &str, state: &Rc<RefCell<ConnectionState>>) {
    CONNECTIONS.with_borrow_mut(|connections| {
        connections.retain(|_, states| {
            states.retain(|state| state.strong_count() != 0);
            !states.is_empty()
        });
        connections
            .entry(path.to_owned())
            .or_default()
            .push(Rc::downgrade(state));
    });
}

/// The VFS keeps each supplied filename unchanged. OPFS SyncAccessHandles
/// exclude other workers. This check also excludes open handles in this worker.
pub(super) fn closed_target(
    path: Option<&str>,
) -> Result<Vec<Rc<RefCell<ConnectionState>>>, PlatformStorageError> {
    CONNECTIONS.with_borrow(|connections| {
        let states: Vec<_> = connections
            .iter()
            .filter(|(name, _)| path.is_none_or(|path| path == name.as_str()))
            .flat_map(|(_, states)| states.iter().filter_map(Weak::upgrade))
            .collect();
        for state in &states {
            let state = state
                .try_borrow()
                .map_err(|_| PlatformStorageError::DatabaseInUse)?;
            if state.connection.is_some() {
                return Err(PlatformStorageError::DatabaseInUse);
            }
        }
        Ok(states)
    })
}

/// Permanently reject reconnects through objects that refer to changed files.
fn fence(states: Vec<Rc<RefCell<ConnectionState>>>) {
    for state in states {
        state.borrow_mut().replaced = true;
    }
}

/// Read the complete file list under utility admission and transition exclusion.
pub async fn list_opfs_databases() -> Result<Vec<String>, StorageError> {
    let _utility = ActiveUtility::acquire()?;
    let _transition = POOL_TRANSITION.lock().await;
    Ok(resume_sqlite(true).await?.list())
}

/// Read the file count after pending VFS transitions finish.
pub async fn opfs_database_count() -> Result<u32, StorageError> {
    let _utility = ActiveUtility::acquire()?;
    let _transition = POOL_TRANSITION.lock().await;
    Ok(resume_sqlite(true).await?.count())
}

/// Read the pool capacity after pending VFS transitions finish.
pub async fn opfs_pool_capacity() -> Result<u32, StorageError> {
    let _utility = ActiveUtility::acquire()?;
    let _transition = POOL_TRANSITION.lock().await;
    Ok(resume_sqlite(true).await?.get_capacity())
}

/// Check for a file without observing maps during a VFS transition.
pub async fn opfs_database_exists(path: &str) -> Result<bool, StorageError> {
    super::validate_persistent_path(path)?;
    let _utility = ActiveUtility::acquire()?;
    let _transition = POOL_TRANSITION.lock().await;
    Ok(resume_sqlite(true)
        .await?
        .exists(path)
        .map_err(PlatformStorageError::from)?)
}

/// Export a closed database without changing its bytes or fencing its objects.
/// The VFS exports file bytes, not a transaction snapshot of an open database.
pub async fn export_opfs_database(path: &str) -> Result<Vec<u8>, StorageError> {
    super::validate_persistent_path(path)?;
    let _lifecycle = PoolChange::acquire()?;
    let _transition = POOL_TRANSITION.lock().await;
    let util = resume_sqlite(true).await?;
    closed_target(Some(path))?;
    Ok(util.export_db(path).map_err(PlatformStorageError::from)?)
}

/// Check integrity and the libxmtp schema, then rotate the in-memory copy's identity.
/// Old delivery cursors and owner tokens cannot authorize work on the restored copy.
async fn restored_bytes(data: &[u8]) -> Result<Vec<u8>, StorageError> {
    if data.len() < 100 || !data.starts_with(b"SQLite format 3\0") {
        return Err(PlatformStorageError::InvalidRestoreInput.into());
    }
    let mut data = data.to_vec();
    // A serialized database has no separate WAL. Match the VFS import contract.
    data[18] = 1;
    data[19] = 1;
    let staging = WasmDb::new(&StorageOption::Ephemeral).await?;
    staging
        .db()
        .raw_query(|conn| conn.deserialize_database_from_buffer(&data))?;
    #[derive(diesel::QueryableByName)]
    struct IntegrityCheck {
        #[diesel(sql_type = diesel::sql_types::Text)]
        integrity_check: String,
    }
    let checks = staging.db().raw_query(|conn| {
        diesel::sql_query("PRAGMA integrity_check").load::<IntegrityCheck>(conn)
    })?;
    if checks.len() != 1 || checks[0].integrity_check != "ok" {
        return Err(PlatformStorageError::InvalidRestoreInput.into());
    }
    // Reject an unrelated SQLite file before init can create a libxmtp schema.
    staging.db().stream_database_id()?;
    staging.init()?;
    staging.db().rotate_stream_database_id()?;
    staging
        .db()
        .raw_query(|conn| Ok(conn.serialize_database_to_buffer().to_vec()))
        .map_err(Into::into)
}

/// Import a whole database to an absent path after all target handles close.
/// Validation and identity rotation occur in memory before file creation.
/// Existing files are never overwritten. Old target objects cannot reconnect.
/// A failed import leaves prior user data unchanged.
/// No await occurs between the lifecycle check and publication of the file.
pub async fn import_opfs_database(path: &str, data: &[u8]) -> Result<(), StorageError> {
    super::validate_persistent_path(path)?;
    let _lifecycle = PoolChange::acquire()?;
    let _transition = POOL_TRANSITION.lock().await;
    let util = resume_sqlite(true).await?;
    let prepared = restored_bytes(data).await?;
    let states = closed_target(Some(path))?;
    if util.exists(path).map_err(PlatformStorageError::from)? {
        return Err(PlatformStorageError::RestoreDestinationExists.into());
    }
    if let Err(error) = util.import_db(path, &prepared) {
        // The VFS can allocate the destination before a write fails.
        // The path was absent above, so cleanup cannot remove prior user data.
        if let Err(cleanup) = util.delete_db(path) {
            tracing::error!(error = %cleanup, "failed to remove incomplete restore destination");
        }
        return Err(PlatformStorageError::from(error).into());
    }
    fence(states);
    Ok(())
}

/// Delete a closed database and permanently fence its old local objects.
pub async fn delete_opfs_database(path: &str) -> Result<bool, StorageError> {
    super::validate_persistent_path(path)?;
    let _lifecycle = PoolChange::acquire()?;
    let _transition = POOL_TRANSITION.lock().await;
    let util = resume_sqlite(true).await?;
    let states = closed_target(Some(path))?;
    let result = util.delete_db(path).map_err(PlatformStorageError::from)?;
    fence(states);
    Ok(result)
}

/// Clear the OPFS pool only after all persistent database handles close.
pub async fn clear_opfs_databases() -> Result<(), StorageError> {
    let _lifecycle = PoolChange::acquire()?;
    let _transition = POOL_TRANSITION.lock().await;
    let util = resume_sqlite(true).await?;
    let states = closed_target(None)?;
    // Fence before the VFS releases its file handles across this await.
    fence(states);
    let failure = VfsTransitionFailure(true);
    util.clear_all().await.map_err(PlatformStorageError::from)?;
    failure.complete();
    Ok(())
}
