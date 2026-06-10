use crate::context::XmtpSharedContext;
use crate::groups::app_data::process_message_with_app_data;
use crate::groups::commit_log::{CommitLogTestFunction, CommitLogWorker};
use crate::groups::mls_ext::CommitLogStorer;
use crate::groups::validated_commit::ValidatedCommit;
use crate::tester;
use openmls::group::MlsGroup as OpenMlsGroup;
use openmls::prelude::{ProcessedMessageContent, Sender};
use xmtp_configuration::Originators;
use xmtp_db::Store;
use xmtp_db::encrypted_store::local_commit_log::NewLocalCommitLog;
use xmtp_db::encrypted_store::remote_commit_log::{CommitResult, NewRemoteCommitLog};
use xmtp_db::local_commit_log::CommitType;
use xmtp_db::prelude::*;
use xmtp_db::{MlsProviderExt, StorageError, TransactionalKeyStore, XmtpOpenMlsProvider};
use xmtp_proto::types::Cursor;

#[cfg_attr(all(feature = "d14n", target_arch = "wasm32"), ignore)]
#[xmtp_common::test(unwrap_try = true)]
async fn test_commit_log_fork_detection_no_fork() -> Result<(), Box<dyn std::error::Error>> {
    tester!(alix);
    let group = alix.create_group(None, None).unwrap();
    let group_id = group.group_id;

    // Insert local commit log entries
    let local_entry_1 = NewLocalCommitLog {
        group_id,
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
        group_id,
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
        group_id,
        commit_sequence_id: 1,
        commit_result: CommitResult::Success,
        applied_epoch_number: 1,
        applied_epoch_authenticator: vec![0xAA, 0xBB, 0xCC], // Same as local
    };

    let remote_entry_2 = NewRemoteCommitLog {
        log_sequence_id: 101,
        group_id,
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
    let fork_status = result
        .is_forked
        .as_ref()
        .unwrap()
        .get(group_id.as_ref())
        .unwrap();
    assert_eq!(*fork_status, Some(false), "Should detect no fork");
    Ok(())
}

#[cfg_attr(all(feature = "d14n", target_arch = "wasm32"), ignore)]
#[xmtp_common::test(unwrap_try = true)]
async fn test_commit_log_fork_detection_forked() -> Result<(), Box<dyn std::error::Error>> {
    tester!(alix);
    let group = alix.create_group(None, None).unwrap();
    let group_id = group.group_id;

    // Insert local commit log entries
    let local_entry_1 = NewLocalCommitLog {
        group_id,
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
        group_id,
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
        group_id,
        commit_sequence_id: 200,
        commit_result: CommitResult::Invalid, // For some reason remote marked this commit invalid
        applied_epoch_number: 1,
        applied_epoch_authenticator: vec![0xAA, 0xBB, 0xCC], // Same as local
    };

    let remote_entry_2 = NewRemoteCommitLog {
        log_sequence_id: 101,
        group_id,
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
    let fork_status = result
        .is_forked
        .as_ref()
        .unwrap()
        .get(group_id.as_ref())
        .unwrap();
    assert_eq!(*fork_status, Some(true), "Should detect a fork");

    Ok(())
}

#[cfg_attr(all(feature = "d14n", target_arch = "wasm32"), ignore)]
#[xmtp_common::test(unwrap_try = true)]
async fn test_commit_log_fork_detection_cursor_updates() -> Result<(), Box<dyn std::error::Error>> {
    tester!(alix);
    let group = alix.create_group(None, None).unwrap();
    let group_id = group.group_id;

    // Insert local commit log entry
    let local_entry = NewLocalCommitLog {
        group_id,
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
        group_id,
        commit_sequence_id: 1, // Same commit_sequence_id
        commit_result: CommitResult::Success,
        applied_epoch_number: 1,
        applied_epoch_authenticator: vec![0xAA, 0xBB, 0xCC], // Same authenticator
    };

    remote_entry.store(&alix.context.db())?;

    // Get initial cursor values (should be 0)
    let initial_local_cursor = alix.context.db().get_last_cursor_for_originator(
        group_id,
        xmtp_db::refresh_state::EntityKind::CommitLogForkCheckLocal,
        Originators::REMOTE_COMMIT_LOG,
    )?;
    let initial_remote_cursor = alix.context.db().get_last_cursor_for_originator(
        group_id,
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
    let fork_status = result
        .is_forked
        .as_ref()
        .unwrap()
        .get(group_id.as_ref())
        .unwrap();
    assert_eq!(
        *fork_status,
        Some(false),
        "Should detect no fork when authenticators match"
    );

    // Verify cursors were updated
    let updated_local_cursor = alix.context.db().get_last_cursor_for_originator(
        group_id,
        xmtp_db::refresh_state::EntityKind::CommitLogForkCheckLocal,
        Originators::REMOTE_COMMIT_LOG,
    )?;
    let updated_remote_cursor = alix.context.db().get_last_cursor_for_originator(
        group_id,
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
        group_id,
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
        group_id,
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
    let fork_status = result
        .is_forked
        .as_ref()
        .unwrap()
        .get(group_id.as_ref())
        .unwrap();
    assert_eq!(
        *fork_status,
        Some(true),
        "Should detect a fork when authenticators differ"
    );

    // Verify cursors were updated
    let updated_two_local_cursor = alix.context.db().get_last_cursor_for_originator(
        group_id,
        xmtp_db::refresh_state::EntityKind::CommitLogForkCheckLocal,
        Originators::REMOTE_COMMIT_LOG,
    )?;
    let updated_two_remote_cursor = alix.context.db().get_last_cursor_for_originator(
        group_id,
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
    let group_id = group.group_id;

    // Insert local commit log entries
    let local_entry_1 = NewLocalCommitLog {
        group_id,
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
        group_id,
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
        group_id,
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
    let fork_status = result
        .is_forked
        .as_ref()
        .unwrap()
        .get(group_id.as_ref())
        .unwrap();
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
    let group_id = group.group_id;

    // Insert local commit log entries
    let local_entry_1 = NewLocalCommitLog {
        group_id,
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
        group_id,
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
        group_id,
        commit_sequence_id: 1,
        commit_result: CommitResult::Success,
        applied_epoch_number: 1,
        applied_epoch_authenticator: vec![0xAA, 0xBB, 0xCC], // Same as local
    };

    let remote_entry_2 = NewRemoteCommitLog {
        log_sequence_id: 101,
        group_id,
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
    let fork_status = result
        .is_forked
        .as_ref()
        .unwrap()
        .get(group_id.as_ref())
        .unwrap();
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
        .get(group_id.as_ref())
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
        .get(group_id.as_ref())
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

/// Regression test for the fork-detection false positive observed in
/// production (Convos group `e08a717548dc996845dc1f2d6986bdd0`, bundle
/// `6E527FC8`): a member that is removed from a group — and possibly re-added
/// later — was flagged as forked, even though nothing diverged.
///
/// Why: when a commit removes *us*, openmls merges only the public diff
/// (`StagedCommitState::PublicState`) — the group context epoch advances and
/// the group becomes inactive, but a removed member cannot derive the new
/// epoch's secrets. The remaining members derive them and publish the real
/// post-commit authenticator to the remote commit log; the removed member's
/// value can never match it.
///
/// This test verifies the fix end to end:
/// 1. The removal is recorded truthfully (`CommitType::RemovedFromGroup`,
///    pre-commit epoch, `applied == last`) instead of claiming the new epoch
///    with a stale authenticator.
/// 2. While removed, fork detection skips the removal row (terminal marker)
///    and reports `Some(false)` based on the consistent earlier history.
/// 3. After re-adding via a new welcome, the `commit_sequence_id == 0` anchor
///    stops comparison at the rejoin boundary and reports `None` (unknown) —
///    never `Some(true)`.
#[cfg_attr(all(feature = "d14n", target_arch = "wasm32"), ignore)]
#[xmtp_common::test(unwrap_try = true)]
async fn test_fork_detection_not_triggered_by_removal_and_readd()
-> Result<(), Box<dyn std::error::Error>> {
    tester!(alix);
    tester!(bo);

    // Alix creates a group and adds Bo; Bo joins and settles.
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;
    bo.sync_welcomes().await?;
    let bo_group = bo.group(&alix_group.group_id)?;
    bo_group.sync().await?;

    // A normal commit Bo applies as an active member, so the commit log has a
    // genuine consensus row before the removal.
    alix_group
        .update_group_name("before removal".to_string())
        .await?;
    bo_group.sync().await?;

    // Alix removes Bo; Bo processes the commit that removes him.
    alix_group.remove_members(&[bo.inbox_id()]).await?;
    bo_group.sync().await?;

    // The removal is recorded truthfully: RemovedFromGroup, Success, and the
    // PRE-commit epoch + authenticator (a removed member cannot derive the
    // new epoch's secrets).
    let bo_logs = bo.context.db().get_group_logs(&bo_group.group_id)?;
    let bo_removal = bo_logs.last().expect("bo logged the removal commit");
    assert_eq!(
        bo_removal.commit_type,
        Some(CommitType::RemovedFromGroup.to_string())
    );
    assert_eq!(bo_removal.commit_result, CommitResult::Success);
    assert_eq!(
        bo_removal.applied_epoch_authenticator, bo_removal.last_epoch_authenticator,
        "a removed member's merge does not advance the authenticator"
    );
    let bo_pre_removal = &bo_logs[bo_logs.len() - 2];
    assert_eq!(
        bo_removal.applied_epoch_number, bo_pre_removal.applied_epoch_number,
        "the removal row records the pre-commit epoch"
    );

    // The remaining member applied the commit fully: new epoch, new
    // authenticator, different from anything Bo could compute.
    let alix_logs = alix.context.db().get_group_logs(&alix_group.group_id)?;
    let alix_removal = alix_logs
        .iter()
        .find(|l| l.commit_sequence_id == bo_removal.commit_sequence_id)
        .expect("alix logged the removal commit");
    assert_eq!(
        alix_removal.applied_epoch_number,
        bo_removal.applied_epoch_number + 1
    );
    assert_ne!(
        alix_removal.applied_epoch_authenticator,
        bo_removal.applied_epoch_authenticator
    );

    // Simulate Bo downloading remote commit log consensus: matching entries
    // for the commits he applied while active, and the remaining members'
    // (true) authenticator for the commit that removed him.
    let mut log_sequence_id = 1000;
    for log in &bo_logs {
        if log.commit_result != CommitResult::Success || log.commit_sequence_id == 0 {
            continue; // welcome rows are never uploaded
        }
        let (epoch, auth) = if log.commit_sequence_id == bo_removal.commit_sequence_id {
            (
                alix_removal.applied_epoch_number,
                alix_removal.applied_epoch_authenticator.clone(),
            )
        } else {
            (
                log.applied_epoch_number,
                log.applied_epoch_authenticator.clone(),
            )
        };
        NewRemoteCommitLog {
            log_sequence_id,
            group_id: bo_group.group_id,
            commit_sequence_id: log.commit_sequence_id,
            commit_result: CommitResult::Success,
            applied_epoch_number: epoch,
            applied_epoch_authenticator: auth,
        }
        .store(&bo.context.db())?;
        log_sequence_id += 1;
    }

    // --- Check 1: while removed. ---
    // The removal row is skipped as a terminal marker; the earlier history
    // matches consensus, so Bo is NOT forked.
    let mut worker = CommitLogWorker::new(bo.context.clone());
    let results = worker
        .run_test(CommitLogTestFunction::CheckForkedState, None)
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    let fork_status = results[0]
        .is_forked
        .as_ref()
        .unwrap()
        .get(bo_group.group_id.as_ref())
        .unwrap();
    assert_eq!(
        *fork_status,
        Some(false),
        "a removed member with consistent prior history must not be reported as forked"
    );

    // --- Check 2: after being re-added. ---
    // Bo's original key package was consumed when he first joined and its
    // rotation is only queued (not yet uploaded) at this point — re-adding
    // with the consumed key package would produce a welcome Bo cannot
    // process. Upload a fresh one first, as would have happened naturally by
    // the time of a real-world re-add.
    bo.rotate_and_upload_key_package().await?;
    alix_group.add_members(&[bo.inbox_id()]).await?;
    bo.sync_welcomes().await?;
    bo_group.sync().await?;
    assert!(
        bo_group.is_active()?,
        "bo must have rejoined via the new welcome"
    );

    let results = worker
        .run_test(CommitLogTestFunction::CheckForkedState, None)
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    let fork_status = results[0]
        .is_forked
        .as_ref()
        .unwrap()
        .get(bo_group.group_id.as_ref())
        .unwrap();
    assert_eq!(
        *fork_status, None,
        "after rejoining via welcome, comparison anchors at the new chain start (unknown)"
    );

    let db_status = bo
        .context
        .db()
        .get_group_commit_log_forked_status(&bo_group.group_id)?;
    assert_eq!(db_status, None, "stored status must not be forked");

    Ok(())
}

/// Regression test for the `EpochAuthenticatorNotAdvanced` merge guard.
///
/// Invariant: a successful staged-commit merge by a member that remains in
/// the group always advances the epoch and therefore changes the epoch
/// authenticator. The only legitimate exception — a commit that removes us —
/// is handled separately (`CommitType::RemovedFromGroup`). If the
/// authenticator does not advance for an active member, the group state the
/// commit was merged onto was corrupt/torn (e.g. a cross-process race on a
/// shared MLS DB: the iOS main app and the Notification Service Extension
/// share one SQLite DB, but `GroupCommitLock` is in-process only).
///
/// This test constructs such a torn state deterministically:
/// 1. Pass 1: process a received commit in a rolled-back transaction and
///    capture the `StagedCommit` + `ValidatedCommit` (what
///    `validate_and_process_external_message` does).
/// 2. "Concurrent process": apply the same commit via a normal `sync()` — the
///    DB is now consistently at epoch E+1.
/// 3. Tear the storage: write the stale epoch-E `GroupContext` (and epoch-E
///    keypair rows, if any) back over the E+1 state, leaving tree / epoch
///    secrets / message secrets at E+1.
/// 4. Reload the group: it reports epoch E but already carries E+1's epoch
///    secrets. Merging the stale staged commit then "succeeds" without
///    advancing the authenticator.
///
/// `merge_staged_commit_logged` must refuse to record that merge: it returns
/// the retryable `EpochAuthenticatorNotAdvanced` error and writes no row. In
/// the real receive path the cursor advance, merge, and log write share one
/// transaction, so the rollback is safe and the message converges via cursor
/// dedup on retry.
#[cfg_attr(all(feature = "d14n", target_arch = "wasm32"), ignore)]
#[xmtp_common::test(unwrap_try = true)]
async fn test_merge_staged_commit_logged_rejects_non_advancing_authenticator()
-> Result<(), Box<dyn std::error::Error>> {
    use diesel::{RunQueryDsl, sql_query};
    use openmls_traits::storage::CURRENT_VERSION;

    // Raw access to the openmls kv store. Rows are located by label prefix +
    // raw group id containment rather than by reconstructing the exact
    // bincode-encoded storage key, so the test does not depend on
    // xmtp_db::sql_key_store's key encoding. Restores use the exact key_bytes
    // returned by the scan.
    const KV_SCAN: &str = "SELECT key_bytes, value_bytes FROM openmls_key_value \
         WHERE substr(key_bytes, 1, ?) = ? AND instr(key_bytes, ?) > 0 AND version = ?";
    const KV_REPLACE: &str =
        "REPLACE INTO openmls_key_value (key_bytes, version, value_bytes) VALUES (?, ?, ?)";
    const GROUP_CONTEXT_LABEL: &[u8] = b"GroupContext";
    const EPOCH_KEY_PAIRS_LABEL: &[u8] = b"EpochKeyPairs";

    #[derive(diesel::QueryableByName)]
    struct KvRow {
        #[diesel(sql_type = diesel::sql_types::Binary)]
        key_bytes: Vec<u8>,
        #[diesel(sql_type = diesel::sql_types::Binary)]
        value_bytes: Vec<u8>,
    }

    /// (key_bytes, value_bytes) pairs from the openmls kv store.
    type KvRows = Vec<(Vec<u8>, Vec<u8>)>;

    fn kv_scan(
        db: &impl xmtp_db::ConnectionExt,
        label: &[u8],
        group_id: &[u8],
    ) -> Result<KvRows, Box<dyn std::error::Error>> {
        let rows: Vec<KvRow> = db.raw_query(|conn| {
            sql_query(KV_SCAN)
                .bind::<diesel::sql_types::Integer, _>(label.len() as i32)
                .bind::<diesel::sql_types::Binary, _>(label)
                .bind::<diesel::sql_types::Binary, _>(group_id)
                .bind::<diesel::sql_types::Integer, _>(CURRENT_VERSION as i32)
                .load(conn)
        })?;
        Ok(rows
            .into_iter()
            .map(|r| (r.key_bytes, r.value_bytes))
            .collect())
    }

    fn kv_replace(
        db: &impl xmtp_db::ConnectionExt,
        storage_key: &[u8],
        value: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        db.raw_query(|conn| {
            sql_query(KV_REPLACE)
                .bind::<diesel::sql_types::Binary, _>(storage_key)
                .bind::<diesel::sql_types::Integer, _>(CURRENT_VERSION as i32)
                .bind::<diesel::sql_types::Binary, _>(value)
                .execute(conn)
        })?;
        Ok(())
    }

    tester!(alix);
    tester!(bo);
    tester!(caro);

    // Alix creates a group and adds Bo.
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;
    bo.sync_welcomes().await?;
    let bo_group = bo.group(&alix_group.group_id)?;
    bo_group.sync().await?;

    // Alix publishes an UpdateGroupMembership commit that Bo has not yet
    // processed.
    alix_group.add_members(&[caro.inbox_id()]).await?;

    // Bo fetches the raw commit envelope from the network.
    let messages = bo
        .mls_store()
        .query_group_messages(bo_group.group_id)
        .await?;
    let commit_envelope = messages
        .into_iter()
        .max_by_key(|m| m.cursor.sequence_id)
        .expect("the add-caro commit must be on the network");
    let commit_sequence_id = commit_envelope.cursor.sequence_id as i64;

    // Bo's group is at epoch E; snapshot the raw epoch-E GroupContext and
    // epoch-keypair kv rows so they can be written back after the sync.
    let mut group_copy =
        OpenMlsGroup::load(bo.context.mls_storage(), &bo_group.group_id.to_openmls())?
            .expect("bo's group must exist");
    let epoch_e = group_copy.epoch();
    let auth_e = group_copy.epoch_authenticator().as_slice().to_vec();

    let db = bo.context.db();
    let ctx_rows_e = kv_scan(&db, GROUP_CONTEXT_LABEL, bo_group.group_id.as_ref())?;
    assert_eq!(
        ctx_rows_e.len(),
        1,
        "expected exactly one GroupContext kv row for the group"
    );
    // A welcome-joined member that has not yet merged a commit has no
    // epoch-keypair row yet — openmls only writes that row in merge_commit
    // (store_epoch_keypairs), and reads of a missing row gracefully return an
    // empty set. Snapshot whatever rows exist (possibly none) so the torn
    // state mirrors Bo's real epoch-E state.
    let ekp_rows_e = kv_scan(&db, EPOCH_KEY_PAIRS_LABEL, bo_group.group_id.as_ref())?;

    // --- Pass 1 (the second process's view; mirrors the receive path). ---
    // Process the commit against a copy of Bo's group inside a transaction
    // that is intentionally rolled back, capturing the staged + validated
    // commit.
    let provider = bo.context.mls_provider();
    let mut processed_message = None;
    let result = provider.key_store().transaction(|conn| {
        let storage = conn.key_store();
        let provider = XmtpOpenMlsProvider::new(storage);
        processed_message = Some(process_message_with_app_data(
            &mut group_copy,
            &provider,
            commit_envelope.message.clone(),
        ));
        Err::<(), StorageError>(StorageError::IntentionalRollback)
    });
    assert!(matches!(result, Err(StorageError::IntentionalRollback)));
    let processed_message = processed_message
        .expect("set in the transaction above")
        .expect("processing the commit at the old epoch succeeds");

    let committer_leaf_index = match processed_message.sender() {
        Sender::Member(idx) => *idx,
        _ => panic!("commit must come from a member"),
    };
    let staged_commit_ref = match processed_message.content() {
        ProcessedMessageContent::StagedCommitMessage(staged) => staged,
        _ => panic!("expected a staged commit message"),
    };
    let validated_commit = ValidatedCommit::from_staged_commit(
        &bo_group.context,
        staged_commit_ref,
        committer_leaf_index,
        &group_copy,
    )
    .await
    .expect("commit validation succeeds");
    let staged_commit = match processed_message.into_content() {
        ProcessedMessageContent::StagedCommitMessage(staged) => *staged,
        _ => unreachable!(),
    };
    drop(group_copy);

    // --- The "concurrent process" applies the same commit normally. ---
    bo_group.sync().await?;

    let consistent = OpenMlsGroup::load(bo.context.mls_storage(), &bo_group.group_id.to_openmls())?
        .expect("bo's group must exist");
    assert_eq!(consistent.epoch(), staged_commit.group_context().epoch());
    let auth_e1 = consistent.epoch_authenticator().as_slice().to_vec();
    assert_ne!(
        auth_e, auth_e1,
        "the applied commit advanced the authenticator"
    );
    drop(consistent);

    // --- Tear the storage. ---
    // Write the stale epoch-E GroupContext (and any epoch-E keypair rows)
    // back over the E+1 state, leaving tree / epoch secrets / message secrets
    // at E+1. This simulates a second process's interleaved writes landing on
    // the shared kv store without cross-process mutual exclusion.
    for (key, value) in ctx_rows_e.iter().chain(ekp_rows_e.iter()) {
        kv_replace(&db, key, value)?;
    }

    // --- The reload hands back the torn group: it reports epoch E, but its
    // epoch secrets (and therefore its authenticator) are already E+1's.
    let mut torn = OpenMlsGroup::load(bo.context.mls_storage(), &bo_group.group_id.to_openmls())?
        .expect("bo's group must exist");
    assert_eq!(torn.epoch(), epoch_e, "torn group reports the stale epoch");
    assert_eq!(
        torn.epoch_authenticator().as_slice(),
        auth_e1.as_slice(),
        "torn group already carries the post-commit epoch secrets"
    );

    let logs_before = bo.context.db().get_group_logs(&bo_group.group_id)?.len();

    // --- Pass 2: merge the stale staged commit on the torn group. ---
    // From the torn group's perspective this is an ordinary merge (epoch E ->
    // E+1), so openmls accepts it — but the authenticator cannot advance.
    let merge_result = torn.merge_staged_commit_logged(
        &provider,
        staged_commit,
        &validated_commit,
        commit_sequence_id,
    );

    assert!(
        merge_result.is_err(),
        "merge_staged_commit_logged must refuse to log a Success commit whose \
         merge did not advance the epoch authenticator (applied == last); \
         persisting it would corrupt the local commit log"
    );

    // And the corrupt row must not have been written.
    let logs = bo.context.db().get_group_logs(&bo_group.group_id)?;
    assert_eq!(
        logs.len(),
        logs_before,
        "no local commit log entry may be written for the rejected merge"
    );
    for log in &logs {
        if log.commit_result == CommitResult::Success
            && !log.last_epoch_authenticator.is_empty()
            && log.commit_type != Some(CommitType::RemovedFromGroup.to_string())
        {
            assert_ne!(
                log.applied_epoch_authenticator, log.last_epoch_authenticator,
                "successful commit at applied_epoch_number={} recorded an epoch \
                 authenticator that did not advance",
                log.applied_epoch_number
            );
        }
    }

    Ok(())
}
