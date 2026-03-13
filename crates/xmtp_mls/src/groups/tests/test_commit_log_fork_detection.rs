use crate::groups::commit_log::{CommitLogTestFunction, CommitLogWorker};
use crate::tester;
use xmtp_configuration::Originators;
use xmtp_db::Store;
use xmtp_db::encrypted_store::local_commit_log::NewLocalCommitLog;
use xmtp_db::encrypted_store::remote_commit_log::{CommitResult, NewRemoteCommitLog};
use xmtp_db::local_commit_log::CommitType;
use xmtp_db::prelude::*;
use xmtp_proto::types::Cursor;

#[cfg_attr(all(feature = "d14n", target_arch = "wasm32"), ignore)]
#[xmtp_common::test(unwrap_try = true)]
async fn test_commit_log_fork_detection_no_fork() -> Result<(), Box<dyn std::error::Error>> {
    tester!(alix);
    let group = alix.create_group(None, None).unwrap();
    let group_id = group.group_id.clone();

    // Insert local commit log entries
    let local_entry_1 = NewLocalCommitLog {
        group_id: group_id.clone(),
        commit_sequence_id: 1,
        last_epoch_authenticator: vec![0x11, 0x22, 0x33],
        commit_result: CommitResult::Success,
        error_message: None,
        applied_epoch_number: 1,
        applied_epoch_authenticator: vec![0xAA, 0xBB, 0xCC],
        sender_inbox_id: None,
        sender_installation_id: None,
        commit_type: None,
    };

    let local_entry_2 = NewLocalCommitLog {
        group_id: group_id.clone(),
        commit_sequence_id: 2,
        last_epoch_authenticator: vec![0xAA, 0xBB, 0xCC],
        commit_result: CommitResult::Success,
        error_message: None,
        applied_epoch_number: 2,
        applied_epoch_authenticator: vec![0xDD, 0xEE, 0xFF],
        sender_inbox_id: None,
        sender_installation_id: None,
        commit_type: None,
    };

    local_entry_1.store(&alix.context.db())?;
    local_entry_2.store(&alix.context.db())?;

    // Insert matching remote commit log entries (no fork)
    let remote_entry_1 = NewRemoteCommitLog {
        log_sequence_id: 100,
        group_id: group_id.clone(),
        commit_sequence_id: 1,
        commit_result: CommitResult::Success,
        applied_epoch_number: 1,
        applied_epoch_authenticator: vec![0xAA, 0xBB, 0xCC], // Same as local
    };

    let remote_entry_2 = NewRemoteCommitLog {
        log_sequence_id: 101,
        group_id: group_id.clone(),
        commit_sequence_id: 2,
        commit_result: CommitResult::Success,
        applied_epoch_number: 2,
        applied_epoch_authenticator: vec![0xDD, 0xEE, 0xFF], // Same as local
    };

    remote_entry_1.store(&alix.context.db())?;
    remote_entry_2.store(&alix.context.db())?;

    // Test fork detection
    let mut worker = CommitLogWorker::new(alix.context.clone());
    let results = worker
        .run_test(CommitLogTestFunction::CheckForkedState, None)
        .await
        .unwrap();

    // Should detect no fork
    assert_eq!(results.len(), 1);
    let result = &results[0];
    assert!(result.is_forked.is_some());
    let fork_status = result.is_forked.as_ref().unwrap().get(&group_id).unwrap();
    assert_eq!(*fork_status, Some(false), "Should detect no fork");
    Ok(())
}

