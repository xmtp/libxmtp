use std::sync::Arc;
use std::time::Duration;

use crate::context::XmtpSharedContext;
use crate::groups::GroupError;
use crate::tester;

#[xmtp_common::test]
async fn test_load_mls_group_with_lock_returns_error_when_locked() {
    tester!(alix);

    let group = alix.create_group(None, None).unwrap();
    let group_clone = group.clone();

    // Acquire the lock in a background task and hold it
    let (done_tx, done_rx) = tokio::sync::oneshot::channel::<()>();
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel::<()>();

    let handle = tokio::spawn(async move {
        group_clone
            .load_mls_group_with_lock_async(|_mls_group| async move {
                // Signal that the lock is held
                let _ = ready_tx.send(());
                // Hold the lock until signaled to release
                let _ = done_rx.await;
                Ok::<_, GroupError>(())
            })
            .await
            .unwrap();
    });

    // Wait for the lock to be acquired
    ready_rx.await.unwrap();

    // Now try to use load_mls_group_with_lock - it should return LockUnavailable error
    let storage = alix.context.mls_storage();
    let result = group.load_mls_group_with_lock(storage, |_mls_group| Ok(()));

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

/// Test that `load_mls_group_with_lock_async` waits for the lock to be released
#[xmtp_common::test(unwrap_try = true)]
async fn test_load_mls_group_with_lock_async_waits_for_lock() {
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

    let handle = tokio::spawn(async move {
        // Acquire the lock
        let _guard = commit_lock.get_lock_async(group_id).await;
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

    // Now try to use load_mls_group_with_lock_async - it should wait
    let execution_order_clone2 = execution_order.clone();
    let result: Result<(), GroupError> = group
        .load_mls_group_with_lock_async(|_mls_group| async move {
            execution_order_clone2.lock().unwrap().push(3);
            Ok(())
        })
        .await;

    assert!(result.is_ok(), "Expected success, got: {:?}", result);

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

/// Test that concurrent calls to load_mls_group_with_lock on different groups work independently
#[xmtp_common::test(unwrap_try = true)]
async fn test_load_mls_group_with_lock_different_groups_independent() {
    tester!(alix);

    let group1 = alix.create_group(None, None)?;
    let group2 = alix.create_group(None, None)?;
    let group1_clone = group1.clone();

    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel::<()>();

    let handle = tokio::spawn(async move {
        group1_clone
            .load_mls_group_with_lock_async(|_mls_group| async move {
                let _ = ready_tx.send(());
                let _ = rx.await;
                Ok::<_, GroupError>(())
            })
            .await
            .unwrap();
    });

    ready_rx.await.unwrap();

    // group1 should be locked
    let storage = alix.context.mls_storage();
    let result1 = group1.load_mls_group(storage, |_| Ok(()));
    assert!(matches!(result1, Err(GroupError::LockUnavailable)));

    // group2 should NOT be locked - it's a different group
    let result2 = group2.load_mls_group(storage, |_| Ok(()));
    assert!(
        result2.is_ok(),
        "group2 should not be locked: {:?}",
        result2
    );

    let _ = tx.send(());
    handle.await.unwrap();
}
