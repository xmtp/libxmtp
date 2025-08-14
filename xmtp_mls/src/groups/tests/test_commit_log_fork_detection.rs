use crate::groups::commit_log::{CommitLogTestFunction, CommitLogWorker};
use crate::tester;
use xmtp_db::Store;
use xmtp_db::encrypted_store::local_commit_log::NewLocalCommitLog;
use xmtp_db::encrypted_store::remote_commit_log::{CommitResult, NewRemoteCommitLog};
use xmtp_db::local_commit_log::CommitType;
use xmtp_db::prelude::*;

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
    let initial_local_cursor = alix.context.db().get_last_cursor_for_id(
        &group_id,
        xmtp_db::refresh_state::EntityKind::CommitLogForkCheckLocal,
    )?;
    let initial_remote_cursor = alix.context.db().get_last_cursor_for_id(
        &group_id,
        xmtp_db::refresh_state::EntityKind::CommitLogForkCheckRemote,
    )?;

    assert_eq!(initial_local_cursor, 0);
    assert_eq!(initial_remote_cursor, 0);

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
    let updated_local_cursor = alix.context.db().get_last_cursor_for_id(
        &group_id,
        xmtp_db::refresh_state::EntityKind::CommitLogForkCheckLocal,
    )?;
    let updated_remote_cursor = alix.context.db().get_last_cursor_for_id(
        &group_id,
        xmtp_db::refresh_state::EntityKind::CommitLogForkCheckRemote,
    )?;

    // Cursors should be updated to the rowids of the matching entries
    assert!(updated_local_cursor > 0, "Local cursor should be updated");
    assert!(updated_remote_cursor > 0, "Remote cursor should be updated");

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
    let updated_two_local_cursor = alix.context.db().get_last_cursor_for_id(
        &group_id,
        xmtp_db::refresh_state::EntityKind::CommitLogForkCheckLocal,
    )?;
    let updated_two_remote_cursor = alix.context.db().get_last_cursor_for_id(
        &group_id,
        xmtp_db::refresh_state::EntityKind::CommitLogForkCheckRemote,
    )?;
    let latest_two_local_log = alix.context.db().get_latest_log_for_group(&group_id)?;
    let latest_two_remote_log = alix
        .context
        .db()
        .get_latest_remote_log_for_group(&group_id)?;

    assert_eq!(
        updated_two_local_cursor,
        latest_two_local_log.unwrap().rowid as i64
    );
    assert_eq!(
        updated_two_remote_cursor,
        latest_two_remote_log.unwrap().rowid as i64
    );

    // Verify that the cursor positions are different
    assert!(updated_two_local_cursor > updated_local_cursor);
    assert!(updated_two_remote_cursor > updated_remote_cursor);

    Ok(())
}

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