#[cfg_attr(all(feature = "d14n", target_arch = "wasm32"), ignore)]
#[xmtp_common::test(unwrap_try = true)]
async fn test_commit_log_fork_detection_forked() -> Result<(), Box<dyn std::error::Error>> {
    tester!(alix);
    let group = alix.create_group(None, None).unwrap();
    let group_id = group.group_id.clone();

    // Insert local commit log entries
    let local_entry_1 = NewLocalCommitLog {
        group_id: group_id.clone(),
        commit_sequence_id: 200,
        last_epoch_authenticator: vec![0x11, 0x22, 0x33],
        commit_result: CommitResult::Success,
        error_message: None,
        applied_epoch_number: 1,
        applied_epoch_authenticator: vec![0xAA, 0xBB, 0xCC],
        sender_inbox_id: None,
        sender_installation_id: None,
        commit_type: None,
    };

    let local_entry_2 = NewLocalCommitLog {
        group_id: group_id.clone(),
        commit_sequence_id: 201,
        last_epoch_authenticator: vec![0xAA, 0xBB, 0xCC],
        commit_result: CommitResult::Success,
        error_message: None,
        applied_epoch_number: 2,
        applied_epoch_authenticator: vec![0xDD, 0xEE, 0xFF],
        sender_inbox_id: None,
        sender_installation_id: None,
        commit_type: None,
    };

    local_entry_1.store(&alix.context.db())?;
    local_entry_2.store(&alix.context.db())?;

    // Insert matching remote commit log entries (no fork)
    let remote_entry_1 = NewRemoteCommitLog {
        log_sequence_id: 100,
        group_id: group_id.clone(),
        commit_sequence_id: 200,
        commit_result: CommitResult::Invalid, // For some reason remote marked this commit invalid
        applied_epoch_number: 1,
        applied_epoch_authenticator: vec![0xAA, 0xBB, 0xCC], // Same as local
    };

    let remote_entry_2 = NewRemoteCommitLog {
        log_sequence_id: 101,
        group_id: group_id.clone(),
        commit_sequence_id: 200,
        commit_result: CommitResult::Success,
        applied_epoch_number: 1,
        applied_epoch_authenticator: vec![0xCD, 0xDE, 0xEF], // Different from local
    };

    remote_entry_1.store(&alix.context.db())?;
    remote_entry_2.store(&alix.context.db())?;

    // Test fork detection
    let mut worker = CommitLogWorker::new(alix.context.clone());
    let results = worker
        .run_test(CommitLogTestFunction::CheckForkedState, None)
        .await?;

    // Should detect a fork
    assert_eq!(results.len(), 1);
    let result = &results[0];
    assert!(result.is_forked.is_some());
    let fork_status = result.is_forked.as_ref().unwrap().get(&group_id).unwrap();
    assert_eq!(*fork_status, Some(true), "Should detect a fork");

    Ok(())
}

