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

/// Regression test for the corrupt commit-log write observed in production
/// (Convos group `e08a717548dc996845dc1f2d6986bdd0`, debug bundle `6E527FC8`).
///
/// The local commit log on the "forked" member contained a *successful*
/// `UpdateGroupMembership` entry whose `applied_epoch_authenticator` equals its
/// `last_epoch_authenticator`. For a successful staged-commit merge this is
/// impossible: merging a commit always advances the epoch and therefore changes
/// the epoch authenticator. `applied == last` is only valid for *failed*
/// commits (`mark_failed_commit_logged`, which records that nothing was
/// applied).
///
/// Root cause (see the commit-log fork investigation): the iOS main app and the
/// Notification Service Extension share one MLS SQLite DB, but `GroupCommitLock`
/// is in-process only — the two processes never contend. With both processes
/// running the receive path's "process twice + reload" pattern
/// (`validate_and_process_external_message`), interleaved per-key writes to the
/// shared `openmls_key_value` store can leave a *torn* group state: some keys
/// from the new epoch, some stale. A group loaded from such state merges a
/// staged commit "successfully" without its authenticator advancing, and the
/// single commit-log write path, `merge_staged_commit_logged`, faithfully
/// records the corrupt `Success` row.
///
/// Note: the *cleanly* advanced case — where the concurrent writer fully
/// applied the commit and the stale staged commit is merged on top of a
/// consistent new-epoch group — is already (incidentally) rejected by openmls'
/// keypair bookkeeping in `merge_commit` ("We should have all the private key
/// material we need"). The torn case below is NOT rejected by anything today.
///
/// This test constructs the torn state deterministically:
/// 1. Pass 1: process Bo's received commit in a rolled-back transaction and
///    capture the `StagedCommit` + `ValidatedCommit` (what
///    `validate_and_process_external_message` does at `mls_sync.rs:1092`).
/// 2. Snapshot the raw `GroupContext` and epoch-keypair kv rows (epoch E).
/// 3. "Concurrent process": apply the same commit via a normal `sync()` — the
///    DB is now consistently at epoch E+1.
/// 4. Tear the storage: write the stale epoch-E `GroupContext` and epoch-E
///    keypair rows back, leaving tree/epoch-secrets/message-secrets at E+1.
///    This simulates one process's writes landing on top of the other's.
/// 5. Reload the group: it reports epoch E but carries E+1's epoch secrets —
///    i.e. its authenticator is already the post-commit one.
/// 6. Pass 2: merge the stale staged commit via `merge_staged_commit_logged`.
///    The merge succeeds (epoch E -> E+1, keypair accounting balances), but the
///    authenticator does not advance: last == applied == auth(E+1).
///
/// EXPECTED (once the invariant guard lands in `commit_log_storer.rs`):
/// `merge_staged_commit_logged` must refuse to write a `Success` entry whose
/// applied authenticator equals the last authenticator, and return an error
/// instead. Erroring is safe: in the real receive path the cursor advance,
/// merge, and log write share one transaction (`mls_sync.rs:1297`), so the
/// error rolls everything back and the message is retried, converging via
/// cursor dedup.
///
/// CURRENT (bug): the merge succeeds and the corrupt row is written — this
/// test FAILS until the guard is added. Our production logs prove this class
/// of write happens in the wild.
#[cfg_attr(all(feature = "d14n", target_arch = "wasm32"), ignore)]
#[xmtp_common::test(unwrap_try = true)]
async fn test_merge_staged_commit_logged_rejects_non_advancing_authenticator()
-> Result<(), Box<dyn std::error::Error>> {
    use diesel::{RunQueryDsl, sql_query};
    use openmls_traits::storage::CURRENT_VERSION;

    // Raw access to the openmls kv store, mirroring xmtp_db::sql_key_store's
    // key layout: storage_key = label ++ bincode(inner_key) ++ version (u16 BE).
    const KV_SELECT: &str =
        "SELECT value_bytes FROM openmls_key_value WHERE key_bytes = ? AND version = ?";
    const KV_REPLACE: &str =
        "REPLACE INTO openmls_key_value (key_bytes, version, value_bytes) VALUES (?, ?, ?)";
    const GROUP_CONTEXT_LABEL: &[u8] = b"GroupContext";
    const EPOCH_KEY_PAIRS_LABEL: &[u8] = b"EpochKeyPairs";

    #[derive(diesel::QueryableByName)]
    struct KvRow {
        #[diesel(sql_type = diesel::sql_types::Binary)]
        value_bytes: Vec<u8>,
    }

    fn kv_storage_key(label: &[u8], inner_key: &[u8]) -> Vec<u8> {
        let mut key = label.to_vec();
        key.extend_from_slice(inner_key);
        key.extend_from_slice(&(CURRENT_VERSION).to_be_bytes());
        key
    }

    fn kv_select(
        db: &impl xmtp_db::ConnectionExt,
        storage_key: &[u8],
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let rows: Vec<KvRow> = db.raw_query(|conn| {
            sql_query(KV_SELECT)
                .bind::<diesel::sql_types::Binary, _>(storage_key)
                .bind::<diesel::sql_types::Integer, _>(CURRENT_VERSION as i32)
                .load(conn)
        })?;
        Ok(rows
            .into_iter()
            .next()
            .expect("kv row must exist")
            .value_bytes)
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

    // Alix publishes an UpdateGroupMembership commit (same commit type as the
    // production bundle) that Bo has not yet processed.
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

    // Bo's group is at epoch E; compute the kv storage keys for the epoch-E
    // GroupContext and epoch-keypair rows so they can be snapshotted.
    let mut group_copy =
        OpenMlsGroup::load(bo.context.mls_storage(), &bo_group.group_id.to_openmls())?
            .expect("bo's group must exist");
    let epoch_e = group_copy.epoch();
    let auth_e = group_copy.epoch_authenticator().as_slice().to_vec();
    let gid_ser = bincode::serialize(&bo_group.group_id.to_openmls())?;
    let ctx_key = kv_storage_key(GROUP_CONTEXT_LABEL, &gid_ser);
    let mut ekp_inner = gid_ser.clone();
    ekp_inner.extend_from_slice(&bincode::serialize(&epoch_e)?);
    ekp_inner.extend_from_slice(&bincode::serialize(&group_copy.own_leaf_index().u32())?);
    let ekp_key = kv_storage_key(EPOCH_KEY_PAIRS_LABEL, &ekp_inner);

    let db = bo.context.db();
    let ctx_e_bytes = kv_select(&db, &ctx_key)?;
    let ekp_e_bytes = kv_select(&db, &ekp_key)?;

    // --- Pass 1 (the second process's view; mirrors mls_sync.rs:1092). ---
    // Process the commit against a copy of Bo's group inside a transaction that
    // is intentionally rolled back, capturing the staged + validated commit.
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
    // In production: the main app processes the commit while the NSE sits
    // between its pass 1 and its reload(); the race exists because
    // GroupCommitLock is per-process. The DB is now consistently at E+1.
    bo_group.sync().await?;

    let consistent =
        OpenMlsGroup::load(bo.context.mls_storage(), &bo_group.group_id.to_openmls())?
            .expect("bo's group must exist");
    assert_eq!(consistent.epoch(), staged_commit.group_context().epoch());
    let auth_e1 = consistent.epoch_authenticator().as_slice().to_vec();
    assert_ne!(auth_e, auth_e1, "the applied commit advanced the authenticator");
    drop(consistent);

    // --- Tear the storage. ---
    // Write the stale epoch-E GroupContext and epoch-E keypair rows back over
    // the E+1 state, leaving tree / epoch secrets / message secrets at E+1.
    // This simulates the second process's interleaved writes landing on the
    // shared kv store without cross-process mutual exclusion.
    kv_replace(&db, &ctx_key, &ctx_e_bytes)?;
    kv_replace(&db, &ekp_key, &ekp_e_bytes)?;

    // --- The reload (mls_sync.rs:1109 analog) hands back the torn group: ---
    // it reports epoch E, but its epoch secrets (and therefore its
    // authenticator) are already E+1's.
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
    // From the torn group's perspective this is a perfectly ordinary merge
    // (epoch E -> E+1), so openmls accepts it. But the authenticator read
    // before the merge already equals the staged commit's: the logged row is
    // Success with applied_epoch_authenticator == last_epoch_authenticator —
    // the exact corruption from bundle 6E527FC8, which fork detection later
    // (correctly) flags against remote consensus.
    let merge_result = torn.merge_staged_commit_logged(
        &provider,
        staged_commit,
        &validated_commit,
        commit_sequence_id,
    );

    // INVARIANT (Fix 1): a Success merge must advance the epoch authenticator.
    // merge_staged_commit_logged must detect applied == last and return an
    // error instead of persisting the corrupt row.
    assert!(
        merge_result.is_err(),
        "merge_staged_commit_logged should refuse to log a Success commit whose \
         merge did not advance the epoch authenticator (applied == last); \
         persisting it corrupts the local commit log and later surfaces as a fork"
    );

    // And the corrupt row must not have been written.
    let logs = bo.context.db().get_group_logs(&bo_group.group_id)?;
    assert_eq!(
        logs.len(),
        logs_before,
        "no local commit log entry may be written for the rejected merge"
    );
    for log in &logs {
        if log.commit_result == CommitResult::Success && !log.last_epoch_authenticator.is_empty() {
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
