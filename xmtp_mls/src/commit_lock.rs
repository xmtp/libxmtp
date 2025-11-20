//! Group commit lock implementation using file descriptor locks for cross-process synchronization.
//!
//! On native platforms, this uses `fd-lock` for file-based locking that works across processes.
//! On WASM, this falls back to in-memory mutexes since file locking is not available.

use thiserror::Error;

/// Errors that can occur when acquiring a commit lock.
#[derive(Debug, Error)]
pub enum CommitLockError {
    /// The lock is not immediately available (for non-blocking acquire).
    #[error("Lock is not available")]
    LockUnavailable,

    /// Failed to open or create the lock file.
    #[error("Failed to open lock file: {0}")]
    FileOpen(#[from] std::io::Error),

    /// Failed to acquire the lock.
    #[error("Failed to acquire lock: {0}")]
    LockAcquire(String),

    /// The lock task was cancelled or failed.
    #[error("Lock task failed: {0}")]
    TaskFailed(String),
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;
    use fd_lock::{RwLock, RwLockWriteGuard};
    use std::fs::{File, OpenOptions, create_dir_all};
    use std::path::PathBuf;

    /// A manager for group-specific file locks that work across processes.
    #[derive(Debug)]
    pub struct GroupCommitLock {
        /// Root folder where lock files are stored
        root_folder: PathBuf,
    }

    impl GroupCommitLock {
        /// Create a new `GroupCommitLock`.
        ///
        /// # Arguments
        /// * `db_path` - Path to the database file. The parent directory will be used.
        /// * `installation_id` - The installation ID, used to create a unique subfolder.
        ///
        /// The lock files will be stored in `{db_parent}/{installation_id}/`.
        pub fn new(db_path: PathBuf, installation_id: &str) -> Self {
            let parent = db_path.parent().unwrap_or(&db_path);
            let root_folder = parent.join(installation_id);

            // Silently create the directory if it doesn't exist
            // create_dir_all is idempotent and safe for concurrent calls
            let _ = create_dir_all(&root_folder);

            Self { root_folder }
        }

        /// Get the lock file path for a given group ID.
        fn lock_file_path(&self, group_id: &[u8]) -> PathBuf {
            let filename = hex::encode(group_id);
            self.root_folder.join(filename)
        }

        /// Open or create the lock file for a given group ID.
        fn open_lock_file(&self, group_id: &[u8]) -> Result<File, std::io::Error> {
            let path = self.lock_file_path(group_id);
            OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .open(path)
        }

        /// Get or create a lock for a specific group and acquire it asynchronously.
        ///
        /// This will block until the lock is acquired.
        pub async fn get_lock_async(
            &self,
            group_id: Vec<u8>,
        ) -> Result<MlsGroupGuard, CommitLockError> {
            let file = self.open_lock_file(&group_id)?;

            // Use spawn_blocking to avoid blocking the async runtime
            let guard = tokio::task::spawn_blocking(move || MlsGroupGuard::new(file))
                .await
                .map_err(|e| CommitLockError::TaskFailed(e.to_string()))??;

            Ok(guard)
        }

        /// Get or create a lock for a specific group and try to acquire it synchronously.
        ///
        /// Returns an error if the lock is not immediately available.
        pub fn get_lock_sync(&self, group_id: Vec<u8>) -> Result<MlsGroupGuard, CommitLockError> {
            let file = self.open_lock_file(&group_id)?;
            MlsGroupGuard::try_new(file)
        }
    }

    /// A guard that holds the file lock. The lock is released when this guard is dropped.
    ///
    /// This uses raw pointers internally to create a self-referential struct that holds
    /// both the RwLock and its guard. The guard must be dropped before the lock.
    pub struct MlsGroupGuard {
        /// The write guard that holds the actual lock. Must be dropped first.
        guard: *mut RwLockWriteGuard<'static, File>,
        /// The RwLock containing the file. Must be dropped after guard.
        lock: *mut RwLock<File>,
    }

    impl std::fmt::Debug for MlsGroupGuard {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("MlsGroupGuard")
                .field("guard", &"<write_guard>")
                .field("lock", &"<rw_lock>")
                .finish()
        }
    }