#[cfg_attr(all(feature = "d14n", target_arch = "wasm32"), ignore)]
#[xmtp_common::test(unwrap_try = true)]
async fn test_commit_log_fork_detection_cursor_updates() -> Result<(), Box<dyn std::error::Error>> {
    tester!(alix);
    let group = alix.create_group(None, None).unwrap();
    let group_id = group.group_id.clone();

    // Insert local commit log entry
    let local_entry = NewLocalCommitLog {
        group_id: group_id.clone(),
        commit_sequence_id: 1,
        last_epoch_authenticator: vec![0x11, 0x22, 0x33],
        commit_result: CommitResult::Success,
        error_message: None,
        applied_epoch_number: 1,
        applied_epoch_authenticator: vec![0xAA, 0xBB, 0xCC],
        sender_inbox_id: None,
        sender_installation_id: None,
        commit_type: None,
    };

    local_entry.store(&alix.context.db())?;

    // Insert matching remote commit log entry with same authenticator (should update cursors)
    let remote_entry = NewRemoteCommitLog {
        log_sequence_id: 100,
        group_id: group_id.clone(),
        commit_sequence_id: 1, // Same commit_sequence_id
        commit_result: CommitResult::Success,
        applied_epoch_number: 1,
        applied_epoch_authenticator: vec![0xAA, 0xBB, 0xCC], // Same authenticator
    };

    remote_entry.store(&alix.context.db())?;

    // Get initial cursor values (should be 0)
    let initial_local_cursor = alix.context.db().get_last_cursor_for_originator(
        &group_id,
        xmtp_db::refresh_state::EntityKind::CommitLogForkCheckLocal,
        Originators::REMOTE_COMMIT_LOG,
    )?;
    let initial_remote_cursor = alix.context.db().get_last_cursor_for_originator(
        &group_id,
        xmtp_db::refresh_state::EntityKind::CommitLogForkCheckRemote,
        Originators::REMOTE_COMMIT_LOG,
    )?;

    assert_eq!(initial_local_cursor, Cursor::commit_log(0));
    assert_eq!(initial_remote_cursor, Cursor::commit_log(0));

    // Test fork detection
    let mut worker = CommitLogWorker::new(alix.context.clone());
    let results = worker
        .run_test(CommitLogTestFunction::CheckForkedState, None)
        .await
        .unwrap();

    // Should detect no fork
    assert_eq!(results.len(), 1);
    let result = &results[0];
    assert!(result.is_forked.is_some());
    let fork_status = result.is_forked.as_ref().unwrap().get(&group_id).unwrap();
    assert_eq!(
        *fork_status,
        Some(false),
        "Should detect no fork when authenticators match"
    );

    // Verify cursors were updated
    let updated_local_cursor = alix.context.db().get_last_cursor_for_originator(
        &group_id,
        xmtp_db::refresh_state::EntityKind::CommitLogForkCheckLocal,
        Originators::REMOTE_COMMIT_LOG,
    )?;
    let updated_remote_cursor = alix.context.db().get_last_cursor_for_originator(
        &group_id,
        xmtp_db::refresh_state::EntityKind::CommitLogForkCheckRemote,
        Originators::REMOTE_COMMIT_LOG,
    )?;

    // Cursors should be updated to the rowids of the matching entries
    assert!(
        updated_local_cursor > Cursor::commit_log(0),
        "Local cursor should be updated"
    );
    assert!(
        updated_remote_cursor > Cursor::commit_log(0),
        "Remote cursor should be updated"
    );

    // Insert local commit log entry
    let local_entry = NewLocalCommitLog {
        group_id: group_id.clone(),
        commit_sequence_id: 2,
        last_epoch_authenticator: vec![0x11, 0x22, 0x33],
        commit_result: CommitResult::Success,
        error_message: None,
        applied_epoch_number: 2,
        applied_epoch_authenticator: vec![0xDD, 0xEE, 0xFF],
        sender_inbox_id: None,
        sender_installation_id: None,
        commit_type: Some(CommitType::UpdateAdminList.to_string()),
    };

    local_entry.store(&alix.context.db())?;

    // Insert matching remote commit log entry with same authenticator (should update cursors)
    let remote_entry = NewRemoteCommitLog {
        log_sequence_id: 101,
        group_id: group_id.clone(),
        commit_sequence_id: 2, // Same commit_sequence_id
        commit_result: CommitResult::Success,
        applied_epoch_number: 2,
        applied_epoch_authenticator: vec![0xDD, 0xEE, 0x11], // different authenticator
    };

    remote_entry.store(&alix.context.db())?;

    // Test fork detection
    let mut worker = CommitLogWorker::new(alix.context.clone());
    let results = worker
        .run_test(CommitLogTestFunction::CheckForkedState, None)
        .await
        .unwrap();

    // Should detect a fork
    assert_eq!(results.len(), 1);
    let result = &results[0];
    assert!(result.is_forked.is_some());
    let fork_status = result.is_forked.as_ref().unwrap().get(&group_id).unwrap();
    assert_eq!(
        *fork_status,
        Some(true),
        "Should detect a fork when authenticators differ"
    );

    // Verify cursors were updated
    let updated_two_local_cursor = alix.context.db().get_last_cursor_for_originator(
        &group_id,
        xmtp_db::refresh_state::EntityKind::CommitLogForkCheckLocal,
        Originators::REMOTE_COMMIT_LOG,
    )?;
    let updated_two_remote_cursor = alix.context.db().get_last_cursor_for_originator(
        &group_id,
        xmtp_db::refresh_state::EntityKind::CommitLogForkCheckRemote,
        Originators::REMOTE_COMMIT_LOG,
    )?;
    let latest_two_local_log = alix.context.db().get_latest_log_for_group(&group_id)?;
    let latest_two_remote_log = alix
        .context
        .db()
        .get_latest_remote_log_for_group(&group_id)?;

    assert_eq!(
        updated_two_local_cursor,
        Cursor::commit_log(latest_two_local_log.unwrap().rowid as u64)
    );
    assert_eq!(
        updated_two_remote_cursor,
        Cursor::commit_log(latest_two_remote_log.unwrap().rowid as u64)
    );

    // Verify that the cursor positions are different
    assert!(updated_two_local_cursor > updated_local_cursor);
    assert!(updated_two_remote_cursor > updated_remote_cursor);

    Ok(())
}

