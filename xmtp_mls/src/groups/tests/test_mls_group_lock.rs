use std::sync::Arc;
use std::time::Duration;

use crate::context::XmtpSharedContext;
use crate::groups::GroupError;
use crate::tester;

#[xmtp_common::test]
async fn test_lock_sync_returns_error_when_locked() {
    tester!(alix);

    let group = alix.create_group(None, None).unwrap();

    let commit_lock = alix.context.mls_commit_lock().clone();
    let group_id = group.group_id.clone();

    // Acquire the lock in a background task and hold it
    let (done_tx, done_rx) = tokio::sync::oneshot::channel::<()>();
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel::<()>();

    let commit_lock_clone = commit_lock.clone();
    let group_id_clone = group_id.clone();

    let handle = tokio::spawn(async move {
        let _guard = commit_lock_clone.get_lock_async(group_id_clone).await;
        // Signal that the lock is held
        let _ = ready_tx.send(());
        // Hold the lock until signaled to release
        let _ = done_rx.await;
    });

    // Wait for the lock to be acquired
    ready_rx.await.unwrap();

    // Now try to acquire lock synchronously - it should return LockUnavailable error
    let result = commit_lock.get_lock_sync(group_id);

    // The sync version uses try_lock, so it should fail with LockUnavailable
    assert!(
        matches!(result, Err(GroupError::LockUnavailable)),
        "Expected LockUnavailable error, got: {:?}",
        result
    );

    // Release the lock
    let _ = done_tx.send(());
    handle.await.unwrap();
}

/// Test that `get_lock_async` waits for the lock to be released
#[xmtp_common::test(unwrap_try = true)]
async fn test_lock_async_waits_for_lock() {
    tester!(alix);

    let group = alix.create_group(None, None)?;

    // Clone what we need for the spawned task
    let group_id = group.group_id.clone();
    let commit_lock = alix.context.mls_commit_lock().clone();

    // Track execution order
    let execution_order = Arc::new(std::sync::Mutex::new(Vec::new()));
    let execution_order_clone = execution_order.clone();

    // Acquire the lock in a background task and hold it briefly
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel::<()>();

    let commit_lock_clone = commit_lock.clone();
    let group_id_clone = group_id.clone();

    let handle = tokio::spawn(async move {
        // Acquire the lock
        let _guard = commit_lock_clone.get_lock_async(group_id_clone).await;
        execution_order_clone.lock().unwrap().push(1);
        // Signal that the lock is held
        let _ = ready_tx.send(());
        // Hold the lock for a short time
        tokio::time::sleep(Duration::from_millis(100)).await;
        execution_order_clone.lock().unwrap().push(2);
        // Lock is released when _guard is dropped
    });

    // Wait for the lock to be acquired
    ready_rx.await.unwrap();

    // Now try to acquire lock async - it should wait
    let execution_order_clone2 = execution_order.clone();

    let _guard = commit_lock.get_lock_async(group_id).await;
    execution_order_clone2.lock().unwrap().push(3);

    handle.await.unwrap();

    // Verify execution order: first task should complete (1, 2) before second task runs (3)
    let order = execution_order.lock().unwrap();
    assert_eq!(
        *order,
        vec![1, 2, 3],
        "Expected execution order [1, 2, 3], got {:?}",
        *order
    );
}

/// Test that locking different groups works independently
#[xmtp_common::test(unwrap_try = true)]
async fn test_lock_different_groups_independent() {
    tester!(alix);

    let group1 = alix.create_group(None, None)?;
    let group2 = alix.create_group(None, None)?;

    let commit_lock = alix.context.mls_commit_lock().clone();

    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel::<()>();

    let commit_lock_clone = commit_lock.clone();
    let group1_id = group1.group_id.clone();

    let handle = tokio::spawn(async move {
        let _guard = commit_lock_clone.get_lock_async(group1_id).await;
        let _ = ready_tx.send(());
        let _ = rx.await;
    });

    ready_rx.await.unwrap();

    // group1 should be locked (sync acquisition fails)
    let result1 = commit_lock.get_lock_sync(group1.group_id.clone());
    assert!(matches!(result1, Err(GroupError::LockUnavailable)));

    // group2 should NOT be locked - it's a different group
    let result2 = commit_lock.get_lock_sync(group2.group_id.clone());
    assert!(
        result2.is_ok(),
        "group2 should not be locked: {:?}",
        result2
    );

    let _ = tx.send(());
    handle.await.unwrap();
}