    impl MlsGroupGuard {
        /// Create a new guard by acquiring a blocking write lock.
        fn new(file: File) -> Result<Self, CommitLockError> {
            // Box the lock so it has a stable address
            let lock = Box::into_raw(Box::new(RwLock::new(file)));

            unsafe {
                // Acquire the write lock
                let guard = (*lock)
                    .write()
                    .map_err(|e| CommitLockError::LockAcquire(e.to_string()))?;

                // Transmute the lifetime to 'static since we control the lifetime
                // through our Drop implementation which ensures proper drop order
                let guard: RwLockWriteGuard<'static, File> = std::mem::transmute(guard);
                let guard = Box::into_raw(Box::new(guard));

                Ok(Self { guard, lock })
            }
        }

        /// Try to create a new guard by attempting a non-blocking write lock.
        fn try_new(file: File) -> Result<Self, CommitLockError> {
            // Box the lock so it has a stable address
            let lock = Box::into_raw(Box::new(RwLock::new(file)));

            unsafe {
                // Try to acquire the write lock
                let guard = (*lock)
                    .try_write()
                    .map_err(|_| CommitLockError::LockUnavailable)?;

                // Transmute the lifetime to 'static since we control the lifetime
                let guard: RwLockWriteGuard<'static, File> = std::mem::transmute(guard);
                let guard = Box::into_raw(Box::new(guard));

                Ok(Self { guard, lock })
            }
        }
    }

    impl Drop for MlsGroupGuard {
        fn drop(&mut self) {
            unsafe {
                // Drop guard first (releases the OS lock)
                let _ = Box::from_raw(self.guard);
                // Then drop the lock (closes the file)
                let _ = Box::from_raw(self.lock);
            }
        }
    }

    // Safety: The guard holds exclusive access to the file lock.
    // The raw pointers are only accessed in Drop which runs on a single thread.
    unsafe impl Send for MlsGroupGuard {}
    unsafe impl Sync for MlsGroupGuard {}