#[cfg_attr(all(feature = "d14n", target_arch = "wasm32"), ignore)]
#[xmtp_common::test(unwrap_try = true)]
async fn test_commit_log_fork_detection_returns_none_when_no_matching_remote()
-> Result<(), Box<dyn std::error::Error>> {
    tester!(alix);
    let group = alix.create_group(None, None).unwrap();
    let group_id = group.group_id.clone();

    // Insert local commit log entries
    let local_entry_1 = NewLocalCommitLog {
        group_id: group_id.clone(),
        commit_sequence_id: 1,
        last_epoch_authenticator: vec![0x11, 0x22, 0x33],
        commit_result: CommitResult::Success,
        error_message: None,
        applied_epoch_number: 1,
        applied_epoch_authenticator: vec![0xAA, 0xBB, 0xCC],
        sender_inbox_id: None,
        sender_installation_id: None,
        commit_type: None,
    };

    let local_entry_2 = NewLocalCommitLog {
        group_id: group_id.clone(),
        commit_sequence_id: 2,
        last_epoch_authenticator: vec![0xAA, 0xBB, 0xCC],
        commit_result: CommitResult::Success,
        error_message: None,
        applied_epoch_number: 2,
        applied_epoch_authenticator: vec![0xDD, 0xEE, 0xFF],
        sender_inbox_id: None,
        sender_installation_id: None,
        commit_type: None,
    };

    local_entry_1.store(&alix.context.db())?;
    local_entry_2.store(&alix.context.db())?;

    // Insert remote commit log entries with different commit_sequence_ids (no match for latest local)
    let remote_entry = NewRemoteCommitLog {
        log_sequence_id: 100,
        group_id: group_id.clone(),
        commit_sequence_id: 1, // Only matches first local entry
        commit_result: CommitResult::Success,
        applied_epoch_number: 1,
        applied_epoch_authenticator: vec![0xAA, 0xBB, 0xCC], // Same as local
    };

    remote_entry.store(&alix.context.db())?;
    // Note: No remote entry for commit_sequence_id 2

    // Test fork detection
    let mut worker = CommitLogWorker::new(alix.context.clone());
    let results = worker
        .run_test(CommitLogTestFunction::CheckForkedState, None)
        .await
        .unwrap();

    // Should return None because latest local commit has no matching remote entry
    assert_eq!(results.len(), 1);
    let result = &results[0];
    assert!(result.is_forked.is_some());
    let fork_status = result.is_forked.as_ref().unwrap().get(&group_id).unwrap();
    assert_eq!(
        *fork_status, None,
        "Should return None when latest local commit has no matching remote entry"
    );

    Ok(())
}