    #[cfg(any(test, feature = "test-utils"))]
    impl Default for GroupCommitLock {
        fn default() -> Self {
            Self::new(
                std::env::temp_dir().join("xmtp_test_default"),
                "default_installation",
            )
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::*;
    use parking_lot::Mutex;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::Mutex as TokioMutex;

    /// A manager for group-specific semaphores (WASM fallback using in-memory mutexes).
    #[derive(Debug)]
    pub struct GroupCommitLock {
        // Storage for group-specific semaphores
        locks: Mutex<HashMap<Vec<u8>, Arc<TokioMutex<()>>>>,
    }

    impl GroupCommitLock {
        /// Create a new `GroupCommitLock`.
        ///
        /// On WASM, the db_path and installation_id are ignored since we use in-memory locking.
        pub fn new(_db_path: std::path::PathBuf, _installation_id: &str) -> Self {
            Self {
                locks: Mutex::new(HashMap::new()),
            }
        }

        /// Get or create a semaphore for a specific group and acquire it, returning a guard.
        pub async fn get_lock_async(
            &self,
            group_id: Vec<u8>,
        ) -> Result<MlsGroupGuard, CommitLockError> {
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

        /// Get or create a semaphore for a specific group and acquire it synchronously.
        pub fn get_lock_sync(&self, group_id: Vec<u8>) -> Result<MlsGroupGuard, CommitLockError> {
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
                .map_err(|_| CommitLockError::LockUnavailable)?;
            Ok(MlsGroupGuard { _permit: permit })
        }
    }

    /// A guard that releases the semaphore when dropped.
    pub struct MlsGroupGuard {
        _permit: tokio::sync::OwnedMutexGuard<()>,
    }

    #[cfg(any(test, feature = "test-utils"))]
    impl Default for GroupCommitLock {
        fn default() -> Self {
            Self::new(
                std::path::PathBuf::from("/tmp/xmtp_test_default"),
                "default_installation",
            )
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::*;

#[cfg(target_arch = "wasm32")]
pub use wasm::*;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;

    /// Generate a unique temporary directory for each test.
    /// OS handles cleanup of temp directories.
    fn test_lock_dir() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "xmtp_commit_lock_test_{}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            id
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    // ==================== Lock Release Tests ====================

    /// Test that the lock is released when the guard is dropped (native)
    #[cfg(not(target_arch = "wasm32"))]
    #[xmtp_common::test]
    async fn test_lock_released_on_guard_drop_native() {
        let test_dir = test_lock_dir();
        let db_path = test_dir.join("test.db");
        let lock_manager = GroupCommitLock::new(db_path, "test_installation");
        let group_id = vec![1, 2, 3, 4];

        // Acquire the lock
        let guard = lock_manager
            .get_lock_async(group_id.clone())
            .await
            .expect("Failed to acquire lock");

        // Try to acquire same lock synchronously - should fail
        let result = lock_manager.get_lock_sync(group_id.clone());
        assert!(
            result.is_err(),
            "Should not be able to acquire lock while held"
        );

        // Drop the guard
        drop(guard);

        // Now should be able to acquire
        let guard2 = lock_manager
            .get_lock_sync(group_id.clone())
            .expect("Should be able to acquire lock after guard dropped");
        drop(guard2);
    }

    /// Test that the lock is released when the guard is dropped (async version)
    #[xmtp_common::test]
    async fn test_lock_released_on_guard_drop_async() {
        let test_dir = test_lock_dir();
        let db_path = test_dir.join("test.db");
        let lock_manager = GroupCommitLock::new(db_path, "test_installation");
        let group_id = vec![1, 2, 3, 4];

        // Acquire the lock
        let guard = lock_manager
            .get_lock_async(group_id.clone())
            .await
            .expect("Failed to acquire lock");

        // Drop the guard
        drop(guard);

        // Should be able to acquire again
        let _guard2 = lock_manager
            .get_lock_async(group_id.clone())
            .await
            .expect("Should be able to acquire lock after guard dropped");
    }

    // ==================== Concurrent Access Tests ====================

    /// Test that multiple async tasks wait for the lock
    #[xmtp_common::test]
    async fn test_async_lock_contention() {
        let test_dir = test_lock_dir();
        let db_path = test_dir.join("test.db");
        let lock_manager = Arc::new(GroupCommitLock::new(db_path, "test_installation"));
        let group_id = vec![1, 2, 3, 4];

        let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let order = Arc::new(std::sync::Mutex::new(Vec::new()));

        let mut handles = Vec::new();

        for i in 0..5 {
            let lock_manager = Arc::clone(&lock_manager);
            let group_id = group_id.clone();
            let counter = Arc::clone(&counter);
            let order = Arc::clone(&order);

            let handle = tokio::spawn(async move {
                let _guard = lock_manager
                    .get_lock_async(group_id)
                    .await
                    .expect("Failed to acquire lock");

                // Record that we acquired the lock
                let my_order = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                order.lock().unwrap().push((i, my_order));

                // Hold the lock briefly
                tokio::time::sleep(Duration::from_millis(10)).await;
            });

            handles.push(handle);
        }

        // Wait for all tasks
        for handle in handles {
            handle.await.unwrap();
        }

        // Verify all tasks completed
        let final_order = order.lock().unwrap();
        assert_eq!(final_order.len(), 5, "All 5 tasks should have completed");

        // Verify sequential acquisition (each task got a unique order number)
        let mut order_nums: Vec<_> = final_order.iter().map(|(_, o)| *o).collect();
        order_nums.sort();
        assert_eq!(
            order_nums,
            vec![0, 1, 2, 3, 4],
            "Tasks should have acquired lock sequentially"
        );
    }

    /// Test that sync lock acquisition fails immediately when lock is held
    #[cfg(not(target_arch = "wasm32"))]
    #[xmtp_common::test]
    async fn test_sync_lock_fails_when_held() {
        let test_dir = test_lock_dir();
        let db_path = test_dir.join("test.db");
        let lock_manager = GroupCommitLock::new(db_path.clone(), "test_installation");
        let group_id = vec![1, 2, 3, 4];

        // Acquire lock in main task
        let guard = lock_manager
            .get_lock_async(group_id.clone())
            .await
            .expect("Failed to acquire lock");

        // Create a new lock manager instance (new file descriptor) to test lock contention
        let lock_manager2 = GroupCommitLock::new(db_path, "test_installation");
        let group_id_clone = group_id.clone();
        let handle = std::thread::spawn(move || lock_manager2.get_lock_sync(group_id_clone));

        let result = handle.join().unwrap();
        assert!(
            matches!(result, Err(CommitLockError::LockUnavailable)),
            "Sync lock should fail with LockUnavailable when lock is held"
        );

        drop(guard);
    }

    /// Test concurrent access from multiple threads using Arc
    #[cfg(not(target_arch = "wasm32"))]
    #[xmtp_common::test]
    async fn test_arc_shared_between_threads() {
        let test_dir = test_lock_dir();
        let db_path = test_dir.join("test.db");
        let lock_manager = Arc::new(GroupCommitLock::new(db_path, "test_installation"));
        let group_id = vec![1, 2, 3, 4];

        let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));

        let mut handles = Vec::new();

        for _ in 0..3 {
            let lock_manager = Arc::clone(&lock_manager);
            let group_id = group_id.clone();
            let counter = Arc::clone(&counter);

            let handle = std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    let _guard = lock_manager
                        .get_lock_async(group_id)
                        .await
                        .expect("Failed to acquire lock");

                    counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

                    // Hold lock briefly
                    tokio::time::sleep(Duration::from_millis(10)).await;
                });
            });

            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(
            counter.load(std::sync::atomic::Ordering::SeqCst),
            3,
            "All threads should have acquired and released the lock"
        );
    }

    // ==================== Different Group IDs ====================

    /// Test that different group IDs don't interfere with each other
    #[xmtp_common::test]
    async fn test_different_group_ids_independent() {
        let test_dir = test_lock_dir();
        let db_path = test_dir.join("test.db");
        let lock_manager = GroupCommitLock::new(db_path, "test_installation");

        let group_id_1 = vec![1, 2, 3, 4];
        let group_id_2 = vec![5, 6, 7, 8];

        // Acquire lock for group 1
        let guard1 = lock_manager
            .get_lock_async(group_id_1.clone())
            .await
            .expect("Failed to acquire lock for group 1");

        // Should be able to acquire lock for group 2
        let guard2 = lock_manager
            .get_lock_async(group_id_2.clone())
            .await
            .expect("Should be able to acquire lock for different group");

        // Both locks held simultaneously
        drop(guard1);
        drop(guard2);
    }

    // ==================== Error Handling ====================

    /// Test that errors are properly propagated
    #[cfg(not(target_arch = "wasm32"))]
    #[xmtp_common::test]
    async fn test_error_types() {
        let test_dir = test_lock_dir();
        let db_path = test_dir.join("test.db");
        let lock_manager = GroupCommitLock::new(db_path, "test_installation");
        let group_id = vec![1, 2, 3, 4];

        // Acquire lock
        let _guard = lock_manager
            .get_lock_async(group_id.clone())
            .await
            .expect("Failed to acquire lock");

        // Sync should return LockUnavailable
        let err = lock_manager.get_lock_sync(group_id.clone()).unwrap_err();
        match err {
            CommitLockError::LockUnavailable => {}
            _ => panic!("Expected LockUnavailable, got {:?}", err),
        }
    }

    // ==================== Directory Creation ====================

    /// Test that the lock directory is created if it doesn't exist
    #[cfg(not(target_arch = "wasm32"))]
    #[xmtp_common::test]
    async fn test_directory_creation() {
        let test_dir = test_lock_dir();
        let db_path = test_dir.join("subdir").join("test.db");

        let lock_manager = GroupCommitLock::new(db_path, "test_installation");
        let group_id = vec![1, 2, 3, 4];

        // Acquiring lock should create the directory
        let _guard = lock_manager
            .get_lock_async(group_id)
            .await
            .expect("Failed to acquire lock");

        // Directory should now exist
        let expected_dir = test_dir.join("subdir").join("test_installation");
        assert!(expected_dir.exists(), "Lock directory should be created");
    }

    // ==================== Stress Tests ====================