#[cfg_attr(all(feature = "d14n", target_arch = "wasm32"), ignore)]
#[xmtp_common::test(unwrap_try = true)]
async fn test_commit_log_fork_status_persistence_no_new_commits()
-> Result<(), Box<dyn std::error::Error>> {
    tester!(alix);
    let group = alix.create_group(None, None).unwrap();
    let group_id = group.group_id.clone();

    // Insert local commit log entries
    let local_entry_1 = NewLocalCommitLog {
        group_id: group_id.clone(),
        commit_sequence_id: 1,
        last_epoch_authenticator: vec![0x11, 0x22, 0x33],
        commit_result: CommitResult::Success,
        error_message: None,
        applied_epoch_number: 1,
        applied_epoch_authenticator: vec![0xAA, 0xBB, 0xCC],
        sender_inbox_id: None,
        sender_installation_id: None,
        commit_type: None,
    };

    let local_entry_2 = NewLocalCommitLog {
        group_id: group_id.clone(),
        commit_sequence_id: 2,
        last_epoch_authenticator: vec![0xAA, 0xBB, 0xCC],
        commit_result: CommitResult::Success,
        error_message: None,
        applied_epoch_number: 2,
        applied_epoch_authenticator: vec![0xDD, 0xEE, 0xFF],
        sender_inbox_id: None,
        sender_installation_id: None,
        commit_type: None,
    };

    local_entry_1.store(&alix.context.db())?;
    local_entry_2.store(&alix.context.db())?;

    // Insert matching remote commit log entries (no fork)
    let remote_entry_1 = NewRemoteCommitLog {
        log_sequence_id: 100,
        group_id: group_id.clone(),
        commit_sequence_id: 1,
        commit_result: CommitResult::Success,
        applied_epoch_number: 1,
        applied_epoch_authenticator: vec![0xAA, 0xBB, 0xCC], // Same as local
    };

    let remote_entry_2 = NewRemoteCommitLog {
        log_sequence_id: 101,
        group_id: group_id.clone(),
        commit_sequence_id: 2,
        commit_result: CommitResult::Success,
        applied_epoch_number: 2,
        applied_epoch_authenticator: vec![0xDD, 0xEE, 0xFF], // Same as local
    };

    remote_entry_1.store(&alix.context.db())?;
    remote_entry_2.store(&alix.context.db())?;

    // First fork detection run - should detect no fork and set status to Some(false)
    let mut worker = CommitLogWorker::new(alix.context.clone());
    let results = worker
        .run_test(CommitLogTestFunction::All, None)
        .await
        .unwrap();

    // Verify initial fork status is set to Some(false)
    assert_eq!(results.len(), 1);
    let result = &results[0];
    assert!(result.is_forked.is_some());
    let fork_status = result.is_forked.as_ref().unwrap().get(&group_id).unwrap();
    assert_eq!(*fork_status, Some(false), "Should initially detect no fork");

    // Verify the status is persisted in the database
    let db_fork_status = alix
        .context
        .db()
        .get_group_commit_log_forked_status(&group_id)?;
    assert_eq!(
        db_fork_status,
        Some(false),
        "Fork status should be persisted as Some(false) in database"
    );

    // Second fork detection run - no new commits have been added
    // This should preserve the existing fork status (Some(false))
    let results_second = worker
        .run_test(CommitLogTestFunction::All, None)
        .await
        .unwrap();

    // Verify fork status remains Some(false) and doesn't get reset to None
    assert_eq!(results_second.len(), 1);
    let result_second = &results_second[0];
    assert!(result_second.is_forked.is_some());
    let fork_status_second = result_second
        .is_forked
        .as_ref()
        .unwrap()
        .get(&group_id)
        .unwrap();
    assert_eq!(
        *fork_status_second,
        Some(false),
        "Fork status should remain Some(false) when no new commits"
    );

    // Verify the status is still persisted correctly in the database
    let db_fork_status_second = alix
        .context
        .db()
        .get_group_commit_log_forked_status(&group_id)?;
    assert_eq!(
        db_fork_status_second,
        Some(false),
        "Fork status should remain Some(false) in database"
    );

    // Third fork detection run - still no new commits
    // This should continue to preserve the existing fork status
    let results_third = worker
        .run_test(CommitLogTestFunction::CheckForkedState, None)
        .await
        .unwrap();

    // Verify fork status still remains Some(false)
    assert_eq!(results_third.len(), 1);
    let result_third = &results_third[0];
    assert!(result_third.is_forked.is_some());
    let fork_status_third = result_third
        .is_forked
        .as_ref()
        .unwrap()
        .get(&group_id)
        .unwrap();
    assert_eq!(
        *fork_status_third,
        Some(false),
        "Fork status should persist across multiple checks with no new commits"
    );

    // Final verification from database
    let db_fork_status_final = alix
        .context
        .db()
        .get_group_commit_log_forked_status(&group_id)?;
    assert_eq!(
        db_fork_status_final,
        Some(false),
        "Final database check: fork status should remain Some(false)"
    );

    Ok(())
}