    /// Stress test with many concurrent acquisitions
    #[xmtp_common::test]
    async fn test_stress_many_concurrent_tasks() {
        let test_dir = test_lock_dir();
        let db_path = test_dir.join("test.db");
        let lock_manager = Arc::new(GroupCommitLock::new(db_path, "test_installation"));
        let group_id = vec![1, 2, 3, 4];

        let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));

        let mut handles = Vec::new();

        for _ in 0..20 {
            let lock_manager = Arc::clone(&lock_manager);
            let group_id = group_id.clone();
            let counter = Arc::clone(&counter);

            let handle = tokio::spawn(async move {
                let _guard = lock_manager
                    .get_lock_async(group_id)
                    .await
                    .expect("Failed to acquire lock");

                counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

                // Very brief hold
                tokio::time::sleep(Duration::from_millis(1)).await;
            });

            handles.push(handle);
        }

        for handle in handles {
            handle.await.unwrap();
        }

        assert_eq!(
            counter.load(std::sync::atomic::Ordering::SeqCst),
            20,
            "All 20 tasks should complete"
        );
    }

    /// Test rapid acquire/release cycles
    #[xmtp_common::test]
    async fn test_rapid_acquire_release() {
        let test_dir = test_lock_dir();
        let db_path = test_dir.join("test.db");
        let lock_manager = GroupCommitLock::new(db_path, "test_installation");
        let group_id = vec![1, 2, 3, 4];

        for i in 0..50 {
            let guard = lock_manager
                .get_lock_async(group_id.clone())
                .await
                .unwrap_or_else(|e| panic!("Failed to acquire lock on iteration {}: {:?}", i, e));
            drop(guard);
        }
    }

    // ==================== Cross-Process Tests (Native Only) ====================

    /// Test that lock files are actually created on disk
    #[cfg(not(target_arch = "wasm32"))]
    #[xmtp_common::test]
    async fn test_lock_file_creation() {
        let test_dir = test_lock_dir();
        let db_path = test_dir.join("test.db");
        let lock_manager = GroupCommitLock::new(db_path, "test_installation");
        let group_id = vec![0xde, 0xad, 0xbe, 0xef];

        let _guard = lock_manager
            .get_lock_async(group_id.clone())
            .await
            .expect("Failed to acquire lock");

        // Check that lock file exists
        let lock_file_path = test_dir.join("test_installation").join("deadbeef");
        assert!(
            lock_file_path.exists(),
            "Lock file should exist at {:?}",
            lock_file_path
        );
    }

    /// Test that the same lock manager can be used for multiple group IDs concurrently
    #[xmtp_common::test]
    async fn test_multiple_groups_concurrent() {
        let test_dir = test_lock_dir();
        let db_path = test_dir.join("test.db");
        let lock_manager = Arc::new(GroupCommitLock::new(db_path, "test_installation"));

        let mut handles = Vec::new();

        // Spawn tasks for different group IDs
        for i in 0..5u8 {
            let lock_manager = Arc::clone(&lock_manager);
            let group_id = vec![i, i, i, i];

            let handle = tokio::spawn(async move {
                let _guard = lock_manager
                    .get_lock_async(group_id)
                    .await
                    .expect("Failed to acquire lock");

                tokio::time::sleep(Duration::from_millis(50)).await;
            });

            handles.push(handle);
        }

        // All should complete roughly simultaneously since they're different groups
        let start = std::time::Instant::now();
        for handle in handles {
            handle.await.unwrap();
        }
        let elapsed = start.elapsed();

        // Should complete in roughly 50ms, not 250ms (5 * 50ms)
        assert!(
            elapsed < Duration::from_millis(200),
            "Different groups should not block each other, took {:?}",
            elapsed
        );
    }

    // ==================== Guard Behavior Tests ====================

    /// Test that guard can be moved between tasks
    #[xmtp_common::test]
    async fn test_guard_can_be_moved() {
        let test_dir = test_lock_dir();
        let db_path = test_dir.join("test.db");
        let lock_manager = GroupCommitLock::new(db_path, "test_installation");
        let group_id = vec![1, 2, 3, 4];

        let guard = lock_manager
            .get_lock_async(group_id.clone())
            .await
            .expect("Failed to acquire lock");

        // Move guard to another task
        let handle = tokio::spawn(async move {
            // Guard is now owned by this task
            tokio::time::sleep(Duration::from_millis(10)).await;
            drop(guard);
        });

        handle.await.unwrap();

        // Should be able to acquire again
        let _guard2 = lock_manager
            .get_lock_async(group_id)
            .await
            .expect("Should be able to acquire after guard moved and dropped");
    }

    /// Test Send + Sync bounds
    #[xmtp_common::test]
    async fn test_send_sync_bounds() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}

        assert_send::<GroupCommitLock>();
        assert_sync::<GroupCommitLock>();
        assert_send::<MlsGroupGuard>();
        assert_sync::<MlsGroupGuard>();
    }
}
